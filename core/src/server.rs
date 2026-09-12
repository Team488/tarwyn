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
use crate::websocket::server::{ControlHandler, DEFAULT_BIND_HOST, ValueSink};
use tarwyn_protobuf::telemetry;

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurve, CompareAndSetCommand, Reply, ReplyCompareAndSetCommand, ReplyDataCommand,
    ReplyDeleteCommand, ReplyJsonCommand, ReplyLogsCommand, ReplyPingCommand,
    ReplyStatisticsCommand, ReplyTablesCommand, Request, SupportedValues, reply, request,
    supported_values,
};

const TELEMETRY_TTL: Duration = Duration::from_secs(10);
/// Addresses one telemetry channel will relay to.
///
/// The relay copies every datagram to every registered address, so this number
/// is the amplification factor a single sender can buy. A UDP source address
/// cannot be verified, so without a cap one host registering from many source
/// ports (or spoofing others) turns the port into an amplifier. A robot's real
/// subscribers are the driver station and a few coprocessors.
const MAX_TELEMETRY_SUBSCRIBERS: usize = 16;
/// Channels the relay will track registrations for at once.
const MAX_TELEMETRY_CHANNELS: usize = 256;
/// Values retained per channel, so a late subscriber sees recent history.
const CHANNEL_HISTORY: usize = 100;
/// How long a receive loop sleeps before it looks at the stop flag again.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The WebSocket topic subscribe_to_logs listens on.
const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";

const DEFAULT_REP_PORT: u16 = ports::DEFAULT_WEBSOCKET_PORT;
const DEFAULT_PUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PULL_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

/// The tarwyn server: the value map, and the sockets that serve it.
///
/// One NT4 server carries the reliable traffic, value publishes and the
/// control plane, alongside a UDP socket for the telemetry plane. Nothing
/// is bound until [`start`](Self::start).
///
/// The server answers reads rather than forwarding them: it owns the table, so a
/// read is one round trip, not two.
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
    ///
    /// Reported rather than swallowed: a server that silently came up without its
    /// telemetry plane looks healthy from every angle except the one where the
    /// datagrams were supposed to arrive.
    #[error("could not bind the telemetry socket to UDP port {port}")]
    Telemetry {
        /// The port it was asked for.
        port: u16,
        /// The underlying OS error.
        source: std::io::Error,
    },
}

/// Wait for every loop to exit, skipping the calling thread if it is one of them.
///
/// A loop can reach this by dropping the last handle to what it is serving, and
/// a thread that joined itself would wait forever.
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
        Self::with_ports(DEFAULT_PUB_PORT, DEFAULT_PULL_PORT, DEFAULT_REP_PORT)
    }

    /// Bind on the given ZeroMQ ports, with telemetry on its default port.
    ///
    /// # Panics
    ///
    /// If a socket cannot be created or a port cannot be bound.
    pub fn with_ports(pub_port: u16, pull_port: u16, rep_port: u16) -> Self {
        Self::with_ports_and_telemetry(
            pub_port,
            pull_port,
            rep_port,
            telemetry::DEFAULT_TELEMETRY_PORT,
        )
    }

    /// Bind on all four ports, telemetry included.
    ///
    /// The telemetry port is what stops two servers sharing a host, so it has to
    /// move for the second one.
    ///
    /// # Panics
    ///
    /// If a socket cannot be created or a port cannot be bound. Use
    /// [`try_with_ports_and_telemetry`](Self::try_with_ports_and_telemetry) to
    /// handle that instead.
    pub fn with_ports_and_telemetry(
        pub_port: u16,
        pull_port: u16,
        rep_port: u16,
        telemetry_port: u16,
    ) -> Self {
        Self::try_with_ports_and_telemetry(pub_port, pull_port, rep_port, telemetry_port)
            .expect("could not bind the tarwyn server")
    }

    /// As [`new`](Self::new), reporting a failed bind instead of panicking.
    pub fn try_new() -> Result<Self, BindError> {
        Self::try_with_ports_and_telemetry(
            DEFAULT_PUB_PORT,
            DEFAULT_PULL_PORT,
            DEFAULT_REP_PORT,
            telemetry::DEFAULT_TELEMETRY_PORT,
        )
    }

    /// As [`with_ports_and_telemetry`](Self::with_ports_and_telemetry), reporting
    /// a failed bind instead of panicking.
    ///
    /// The WebSocket port is retried for about a second before it is given up on, so a
    /// port held by something on its way out does not stop the server starting.
    pub fn try_with_ports_and_telemetry(
        pub_port: u16,
        pull_port: u16,
        rep_port: u16,
        telemetry_port: u16,
    ) -> Result<Self, BindError> {
        Self::try_with_bind(
            DEFAULT_BIND_HOST,
            pub_port,
            pull_port,
            rep_port,
            telemetry_port,
        )
    }

    /// As [`try_with_ports_and_telemetry`](Self::try_with_ports_and_telemetry),
    /// with the address the WebSocket plane listens on spelled out.
    ///
    /// Narrow this to `127.0.0.1` to keep the server off the network entirely.
    pub fn try_with_bind(
        host: &str,
        pub_port: u16,
        pull_port: u16,
        rep_port: u16,
        telemetry_port: u16,
    ) -> Result<Self, BindError> {
        // The PUB/PULL ports are inert: value publish and the control plane ride
        // the WebSocket port (rep_port). They stay in the signature so existing call
        // sites compile unchanged.
        let _ = (pub_port, pull_port);

        let cached_messages = Arc::new(Mutex::new(HashMap::new()));
        let telemetry_subscribers = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let telemetry_registry = Arc::new(Mutex::new(HashMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));
        let started = Instant::now();

        // The control plane (get/delete/tables/ping/stats/json/CAS/logs) rides
        // the WebSocket connection as binary protobuf Request/Reply frames. The
        // websocket::Server is created after this closure, so CAS fan-out reaches it
        // through the slot. The slot holds a Weak reference: the closure lives
        // inside the websocket::Server, so a strong reference would keep the server
        // alive forever (a cycle that leaks the bound port).
        let websocket_slot = Arc::new(Mutex::new(None::<Weak<websocket::Server>>));

        let control_handler: ControlHandler = {
            let cached_messages = cached_messages.clone();
            let telemetry_subscribers = telemetry_subscribers.clone();
            let websocket_slot = websocket_slot.clone();
            Arc::new(move |payload: &[u8]| -> Option<Vec<u8>> {
                let request_payload = Request::decode(payload)
                    .ok()
                    .and_then(|request| request.payload)?;
                let reply = match request_payload {
                    request::Payload::Data(command) => {
                        let data = match cached_messages.lock() {
                            Ok(cached) => Server::read(&cached, &command.channel),
                            Err(_) => None,
                        };
                        Server::data_reply(data)
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
                            payload: Some(reply::Payload::Tables(ReplyTablesCommand { channels })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::Ping(command) => Reply {
                        payload: Some(reply::Payload::Ping(ReplyPingCommand {
                            sent_nanos: command.sent_nanos,
                            server_nanos: Server::now_nanos(),
                        })),
                    }
                    .encode_to_vec(),
                    request::Payload::Statistics(_) => {
                        let (channels, values) = match cached_messages.lock() {
                            Ok(cached) => (
                                cached.len() as u64,
                                cached.values().map(|ring| ring.items.len() as u64).sum(),
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
                            payload: Some(reply::Payload::Json(ReplyJsonCommand { json })),
                        }
                        .encode_to_vec()
                    }
                    request::Payload::CompareAndSet(command) => {
                        let channel = command.channel.clone();
                        let (swapped, current) = match cached_messages.lock() {
                            Ok(mut cached) => Server::compare_and_set(&mut cached, command),
                            Err(_) => (false, None),
                        };
                        // A successful swap is a server-assigned value: it must
                        // reach NT4 subscribers too, so fan it out (creating the
                        // topic if the channel was never published).
                        if swapped
                            && let Some(kind) = current.clone()
                            && let Some(websocket) = websocket_slot
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .as_ref()
                                .and_then(|websocket| websocket.upgrade())
                        {
                            websocket.fan_out_upsert(
                                &channel,
                                &Value::from(kind),
                                Server::now_micros(),
                            );
                        }
                        Reply {
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
                let mut ring = RingBuffer::new(CHANNEL_HISTORY);
                ring.push(kind);
                cached.insert(name.to_string(), ring);
            })
        };

        let websocket = Arc::new(
            websocket::Server::bind_with_handler(host, rep_port, control_handler, value_sink)
                .map_err(|source| BindError::WebsocketBind {
                    port: rep_port,
                    source,
                })?,
        );
        *websocket_slot.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Arc::downgrade(&websocket));

        let telemetry_socket = UdpSocket::bind(("0.0.0.0", telemetry_port)).map_err(|source| {
            BindError::Telemetry {
                port: telemetry_port,
                source,
            }
        })?;
        telemetry::tune(&telemetry_socket);
        let _ = telemetry_socket.set_read_timeout(Some(POLL_INTERVAL));

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

    /// How many fan-out frames the WebSocket server dropped because a subscriber's
    /// queue was full. Zero unless a subscriber cannot keep up.
    ///
    /// Publishes are still stored before they are fanned out, so a value counted
    /// here is readable through a control-plane read - it was only missed by the
    /// live subscription.
    pub fn dropped_publishes(&self) -> u64 {
        self.websocket.dropped_publishes()
    }

    fn track(&self, handle: std::thread::JoinHandle<()>) {
        if let Ok(mut threads) = self.threads.lock() {
            threads.push(handle);
        }
    }

    fn now_nanos() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos() as u64)
            .unwrap_or(0)
    }

    fn now_micros() -> u64 {
        Self::now_nanos() / 1000
    }

    fn read(
        cached: &HashMap<String, RingBuffer<supported_values::Kind>>,
        channel: &str,
    ) -> Option<supported_values::Kind> {
        cached.get(channel)?.peek().cloned()
    }

    fn compare_and_set(
        cached: &mut HashMap<String, RingBuffer<supported_values::Kind>>,
        command: CompareAndSetCommand,
    ) -> (bool, Option<supported_values::Kind>) {
        let current = Self::read(cached, &command.channel);

        let matches = if command.expect_absent {
            current.is_none()
        } else {
            match (&current, command.expected.and_then(|value| value.kind)) {
                (Some(current), Some(expected)) => *current == expected,
                _ => false,
            }
        };

        if !matches {
            return (false, current);
        }

        let Some(kind) = command.value.and_then(|value| value.kind) else {
            return (false, current);
        };
        cached
            .entry(command.channel)
            .or_insert_with(|| RingBuffer::new(CHANNEL_HISTORY))
            .push(kind.clone());
        (true, Some(kind))
    }

    fn write_json_value(out: &mut String, kind: &supported_values::Kind) {
        use supported_values::Kind;
        match kind {
            Kind::String(value) => Self::write_json_string(out, value),
            Kind::Int32(value) => out.push_str(&value.to_string()),
            Kind::Int64(value) => out.push_str(&value.to_string()),
            Kind::Uint32(value) => out.push_str(&value.to_string()),
            Kind::Uint64(value) => out.push_str(&value.to_string()),
            Kind::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Kind::Double(value) => Self::write_json_number(out, *value),
            Kind::Float(value) => Self::write_json_number(out, f64::from(*value)),
            Kind::Bytes(value) => {
                out.push('"');
                for byte in value {
                    out.push_str(&format!("{byte:02x}"));
                }
                out.push('"');
            }
            Kind::StringList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_string(out, value)
                });
            }
            Kind::BytesList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push('"');
                    for byte in value {
                        out.push_str(&format!("{byte:02x}"));
                    }
                    out.push('"');
                });
            }
            Kind::BoolList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(if *value { "true" } else { "false" })
                });
            }
            Kind::FloatList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_number(out, f64::from(*value))
                });
            }
            Kind::DoubleList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_number(out, *value)
                });
            }
            Kind::IntegerList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(&value.to_string())
                });
            }
            Kind::LongList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(&value.to_string())
                });
            }
            Kind::CoordinateList(list) => {
                Self::write_json_array(out, &list.coordinates, |out, coordinate| {
                    out.push_str("{\"x\":");
                    Self::write_json_number(out, coordinate.x);
                    out.push_str(",\"y\":");
                    Self::write_json_number(out, coordinate.y);
                    out.push('}');
                });
            }
            Kind::BezierCurve(curve) => Self::write_json_curve(out, curve),
            Kind::BezierCurves(curves) => {
                Self::write_json_array(out, &curves.curves, Self::write_json_curve)
            }
            Kind::BezierCurvesList(list) => {
                Self::write_json_array(out, &list.values, |out, curves| {
                    Self::write_json_array(out, &curves.curves, Self::write_json_curve)
                })
            }
        }
    }

    fn write_json_curve(out: &mut String, curve: &BezierCurve) {
        Self::write_json_array(out, &curve.control_points, |out, point| {
            out.push_str("{\"x\":");
            Self::write_json_number(out, point.x);
            out.push_str(",\"y\":");
            Self::write_json_number(out, point.y);
            if let Some(degrees) = point.rotation_degrees {
                out.push_str(",\"rotationDegrees\":");
                Self::write_json_number(out, degrees);
            }
            out.push('}');
        });
    }

    fn write_json_array<T>(out: &mut String, values: &[T], mut write: impl FnMut(&mut String, &T)) {
        out.push('[');
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write(out, value);
        }
        out.push(']');
    }

    fn write_json_number(out: &mut String, value: f64) {
        if value.is_finite() {
            out.push_str(&value.to_string());
        } else {
            out.push_str("null");
        }
    }

    fn write_json_string(out: &mut String, value: &str) {
        out.push('"');
        for character in value.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                control if (control as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", control as u32))
                }
                other => out.push(other),
            }
        }
        out.push('"');
    }

    fn to_json(
        cached: &HashMap<String, RingBuffer<supported_values::Kind>>,
        prefix: &str,
    ) -> String {
        let mut names: Vec<&String> = cached
            .keys()
            .filter(|name| name.starts_with(prefix))
            .collect();
        names.sort();

        let mut out = String::from("{");
        let mut first = true;
        for name in names {
            let Some(kind) = cached.get(name).and_then(|ring| ring.peek()) else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            Self::write_json_string(&mut out, name);
            out.push(':');
            Self::write_json_value(&mut out, kind);
        }
        out.push('}');
        out
    }

    fn data_reply(data: Option<supported_values::Kind>) -> Vec<u8> {
        let kind =
            data.unwrap_or_else(|| supported_values::Kind::String(String::from(NO_DATA_SENTINEL)));

        Reply {
            payload: Some(reply::Payload::Data(ReplyDataCommand {
                value: Some(SupportedValues { kind: Some(kind) }),
            })),
        }
        .encode_to_vec()
    }

    /// Bind the sockets and start the receive loops.
    ///
    /// Calling it again after [`stop`](Self::stop) resumes; calling it on a running
    /// server does nothing. A malformed message is logged and dropped rather than
    /// taking a loop down, and a malformed request is still answered, since the
    /// control plane is lock-step and a silent request would wedge the client.
    pub fn start(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            info!("Initializing tarwyn server...");
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            info!("Starting tarwyn server...");
            self.stop.store(false, Ordering::SeqCst);
        } else {
            info!("tarwyn server is already running.");
            return;
        }

        // The WebSocket accept loop serves both the value plane and the control plane.
        self.websocket.stop_flag().store(false, Ordering::SeqCst);
        self.track(self.websocket.start());

        self.start_telemetry_relay();
        self.start_log_relay();
    }

    /// Route `channel_hash` to `address`, and sweep every lease that has expired.
    ///
    /// `address` is the source of the registration datagram, never a name the
    /// caller chose. A caller that could name its own destination could name
    /// somebody else's, and have the server aim a channel's whole fan-out at a
    /// machine that never asked for it. A source address is not proof either,
    /// only cheaper to abuse, which is what [`MAX_TELEMETRY_SUBSCRIBERS`] and
    /// [`MAX_TELEMETRY_CHANNELS`] bound.
    ///
    /// Returns whether the address is registered afterwards.
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
        // Sweep first: an expired lease must not hold a slot against a live
        // subscriber, or one burst of registrations locks a channel for a TTL.
        for addresses in registry.values_mut() {
            addresses.retain(|_, seen| now.duration_since(*seen) < TELEMETRY_TTL);
        }
        registry.retain(|_, addresses| !addresses.is_empty());

        let known = registry.contains_key(&channel_hash);
        if !known && registry.len() >= MAX_TELEMETRY_CHANNELS {
            return false;
        }
        let addresses = registry.entry(channel_hash).or_default();
        // Refreshing an existing lease is always allowed; only a new address
        // can be turned away, so a full channel cannot evict its subscribers.
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
    /// The port carries both halves of the plane: a registration, which routes a
    /// channel to the sender's own address, and a data datagram, which is copied
    /// to everyone registered for its channel.
    fn start_telemetry_relay(&self) {
        let subscribers = self.telemetry_subscribers.clone();
        let registry = self.telemetry_registry.clone();
        let stop = self.stop.clone();
        let socket = self.telemetry_socket.clone();

        let handle = std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            // A registration whose source is the relay's own address would make
            // every datagram on that channel arrive back here and be relayed
            // again: one packet, then a loop that only the lease expiry ends.
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
                    // Any target on the relay's own port is another relay (or
                    // this one), and relaying to it loops the datagram back.
                    // Subscribers register from an ephemeral port, never this one.
                    if own.is_some_and(|own| own.port() == target.port()) {
                        continue;
                    }
                    let _ = socket.send_to(&buf[..len], target);
                }
            }
        });
        self.track(handle);
    }

    /// Relays retained log lines onto the WebSocket topic.
    ///
    /// `subscribe_to_logs` has always subscribed to this topic, and until now
    /// nothing published to it: the server only answered a control-plane
    /// request, so a subscriber received one batch and then silence.
    fn start_log_relay(&self) {
        let websocket = self.websocket.clone();
        let stop = self.stop.clone();

        let handle = std::thread::spawn(move || {
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                match crate::utils::log::LOGGER.read_unread_logs() {
                    Some(logs) => {
                        websocket.fan_out_upsert(
                            LOG_TOPIC,
                            &Value::StringArray(logs),
                            Server::now_micros(),
                        );
                    }
                    None => std::thread::sleep(POLL_INTERVAL),
                }
            }
        });
        self.track(handle);
    }

    /// Stop the receive loops. Cached values survive and are served again on the
    /// next [`start`](Self::start).
    ///
    /// Blocks until every loop has exited, which takes up to 100 ms.
    /// Joining rather than abandoning them is what lets the sockets be picked up
    /// again by the next [`start`](Self::start).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.websocket.stop();
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
