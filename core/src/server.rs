use arc_swap::ArcSwap;
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::utils::{log::LOGGER, ports, ring_buffer::RingBuffer};
use crate::value::Value;
use crate::websocket;
use crate::websocket::protocol::MAX_TOPICS;
use crate::websocket::server::{ControlHandler, DEFAULT_BIND_HOST, ValueSink, loopback_for};
use tarwyn_protobuf::telemetry;
use tarwyn_protobuf::telemetry::{now_micros, now_nanos};

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurve, CompareAndSetCommand, Reply, ReplyCompareAndSetCommand, ReplyDataCommand,
    ReplyDeleteCommand, ReplyJsonCommand, ReplyLogsCommand, ReplyPingCommand,
    ReplyStatisticsCommand, ReplyTablesCommand, Request, SupportedValues, reply, request,
    supported_values,
};

const TELEMETRY_TTL: Duration = Duration::from_secs(10);
/// Addresses one telemetry channel will relay to, capping how far a spoofed
/// sender can amplify one datagram.
const MAX_TELEMETRY_SUBSCRIBERS: usize = 16;
/// Channels the relay will track registrations for at once.
const MAX_TELEMETRY_CHANNELS: usize = 256;
/// Values the read cache keeps per channel. Reads only ever want the newest.
const CHANNEL_HISTORY: usize = 1;
/// How long a receive loop waits before checking the stop flag anyway, in
/// case the wake on stop is lost.
const STOP_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The WebSocket topic subscribe_to_logs listens on.
const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";

/// The tarwyn server: the value map, the NT4 WebSocket and the UDP telemetry
/// plane. Nothing listens until [`start`](Self::start).
pub struct Server {
    websocket: Arc<websocket::Server>,
    telemetry_subscribers: Arc<ArcSwap<HashMap<u32, Vec<SocketAddr>>>>,
    telemetry_registry: Arc<Mutex<HashMap<u32, HashMap<SocketAddr, Instant>>>>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    started: Instant,
    telemetry_socket: Arc<UdpSocket>,
    telemetry_port: u16,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

/// Why a server could not take the ports it was asked for.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// The WebSocket port could not be bound.
    #[error("could not bind the WebSocket server to port {port}")]
    WebsocketBind {
        /// The port it was asked for.
        port: u16,
        /// The underlying OS error.
        source: std::io::Error,
    },
    /// The UDP telemetry port could not be bound.
    #[error("could not bind the telemetry socket to UDP port {port}")]
    Telemetry {
        /// The port it was asked for.
        port: u16,
        /// The underlying OS error.
        source: std::io::Error,
    },
}

/// Wait for every loop to exit, skipping the calling thread if it is one of them.
fn join_running(threads: &Mutex<Vec<std::thread::JoinHandle<()>>>) {
    let handles = match threads.lock() {
        Ok(mut threads) => std::mem::take(&mut *threads),
        Err(_) => return,
    };
    let current = std::thread::current().id();
    for handle in handles {
        if handle.thread().id() != current {
            let _ = handle.join();
        }
    }
}

impl Server {
    /// Bind on the default ports.
    pub fn new() -> Self {
        Self::with_ports(
            ports::DEFAULT_WEBSOCKET_PORT,
            telemetry::DEFAULT_TELEMETRY_PORT,
        )
    }

    /// Bind the WebSocket plane on `port` and the telemetry plane on
    /// `telemetry_port`.
    ///
    /// # Panics
    ///
    /// Panics if a port cannot be bound. [`try_with_ports`](Self::try_with_ports)
    /// returns the error instead.
    pub fn with_ports(port: u16, telemetry_port: u16) -> Self {
        Self::try_with_ports(port, telemetry_port).expect("could not bind the tarwyn server")
    }

    /// As [`new`](Self::new), reporting a failed bind instead of panicking.
    pub fn try_new() -> Result<Self, BindError> {
        Self::try_with_ports(
            ports::DEFAULT_WEBSOCKET_PORT,
            telemetry::DEFAULT_TELEMETRY_PORT,
        )
    }

    /// As [`with_ports`](Self::with_ports), but returns a failed bind as an
    /// error. The WebSocket port is retried for about a second.
    pub fn try_with_ports(port: u16, telemetry_port: u16) -> Result<Self, BindError> {
        Self::try_with_bind(DEFAULT_BIND_HOST, port, telemetry_port)
    }

    /// As [`try_with_ports`](Self::try_with_ports), with both planes bound to
    /// `host`. `127.0.0.1` keeps the server off the network.
    pub fn try_with_bind(host: &str, port: u16, telemetry_port: u16) -> Result<Self, BindError> {
        let cached_messages = Arc::new(Mutex::new(HashMap::new()));
        let telemetry_subscribers = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let telemetry_registry = Arc::new(Mutex::new(HashMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));
        let started = Instant::now();

        // Weak: the WebSocket server owns the handler, so a strong reference would never drop.
        let websocket_slot = Arc::new(Mutex::new(None::<Weak<websocket::Server>>));

        let control_handler: ControlHandler = {
            let cached_messages = cached_messages.clone();
            let telemetry_subscribers = telemetry_subscribers.clone();
            let websocket_slot = websocket_slot.clone();
            Arc::new(move |payload: &[u8]| -> Option<Vec<u8>> {
                let request = Request::decode(payload).ok()?;
                let id = request.id;
                let request_payload = request.payload?;
                let reply = match request_payload {
                    request::Payload::Data(command) => {
                        let data = match cached_messages.lock() {
                            Ok(cached) => Server::read(&cached, &command.channel).cloned(),
                            Err(_) => None,
                        };
                        Server::data_reply(id, data)
                    }
                    request::Payload::Delete(command) => {
                        let deleted = match cached_messages.lock() {
                            Ok(mut cached) => {
                                if command.channel.is_empty() {
                                    let count = cached.len();
                                    cached.clear();
                                    count
                                } else {
                                    usize::from(cached.remove(&command.channel).is_some())
                                }
                            }
                            Err(_) => 0,
                        };
                        Reply {
                            id,
                            payload: Some(reply::Payload::Delete(ReplyDeleteCommand {
                                deleted: deleted as u32,
                            })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::Tables(command) => {
                        let channels = match cached_messages.lock() {
                            Ok(cached) => {
                                let mut names: Vec<String> = cached
                                    .keys()
                                    .filter(|name| name.starts_with(&command.prefix))
                                    .cloned()
                                    .collect();
                                names.sort();
                                names
                            }
                            Err(_) => Vec::new(),
                        };
                        Reply {
                            id,
                            payload: Some(reply::Payload::Tables(ReplyTablesCommand { channels })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::Ping(command) => Reply {
                        id,
                        payload: Some(reply::Payload::Ping(ReplyPingCommand {
                            sent_nanos: command.sent_nanos,
                            server_nanos: now_nanos(),
                        })),
                    }
                    .encode_to_vec(),
                    request::Payload::Statistics(_) => {
                        let (channels, values) = match cached_messages.lock() {
                            Ok(cached) => (
                                cached.len() as u64,
                                cached.values().map(|ring| ring.len() as u64).sum(),
                            ),
                            Err(_) => (0, 0),
                        };
                        let subscribers = telemetry_subscribers
                            .load()
                            .values()
                            .map(|addresses: &Vec<SocketAddr>| addresses.len() as u64)
                            .sum();
                        let dropped_publishes = websocket_slot
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .as_ref()
                            .and_then(|websocket| websocket.upgrade())
                            .map(|websocket| websocket.dropped_publishes())
                            .unwrap_or(0);
                        Reply {
                            id,
                            payload: Some(reply::Payload::Statistics(ReplyStatisticsCommand {
                                channels,
                                values,
                                telemetry_subscribers: subscribers,
                                uptime_seconds: started.elapsed().as_secs(),
                                version: env!("CARGO_PKG_VERSION").to_string(),
                                dropped_publishes,
                                dropped_logs: LOGGER.dropped(),
                            })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::Json(command) => {
                        let json = match cached_messages.lock() {
                            Ok(cached) => Server::to_json(&cached, &command.prefix),
                            Err(_) => String::from("{}"),
                        };
                        Reply {
                            id,
                            payload: Some(reply::Payload::Json(ReplyJsonCommand { json })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::CompareAndSet(command) => {
                        let channel = command.channel.clone();
                        let mut outcome = (false, None);
                        let swap = || {
                            let mut cached =
                                cached_messages.lock().unwrap_or_else(|p| p.into_inner());
                            outcome = Server::compare_and_set(&mut cached, command);
                            match &outcome {
                                (true, Some(kind)) => Some(Value::from(kind.clone())),
                                _ => None,
                            }
                        };
                        let websocket = websocket_slot
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .as_ref()
                            .and_then(|websocket| websocket.upgrade());
                        match websocket {
                            Some(websocket) => {
                                websocket.fan_out_upsert_with(&channel, now_micros(), swap)
                            }
                            None => {
                                swap();
                            }
                        }
                        let (swapped, current) = outcome;
                        Reply {
                            id,
                            payload: Some(reply::Payload::CompareAndSet(
                                ReplyCompareAndSetCommand {
                                    swapped,
                                    current: current
                                        .map(|kind| Box::new(SupportedValues { kind: Some(kind) })),
                                },
                            )),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::Logs(_) => {
                        let logs = LOGGER.get_logs().unwrap_or_default();
                        Reply {
                            id,
                            payload: Some(reply::Payload::Logs(ReplyLogsCommand { logs })),
                        }
                        .encode_to_vec()
                    }
                };
                Some(reply)
            })
        };

        let value_sink: ValueSink = {
            Arc::new(move |name: &str, value: &Value| {
                let Ok(mut cached) = cached_messages.lock() else {
                    return;
                };
                let kind = supported_values::Kind::from(value.clone());
                if let Some(ring) = cached.get_mut(name) {
                    ring.push(kind);
                    return;
                }
                if cached.len() >= MAX_TOPICS {
                    return;
                }
                let mut ring = RingBuffer::new(CHANNEL_HISTORY);
                ring.push(kind);
                cached.insert(name.to_string(), ring);
            })
        };

        let websocket = Arc::new(
            websocket::Server::bind_with_handler(host, port, control_handler, value_sink)
                .map_err(|source| BindError::WebsocketBind { port, source })?,
        );
        *websocket_slot.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Arc::downgrade(&websocket));

        let telemetry_socket =
            UdpSocket::bind((host, telemetry_port)).map_err(|source| BindError::Telemetry {
                port: telemetry_port,
                source,
            })?;
        telemetry::tune(&telemetry_socket);
        let _ = telemetry_socket.set_read_timeout(Some(STOP_CHECK_INTERVAL));

        Ok(Server {
            websocket,
            telemetry_subscribers,
            telemetry_registry,
            stop,
            initialized,
            started,
            telemetry_socket: Arc::new(telemetry_socket),
            telemetry_port,
            threads: Mutex::new(Vec::new()),
        })
    }

    /// The address the WebSocket plane is bound to, for a caller that asked
    /// for port 0 and needs the one the OS picked.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.websocket.local_addr()
    }

    /// The address the telemetry plane is bound to.
    pub fn telemetry_addr(&self) -> std::io::Result<SocketAddr> {
        self.telemetry_socket.local_addr()
    }

    /// How many fan-out frames were dropped because a subscriber's queue was
    /// full. The values are still stored and readable.
    pub fn dropped_publishes(&self) -> u64 {
        self.websocket.dropped_publishes()
    }

    fn track(&self, handle: std::thread::JoinHandle<()>) {
        if let Ok(mut threads) = self.threads.lock() {
            threads.push(handle);
        }
    }

    fn read<'a>(
        cached: &'a HashMap<String, RingBuffer<supported_values::Kind>>,
        channel: &str,
    ) -> Option<&'a supported_values::Kind> {
        cached.get(channel)?.peek()
    }

    fn compare_and_set(
        cached: &mut HashMap<String, RingBuffer<supported_values::Kind>>,
        command: CompareAndSetCommand,
    ) -> (bool, Option<supported_values::Kind>) {
        let current = Self::read(cached, &command.channel);

        let matches = if command.expect_absent {
            current.is_none()
        } else {
            match (current, command.expected.and_then(|value| value.kind)) {
                (Some(current), Some(expected)) => *current == expected,
                _ => false,
            }
        };

        if !matches {
            return (false, current.cloned());
        }

        let Some(kind) = command.value.and_then(|value| value.kind) else {
            return (false, current.cloned());
        };
        if current.is_none() && cached.len() >= MAX_TOPICS {
            return (false, None);
        }
        cached
            .entry(command.channel)
            .or_insert_with(|| RingBuffer::new(CHANNEL_HISTORY))
            .push(kind.clone());
        (true, Some(kind))
    }

    /// The JSON a value renders as: scalars and lists as themselves, bytes as
    /// hex, points as `{"x", "y"}` objects with the heading when it has one.
    fn json_value(kind: &supported_values::Kind) -> serde_json::Value {
        use serde_json::{Value as Json, json};
        use supported_values::Kind;
        let hex = |bytes: &[u8]| Json::String(bytes.iter().map(|b| format!("{b:02x}")).collect());
        let curve = |curve: &BezierCurve| -> Json {
            curve
                .control_points
                .iter()
                .map(|point| {
                    let mut object = json!({ "x": point.x, "y": point.y });
                    if let Some(degrees) = point.rotation_degrees {
                        object["rotationDegrees"] = json!(degrees);
                    }
                    object
                })
                .collect()
        };
        match kind {
            Kind::String(v) => json!(v),
            Kind::Int32(v) => json!(v),
            Kind::Int64(v) => json!(v),
            Kind::Uint32(v) => json!(v),
            Kind::Uint64(v) => json!(v),
            Kind::Bool(v) => json!(v),
            Kind::Double(v) => json!(v),
            Kind::Float(v) => json!(f64::from(*v)),
            Kind::Bytes(v) => hex(v),
            Kind::StringList(list) => json!(list.values),
            Kind::BytesList(list) => list.values.iter().map(|v| hex(v)).collect(),
            Kind::BoolList(list) => json!(list.values),
            Kind::FloatList(list) => json!(list.values),
            Kind::DoubleList(list) => json!(list.values),
            Kind::IntegerList(list) => json!(list.values),
            Kind::LongList(list) => json!(list.values),
            Kind::CoordinateList(list) => list
                .coordinates
                .iter()
                .map(|c| json!({ "x": c.x, "y": c.y }))
                .collect(),
            Kind::BezierCurve(c) => curve(c),
            Kind::BezierCurves(curves) => curves.curves.iter().map(curve).collect(),
            Kind::BezierCurvesList(list) => list
                .values
                .iter()
                .map(|curves| curves.curves.iter().map(curve).collect::<Json>())
                .collect(),
        }
    }

    /// Every channel under `prefix` and its current value, as one JSON object
    /// with the channel names in sorted order, which `serde_json`'s map gives
    /// for free and callers rely on to diff two documents.
    fn to_json(
        cached: &HashMap<String, RingBuffer<supported_values::Kind>>,
        prefix: &str,
    ) -> String {
        let document: serde_json::Map<String, serde_json::Value> = cached
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .filter_map(|(name, ring)| Some((name.clone(), Self::json_value(ring.peek()?))))
            .collect();
        serde_json::Value::Object(document).to_string()
    }

    fn data_reply(id: u64, data: Option<supported_values::Kind>) -> Vec<u8> {
        let kind =
            data.unwrap_or_else(|| supported_values::Kind::String(String::from(NO_DATA_SENTINEL)));

        Reply {
            id,
            payload: Some(reply::Payload::Data(ReplyDataCommand {
                value: Some(SupportedValues { kind: Some(kind) }),
            })),
        }
        .encode_to_vec()
    }

    /// Sets how long each connection's reader spins before it blocks, where
    /// zero, the default, blocks at once. See
    /// [`websocket::Server::set_busy_poll`](crate::websocket::Server::set_busy_poll).
    pub fn set_busy_poll(&self, window: Duration) {
        self.websocket.set_busy_poll(window);
    }

    /// Sets how far around a predicted arrival each connection's reader spins.
    /// See [`websocket::Server::set_predict`](crate::websocket::Server::set_predict).
    pub fn set_predict(&self, margin: Duration) {
        self.websocket.set_predict(margin);
    }

    /// Bind the sockets and start the receive loops.
    ///
    /// Resumes after [`stop`](Self::stop), and does nothing while running.
    /// Every request gets a reply, including a malformed one.
    pub fn start(&self) {
        if !self.initialized.swap(true, Ordering::SeqCst) {
            info!("Initializing tarwyn server...");
        } else if self.stop.swap(false, Ordering::SeqCst) {
            info!("Starting tarwyn server...");
        } else {
            info!("tarwyn server is already running.");
            return;
        }

        self.websocket.stop_flag().store(false, Ordering::SeqCst);
        self.track(self.websocket.start());

        self.start_telemetry_relay();
        self.start_log_relay();
    }

    /// Route `channel_hash` to `address`, after sweeping expired leases.
    ///
    /// Refreshing an existing lease always succeeds, while a new address can be
    /// turned away when the channel or registry is full. Returns whether
    /// `address` is registered afterwards.
    fn register_telemetry(
        registry: &Mutex<HashMap<u32, HashMap<SocketAddr, Instant>>>,
        published: &ArcSwap<HashMap<u32, Vec<SocketAddr>>>,
        channel_hash: u32,
        address: SocketAddr,
    ) -> bool {
        let Ok(mut registry) = registry.lock() else {
            return false;
        };
        let now = Instant::now();
        for addresses in registry.values_mut() {
            addresses.retain(|_, seen| now.duration_since(*seen) < TELEMETRY_TTL);
        }
        registry.retain(|_, addresses| !addresses.is_empty());

        let known = registry.contains_key(&channel_hash);
        if !known && registry.len() >= MAX_TELEMETRY_CHANNELS {
            return false;
        }
        let addresses = registry.entry(channel_hash).or_default();
        if !addresses.contains_key(&address) && addresses.len() >= MAX_TELEMETRY_SUBSCRIBERS {
            registry.retain(|_, addresses| !addresses.is_empty());
            return false;
        }
        addresses.insert(address, now);

        let snapshot: HashMap<u32, Vec<SocketAddr>> = registry
            .iter()
            .map(|(hash, addresses)| (*hash, addresses.keys().copied().collect()))
            .collect();
        published.store(Arc::new(snapshot));
        true
    }

    /// Binds the telemetry port and relays what arrives on it.
    ///
    /// A registration routes a channel to its sender. A data datagram is copied
    /// to every registered address except the relay's own port.
    fn start_telemetry_relay(&self) {
        let subscribers = self.telemetry_subscribers.clone();
        let registry = self.telemetry_registry.clone();
        let stop = self.stop.clone();
        let socket = self.telemetry_socket.clone();

        let handle = std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            let own = socket.local_addr().ok();
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok((len, from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                if let Some(channel_hash) = telemetry::decode_registration(&buf[..len]) {
                    Self::register_telemetry(&registry, &subscribers, channel_hash, from);
                    continue;
                }
                let Some((channel_hash, _timestamp, _payload)) = telemetry::decode(&buf[..len])
                else {
                    continue;
                };
                let routes = subscribers.load();
                let Some(targets) = routes.get(&channel_hash) else {
                    continue;
                };
                for target in targets {
                    if own.is_some_and(|own| own.port() == target.port()) {
                        continue;
                    }
                    let _ = socket.send_to(&buf[..len], target);
                }
            }
        });
        self.track(handle);
    }

    /// Relays new log lines onto the WebSocket log topic, blocking on the
    /// logger in between.
    fn start_log_relay(&self) {
        let websocket = self.websocket.clone();
        let stop = self.stop.clone();

        let handle = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if let Some(logs) =
                    crate::utils::log::LOGGER.wait_unread_logs(&stop, STOP_CHECK_INTERVAL)
                {
                    websocket.fan_out_upsert(LOG_TOPIC, &Value::StringArray(logs), now_micros());
                }
            }
        });
        self.track(handle);
    }

    /// Stop the receive loops and wait for them to exit. Cached values
    /// survive the next [`start`](Self::start).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.websocket.stop();
        crate::utils::log::LOGGER.wake();
        if let Ok(local) = self.telemetry_socket.local_addr() {
            let loopback = SocketAddr::new(loopback_for(local), local.port());
            let _ = self.telemetry_socket.send_to(&[], loopback);
        }
        join_running(&self.threads);
        info!("tarwyn server has been stopped.");
    }
}

mod convert;

impl std::fmt::Debug for Server {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Server")
            .field("telemetry_port", &self.telemetry_port)
            .field("running", &!self.stop.load(Ordering::SeqCst))
            .field("uptime", &self.started.elapsed())
            .finish_non_exhaustive()
    }
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

/// Stops the loops, so a server that goes out of scope does not leave its
/// threads holding the ports.
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod telemetry_registration_tests;
