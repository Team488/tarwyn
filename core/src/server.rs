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
use crate::value::XtValue;
use crate::websocket::server::{ControlHandler, DEFAULT_BIND_HOST, ValueSink, WebsocketServer};
use tarwyn_protobuf::telemetry;

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BoolList, BytesList, CompareAndSetCommand, CoordinateList,
    DoubleList, FloatList, IntegerList, LongList, Reply, ReplyCompareAndSetCommand,
    ReplyDataCommand, ReplyDeleteCommand, ReplyJsonCommand, ReplyLogsCommand, ReplyPingCommand,
    ReplyStatisticsCommand, ReplyTablesCommand, Request, StringList, SupportedValues, reply,
    request, supported_values,
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

/// The TARWYN server: the value map, and the sockets that serve it.
///
/// One NT4 server carries the reliable traffic, value publishes and the
/// control plane, alongside a UDP socket for the telemetry plane. Nothing
/// is bound until [`start`](Self::start).
///
/// The server answers reads rather than forwarding them: it owns the table, so a
/// read is one round trip, not two.
pub struct TarwynServer {
    websocket: Arc<WebsocketServer>,
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

impl TarwynServer {
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
            .expect("could not bind the Tarwyn server")
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
        // WebsocketServer is created after this closure, so CAS fan-out reaches it
        // through the slot. The slot holds a Weak reference: the closure lives
        // inside the WebsocketServer, so a strong reference would keep the server
        // alive forever (a cycle that leaks the bound port).
        let websocket_slot = Arc::new(Mutex::new(None::<Weak<WebsocketServer>>));

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
                            Ok(cached) => TarwynServer::read(&cached, &command.channel),
                            Err(_) => None,
                        };
                        TarwynServer::data_reply(data)
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
                            server_nanos: TarwynServer::now_nanos(),
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
                            Ok(cached) => TarwynServer::to_json(&cached, &command.prefix),
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
                            Ok(mut cached) => TarwynServer::compare_and_set(&mut cached, command),
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
                                &XtValue::from(kind),
                                TarwynServer::now_micros(),
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
            Arc::new(move |name: &str, value: &XtValue| {
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
            WebsocketServer::bind_with_handler(host, rep_port, control_handler, value_sink)
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

        Ok(TarwynServer {
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
            info!("Initializing Tarwyn server...");
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            info!("Starting Tarwyn server...");
            self.stop.store(false, Ordering::SeqCst);
        } else {
            info!("Tarwyn server is already running.");
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
                            &XtValue::StringArray(logs),
                            TarwynServer::now_micros(),
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
        info!("Tarwyn server has been stopped.");
    }
}

impl From<supported_values::Kind> for XtValue {
    fn from(kind: supported_values::Kind) -> Self {
        use supported_values::Kind;
        match kind {
            Kind::String(v) => XtValue::String(v),
            Kind::Int32(v) => XtValue::Int32(v),
            Kind::Int64(v) => XtValue::Int64(v),
            Kind::Uint32(v) => XtValue::Uint32(v),
            Kind::Uint64(v) => XtValue::Uint64(v),
            Kind::Bool(v) => XtValue::Bool(v),
            Kind::Double(v) => XtValue::Double(v),
            Kind::Float(v) => XtValue::Float(v),
            Kind::Bytes(v) => XtValue::Bytes(v),
            Kind::StringList(list) => XtValue::StringArray(list.values),
            Kind::FloatList(list) => XtValue::FloatArray(list.values),
            Kind::BytesList(list) => XtValue::BytesList(list.encode_to_vec()),
            Kind::BoolList(list) => XtValue::BoolArray(list.values),
            Kind::DoubleList(list) => XtValue::DoubleArray(list.values),
            Kind::IntegerList(list) => XtValue::Int32Array(list.values),
            Kind::LongList(list) => XtValue::Int64Array(list.values),
            Kind::CoordinateList(list) => XtValue::Coordinate(list.encode_to_vec()),
            Kind::BezierCurve(curve) => XtValue::Bezier(curve.encode_to_vec()),
            Kind::BezierCurves(curves) => XtValue::Bezier(curves.encode_to_vec()),
            Kind::BezierCurvesList(list) => XtValue::Bezier(list.encode_to_vec()),
        }
    }
}

impl From<XtValue> for supported_values::Kind {
    fn from(value: XtValue) -> Self {
        use supported_values::Kind;
        match value {
            XtValue::Int8(v) => Kind::Int32(v as i32),
            XtValue::Int16(v) => Kind::Int32(v as i32),
            XtValue::Int32(v) => Kind::Int32(v),
            XtValue::Int64(v) => Kind::Int64(v),
            XtValue::Uint8(v) => Kind::Uint32(v as u32),
            XtValue::Uint16(v) => Kind::Uint32(v as u32),
            XtValue::Uint32(v) => Kind::Uint32(v),
            XtValue::Uint64(v) => Kind::Uint64(v),
            XtValue::Float(v) => Kind::Float(v),
            XtValue::Double(v) => Kind::Double(v),
            XtValue::String(v) => Kind::String(v),
            XtValue::Bool(v) => Kind::Bool(v),
            XtValue::Bytes(v) => Kind::Bytes(v),
            XtValue::Int8Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            XtValue::Int16Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            XtValue::Int32Array(v) => Kind::IntegerList(IntegerList { values: v }),
            XtValue::Int64Array(v) => Kind::LongList(LongList { values: v }),
            XtValue::Uint8Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            XtValue::Uint16Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            XtValue::Uint32Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            XtValue::Uint64Array(v) => Kind::LongList(LongList {
                values: v.into_iter().map(|x| x as i64).collect(),
            }),
            XtValue::FloatArray(v) => Kind::FloatList(FloatList { values: v }),
            XtValue::DoubleArray(v) => Kind::DoubleList(DoubleList { values: v }),
            XtValue::StringArray(v) => Kind::StringList(StringList { values: v }),
            XtValue::BoolArray(v) => Kind::BoolList(BoolList { values: v }),
            XtValue::BytesList(v) => {
                Kind::BytesList(BytesList::decode(v.as_slice()).unwrap_or_default())
            }
            XtValue::Coordinate(v) => {
                Kind::CoordinateList(CoordinateList::decode(v.as_slice()).unwrap_or_default())
            }
            XtValue::Bezier(v) => {
                Kind::BezierCurves(BezierCurves::decode(v.as_slice()).unwrap_or_default())
            }
        }
    }
}

impl std::fmt::Debug for TarwynServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TarwynServer")
            .field("telemetry_port", &self.telemetry_port)
            .field("running", &!self.stop.load(Ordering::SeqCst))
            .field("uptime", &self.started.elapsed())
            .finish_non_exhaustive()
    }
}

impl Default for TarwynServer {
    fn default() -> Self {
        TarwynServer::new()
    }
}

/// Stops the loops, so a server that goes out of scope does not leave its
/// threads holding the ports.
impl Drop for TarwynServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use tarwyn_protobuf::protobuf::GetDataCommand;

    /// The RFC 6455 example key.
    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";

    fn get_request(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Data(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    fn string(value: &str) -> supported_values::Kind {
        supported_values::Kind::String(value.to_string())
    }

    fn wrap(kind: supported_values::Kind) -> Option<Box<SupportedValues>> {
        Some(Box::new(SupportedValues { kind: Some(kind) }))
    }

    /// Connects a WebSocket client to the server and completes the handshake.
    fn connect(server: &TarwynServer) -> TcpStream {
        let addr = server.websocket.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let req = format!(
            "GET /nt/test HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Protocol: {NT4_SUBPROTOCOL}\r\n\r\n"
        );
        client.write_all(req.as_bytes()).unwrap();
        let mut resp = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n = client.read(&mut buf).unwrap();
            assert!(n > 0, "server closed during handshake");
            resp.extend_from_slice(&buf[..n]);
            if resp.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        assert!(
            String::from_utf8(resp).unwrap().starts_with("HTTP/1.1 101"),
            "handshake failed"
        );
        client
    }

    /// Writes a masked binary frame.
    fn write_masked_binary(stream: &mut TcpStream, payload: &[u8]) {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut header = vec![0x80 | 0x2];
        let len = payload.len();
        if len < 126 {
            header.push(0x80 | len as u8);
        } else if len <= u16::MAX as usize {
            header.push(0x80 | 126);
            header.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            header.push(0x80 | 127);
            header.extend_from_slice(&(len as u64).to_be_bytes());
        }
        header.extend_from_slice(&mask);
        let masked: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % 4])
            .collect();
        stream.write_all(&header).unwrap();
        stream.write_all(&masked).unwrap();
    }

    /// Reads one unmasked server frame, returning `(opcode, payload)`.
    fn read_server_frame(stream: &mut TcpStream) -> (u8, Vec<u8>) {
        let mut hdr = [0u8; 2];
        stream.read_exact(&mut hdr).unwrap();
        let opcode = hdr[0] & 0x0f;
        let len = match hdr[1] & 0x7f {
            126 => {
                let mut b = [0u8; 2];
                stream.read_exact(&mut b).unwrap();
                u16::from_be_bytes(b) as usize
            }
            127 => {
                let mut b = [0u8; 8];
                stream.read_exact(&mut b).unwrap();
                u64::from_be_bytes(b) as usize
            }
            n => n as usize,
        };
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        (opcode, payload)
    }

    /// Reads one server frame, returning `None` on a clean close or timeout.
    fn try_read_server_frame(stream: &mut TcpStream) -> std::io::Result<Option<(u8, Vec<u8>)>> {
        let mut hdr = [0u8; 2];
        match stream.read_exact(&mut hdr) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let opcode = hdr[0] & 0x0f;
        let len = match hdr[1] & 0x7f {
            126 => {
                let mut b = [0u8; 2];
                stream.read_exact(&mut b)?;
                u16::from_be_bytes(b) as usize
            }
            127 => {
                let mut b = [0u8; 8];
                stream.read_exact(&mut b)?;
                u64::from_be_bytes(b) as usize
            }
            n => n as usize,
        };
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload)?;
        Ok(Some((opcode, payload)))
    }

    /// Sends a control request and returns the decoded reply payload.
    fn control_round_trip(server: &TarwynServer, request: &[u8]) -> reply::Payload {
        let mut client = connect(server);
        write_masked_binary(&mut client, request);
        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "control reply must be a binary frame");
        let reply = Reply::decode(payload.as_slice()).expect("not a Reply");
        reply.payload.expect("reply carried no payload")
    }

    #[test]
    fn reading_an_absent_channel_does_not_invent_it() {
        let cached: HashMap<String, RingBuffer<supported_values::Kind>> = HashMap::new();

        let value = TarwynServer::read(&cached, "never-published");

        assert!(value.is_none());
        assert!(
            cached.is_empty(),
            "a read created the channel, so getTables reports one that was never published \
             and the map grows for every name anyone asks about"
        );
    }

    #[test]
    fn a_refused_compare_and_set_does_not_invent_the_channel() {
        let mut cached = HashMap::new();

        let (swapped, current) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "never-published".into(),
                expected: wrap(string("something")),
                value: wrap(string("agent-a")),
                expect_absent: false,
            },
        );

        assert!(!swapped);
        assert_eq!(current, None);
        assert!(cached.is_empty(), "a refused swap created the channel");
    }

    #[test]
    fn compare_and_set_claims_an_empty_channel_once() {
        let mut cached = HashMap::new();

        let (claimed, _) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "lock".into(),
                expected: None,
                value: wrap(string("agent-a")),
                expect_absent: true,
            },
        );
        assert!(claimed);

        let (stolen, current) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "lock".into(),
                expected: None,
                value: wrap(string("agent-b")),
                expect_absent: true,
            },
        );
        assert!(
            !stolen,
            "a second claimant took a lock that was already held"
        );
        assert_eq!(current, Some(string("agent-a")));
    }

    #[test]
    fn compare_and_set_refuses_a_stale_expectation() {
        let mut cached = HashMap::new();
        cached
            .entry(String::from("counter"))
            .or_insert_with(|| RingBuffer::new(100))
            .push(supported_values::Kind::Double(1.0));

        let (moved, _) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "counter".into(),
                expected: wrap(supported_values::Kind::Double(1.0)),
                value: wrap(supported_values::Kind::Double(2.0)),
                expect_absent: false,
            },
        );
        assert!(moved);

        let (again, current) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "counter".into(),
                expected: wrap(supported_values::Kind::Double(1.0)),
                value: wrap(supported_values::Kind::Double(3.0)),
                expect_absent: false,
            },
        );
        assert!(!again, "a read-modify-write raced and both writers won");
        assert_eq!(current, Some(supported_values::Kind::Double(2.0)));
    }

    #[test]
    fn json_escapes_what_would_otherwise_break_the_document() {
        let mut cached = HashMap::new();
        cached
            .entry(String::from("quote\"and\\slash"))
            .or_insert_with(|| RingBuffer::new(100))
            .push(string("line\nbreak\ttab"));

        let json = TarwynServer::to_json(&cached, "");
        assert_eq!(
            json, r#"{"quote\"and\\slash":"line\nbreak\ttab"}"#,
            "the document would not parse"
        );
    }

    #[test]
    fn json_leaves_out_channels_outside_the_prefix() {
        let mut cached = HashMap::new();
        for name in ["robot/a", "robot/b", "vision/c"] {
            cached
                .entry(String::from(name))
                .or_insert_with(|| RingBuffer::new(100))
                .push(supported_values::Kind::Bool(true));
        }

        assert_eq!(
            TarwynServer::to_json(&cached, "robot/"),
            r#"{"robot/a":true,"robot/b":true}"#
        );
    }

    #[test]
    fn kind_xtvalue_round_trips_scalars_and_lists() {
        use supported_values::Kind;
        let cases: Vec<Kind> = vec![
            Kind::String("hi".into()),
            Kind::Int32(-5),
            Kind::Int64(-9_000_000_000),
            Kind::Uint32(7),
            Kind::Uint64(9_000_000_000),
            Kind::Bool(true),
            Kind::Double(1.5),
            Kind::Float(2.5),
            Kind::Bytes(vec![1, 2, 3]),
            Kind::StringList(StringList {
                values: vec!["a".into(), "b".into()],
            }),
            Kind::FloatList(FloatList {
                values: vec![1.0, 2.0],
            }),
            Kind::BoolList(BoolList {
                values: vec![true, false],
            }),
            Kind::DoubleList(DoubleList {
                values: vec![1.5, 2.5],
            }),
            Kind::IntegerList(IntegerList {
                values: vec![1, 2, 3],
            }),
            Kind::LongList(LongList {
                values: vec![1, 2, 3],
            }),
        ];
        for kind in cases {
            let value = XtValue::from(kind.clone());
            let back = supported_values::Kind::from(value);
            assert_eq!(back, kind, "round trip changed the value");
        }
    }

    #[test]
    fn control_plane_get_returns_no_data_for_absent_channel() {
        let server = TarwynServer::with_ports_and_telemetry(21841, 21842, 21843, 21844);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        match control_round_trip(&server, &get_request("absent")) {
            reply::Payload::Data(cmd) => {
                let value = cmd.value.and_then(|v| v.kind);
                assert_eq!(
                    value,
                    Some(supported_values::Kind::String(NO_DATA_SENTINEL.to_string()))
                );
            }
            other => panic!("expected data reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_cas_then_get() {
        let server = TarwynServer::with_ports_and_telemetry(21851, 21852, 21853, 21854);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let cas_request = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: "lock".into(),
                expected: None,
                value: wrap(string("agent-a")),
                expect_absent: true,
            })),
        }
        .encode_to_vec();
        match control_round_trip(&server, &cas_request) {
            reply::Payload::CompareAndSet(cmd) => {
                assert!(cmd.swapped, "CAS should claim an empty channel");
            }
            other => panic!("expected CAS reply, got {other:?}"),
        }

        match control_round_trip(&server, &get_request("lock")) {
            reply::Payload::Data(cmd) => {
                assert_eq!(cmd.value.and_then(|v| v.kind), Some(string("agent-a")));
            }
            other => panic!("expected data reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_tables_lists_channels() {
        let server = TarwynServer::with_ports_and_telemetry(21861, 21862, 21863, 21864);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let cas_a = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: "robot/a".into(),
                expected: None,
                value: wrap(string("va")),
                expect_absent: true,
            })),
        }
        .encode_to_vec();
        let _ = control_round_trip(&server, &cas_a);

        let cas_b = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: "robot/b".into(),
                expected: None,
                value: wrap(string("vb")),
                expect_absent: true,
            })),
        }
        .encode_to_vec();
        let _ = control_round_trip(&server, &cas_b);

        let tables_request = Request {
            payload: Some(request::Payload::Tables(
                tarwyn_protobuf::protobuf::ListTablesCommand {
                    prefix: "robot/".into(),
                },
            )),
        }
        .encode_to_vec();
        match control_round_trip(&server, &tables_request) {
            reply::Payload::Tables(cmd) => {
                assert_eq!(cmd.channels, vec!["robot/a", "robot/b"]);
            }
            other => panic!("expected tables reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_ping_returns_server_nanos() {
        let server = TarwynServer::with_ports_and_telemetry(21871, 21872, 21873, 21874);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let ping_request = Request {
            payload: Some(request::Payload::Ping(
                tarwyn_protobuf::protobuf::PingCommand { sent_nanos: 42 },
            )),
        }
        .encode_to_vec();
        match control_round_trip(&server, &ping_request) {
            reply::Payload::Ping(cmd) => {
                assert_eq!(cmd.sent_nanos, 42);
                assert!(cmd.server_nanos > 0);
            }
            other => panic!("expected ping reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_statistics() {
        let server = TarwynServer::with_ports_and_telemetry(21881, 21882, 21883, 21884);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let stats_request = Request {
            payload: Some(request::Payload::Statistics(
                tarwyn_protobuf::protobuf::StatisticsCommand {},
            )),
        }
        .encode_to_vec();
        match control_round_trip(&server, &stats_request) {
            reply::Payload::Statistics(cmd) => {
                assert_eq!(cmd.version, env!("CARGO_PKG_VERSION"));
            }
            other => panic!("expected statistics reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_json() {
        let server = TarwynServer::with_ports_and_telemetry(21891, 21892, 21893, 21894);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let cas_request = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: "test".into(),
                expected: None,
                value: wrap(string("hello")),
                expect_absent: true,
            })),
        }
        .encode_to_vec();
        let _ = control_round_trip(&server, &cas_request);

        let json_request = Request {
            payload: Some(request::Payload::Json(
                tarwyn_protobuf::protobuf::JsonCommand {
                    prefix: "test".into(),
                },
            )),
        }
        .encode_to_vec();
        match control_round_trip(&server, &json_request) {
            reply::Payload::Json(cmd) => {
                assert!(cmd.json.contains("hello"));
            }
            other => panic!("expected json reply, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn control_plane_delete() {
        let server = TarwynServer::with_ports_and_telemetry(21901, 21902, 21903, 21904);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let cas_request = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: "del".into(),
                expected: None,
                value: wrap(string("val")),
                expect_absent: true,
            })),
        }
        .encode_to_vec();
        let _ = control_round_trip(&server, &cas_request);

        let delete_request = Request {
            payload: Some(request::Payload::Delete(
                tarwyn_protobuf::protobuf::DeleteCommand {
                    channel: "del".into(),
                },
            )),
        }
        .encode_to_vec();
        match control_round_trip(&server, &delete_request) {
            reply::Payload::Delete(cmd) => {
                assert_eq!(cmd.deleted, 1);
            }
            other => panic!("expected delete reply, got {other:?}"),
        }

        match control_round_trip(&server, &get_request("del")) {
            reply::Payload::Data(cmd) => {
                let value = cmd.value.and_then(|v| v.kind);
                assert_eq!(value, Some(string(NO_DATA_SENTINEL)));
            }
            other => panic!("expected data reply after delete, got {other:?}"),
        }
        server.stop();
    }

    #[test]
    fn stop_joins_its_loops_so_the_sockets_can_be_picked_up_again() {
        let server = TarwynServer::with_ports_and_telemetry(21911, 21912, 21913, 21914);
        server.start();
        std::thread::sleep(Duration::from_millis(200));
        server.stop();

        assert!(
            server.threads.lock().unwrap().is_empty(),
            "stop() left thread handles behind"
        );
    }

    #[test]
    fn malformed_ws_payload_closes_connection() {
        let server = TarwynServer::with_ports_and_telemetry(21921, 21922, 21923, 21924);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let mut client = connect(&server);
        write_masked_binary(&mut client, b"\xff\xfe\xfd\xfc");
        let _ = client.set_read_timeout(Some(Duration::from_secs(2)));

        let closed = match try_read_server_frame(&mut client) {
            Ok(Some((opcode, _))) => opcode == 0x8,
            Ok(None) => true,
            Err(_) => false,
        };
        assert!(
            closed,
            "server must close the connection on malformed input"
        );

        let mut client2 = connect(&server);
        let ping_request = Request {
            payload: Some(request::Payload::Ping(
                tarwyn_protobuf::protobuf::PingCommand { sent_nanos: 1 },
            )),
        }
        .encode_to_vec();
        write_masked_binary(&mut client2, &ping_request);
        let (opcode, _) = read_server_frame(&mut client2);
        assert_eq!(
            opcode, 0x2,
            "server must still accept requests after a malformed one"
        );

        server.stop();
    }

    #[test]
    fn dropping_a_server_releases_its_ws_port() {
        let port;
        {
            let server = TarwynServer::with_ports_and_telemetry(21931, 21932, 21933, 21934);
            server.start();
            port = server.websocket.local_addr().unwrap().port();
            std::thread::sleep(Duration::from_millis(200));
        }

        assert!(
            std::net::TcpListener::bind((DEFAULT_BIND_HOST, port)).is_ok(),
            "WebSocket port {port} was still bound after the server was dropped"
        );
        assert!(
            std::net::UdpSocket::bind(("127.0.0.1", 21934)).is_ok(),
            "the telemetry port was still bound after the server was dropped"
        );
    }

    #[test]
    fn a_port_that_stays_taken_is_reported_rather_than_panicking() {
        let squatter = std::net::TcpListener::bind((DEFAULT_BIND_HOST, 22023))
            .expect("the squatter has to hold the address the server will ask for");

        let error = TarwynServer::try_with_ports_and_telemetry(22021, 22022, 22023, 22024)
            .expect_err(
                "the WebSocket port was already bound on the address the server binds, \
                 so this cannot succeed",
            );

        assert!(
            matches!(error, BindError::WebsocketBind { port: 22023, .. }),
            "the error has to name the port, got {error:?}"
        );

        drop(squatter);
    }

    /// The relay routes a channel to the address its registration arrived from.
    ///
    /// A subscriber cannot learn its own address - its socket is bound to
    /// `0.0.0.0` - so any address it could name would be a guess, and the guess
    /// that was made was the server's own. That reached a subscriber only on
    /// loopback, where the guess happens to be right, which is where every test
    /// ran. Registering by datagram takes the address out of the caller's hands:
    /// the server reads it off the packet.
    #[test]
    fn a_subscriber_is_routed_to_wherever_its_registration_came_from() {
        let server = TarwynServer::with_ports_and_telemetry(22041, 22042, 22043, 22044);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let relay: SocketAddr = ([127, 0, 0, 1], 22044).into();
        let subscriber = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        subscriber
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();

        let mut buf = [0u8; telemetry::MAX_DATAGRAM];
        let len = telemetry::encode_registration(&mut buf, telemetry::topic_hash("routed"));
        subscriber.send_to(&buf[..len], relay).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let publisher = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let len = telemetry::encode(
            &mut buf,
            telemetry::topic_hash("routed"),
            telemetry::now_micros(),
            b"payload",
        );
        publisher.send_to(&buf[..len], relay).unwrap();

        let mut received = [0u8; telemetry::MAX_DATAGRAM];
        let (len, _) = subscriber
            .recv_from(&mut received)
            .expect("the relay never reached the address the registration came from");
        assert_eq!(
            telemetry::decode(&received[..len]).map(|(_, _, payload)| payload),
            Some(&b"payload"[..])
        );
    }

    /// Registration carries no address, so a caller cannot name one.
    ///
    /// It used to name one over REQ/REP, which let anyone aim a channel's whole
    /// fan-out at a machine that never asked for it - the server would send
    /// traffic on their behalf, to a target of their choosing, at a rate they did
    /// not have to generate.
    #[test]
    fn a_publisher_is_not_registered_by_publishing() {
        let server = TarwynServer::with_ports_and_telemetry(22051, 22052, 22053, 22054);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let relay: SocketAddr = ([127, 0, 0, 1], 22054).into();
        let publisher = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        publisher
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();

        let mut buf = [0u8; telemetry::MAX_DATAGRAM];
        for _ in 0..3 {
            let len = telemetry::encode(
                &mut buf,
                telemetry::topic_hash("loud"),
                telemetry::now_micros(),
                b"payload",
            );
            publisher.send_to(&buf[..len], relay).unwrap();
        }

        let mut received = [0u8; telemetry::MAX_DATAGRAM];
        assert!(
            publisher.recv_from(&mut received).is_err(),
            "publishing subscribed the publisher, so the relay echoes traffic back \
             to whoever sends it"
        );
        assert!(
            server.telemetry_registry.lock().unwrap().is_empty(),
            "a datagram that was not a registration created one"
        );
    }
}

#[cfg(test)]
mod telemetry_registration_tests {
    use super::*;

    fn register(server: &TarwynServer, socket: &std::net::UdpSocket, hash: u32, to: SocketAddr) {
        let mut buf = [0u8; telemetry::HEADER_LEN];
        let n = telemetry::encode_registration(&mut buf, hash);
        socket.send_to(&buf[..n], to).unwrap();
        let _ = server;
    }

    #[test]
    fn one_datagram_is_not_amplified_past_the_subscriber_cap() {
        let fanout = MAX_TELEMETRY_SUBSCRIBERS * 4;
        let server = TarwynServer::with_ports_and_telemetry(22201, 22202, 22203, 22204);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let relay: SocketAddr = "127.0.0.1:22204".parse().unwrap();
        let hash = telemetry::topic_hash("amplify");

        // One host, many source ports: without a cap each is its own subscriber
        // and the relay copies every datagram to all of them.
        let sockets: Vec<std::net::UdpSocket> = (0..fanout)
            .map(|_| {
                let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
                socket.set_nonblocking(true).unwrap();
                register(&server, &socket, hash, relay);
                socket
            })
            .collect();
        std::thread::sleep(Duration::from_millis(300));

        let mut datagram = vec![0u8; telemetry::HEADER_LEN + 64];
        let n = telemetry::encode(&mut datagram, hash, 0, &[7u8; 64]);
        std::net::UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .send_to(&datagram[..n], relay)
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));

        let mut copies = 0;
        let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
        for socket in &sockets {
            if socket.recv_from(&mut buf).is_ok() {
                copies += 1;
            }
        }
        server.stop();

        assert!(
            copies <= MAX_TELEMETRY_SUBSCRIBERS,
            "one datagram reached {copies} addresses, past the cap of \
             {MAX_TELEMETRY_SUBSCRIBERS}"
        );
        assert!(
            copies > 0,
            "the relay has to still deliver to real subscribers"
        );
    }

    #[test]
    fn a_subscriber_keeps_its_slot_when_the_channel_is_full() {
        let registry = Mutex::new(HashMap::new());
        let published = ArcSwap::from_pointee(HashMap::new());
        let address = |port: u16| SocketAddr::from(([127, 0, 0, 1], port));

        for port in 0..MAX_TELEMETRY_SUBSCRIBERS as u16 {
            assert!(TarwynServer::register_telemetry(
                &registry,
                &published,
                7,
                address(9000 + port)
            ));
        }
        assert!(
            !TarwynServer::register_telemetry(&registry, &published, 7, address(9999)),
            "a full channel has to turn a new address away"
        );
        assert!(
            TarwynServer::register_telemetry(&registry, &published, 7, address(9000)),
            "an address already holding a slot has to be able to renew its lease"
        );
    }
}
