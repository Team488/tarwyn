use arc_swap::ArcSwap;
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::utils::{log::LOGGER, ports, ring_buffer::RingBuffer};
use tarwyn_protobuf::telemetry;

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurve, CompareAndSetCommand, Publish, Push, Reply, ReplyCompareAndSetCommand,
    ReplyDataCommand, ReplyDeleteCommand, ReplyJsonCommand, ReplyLogsCommand, ReplyPingCommand,
    ReplyStatisticsCommand, ReplyTablesCommand, Request, SendDataCommand, SendLogsCommand,
    SupportedValues, publish, push, reply, request, supported_values,
};

use zmq::{
    Context, SNDMORE,
    SocketType::{PUB, PULL, REP},
};

const TELEMETRY_TTL: Duration = Duration::from_secs(10);
/// Outbound messages the PUB socket queues per subscriber before it refuses more.
const PUB_HIGH_WATER_MARK: i32 = 10_000;
/// How long a fan-out send waits for a full subscriber queue before giving up.
const PUB_SEND_TIMEOUT_MS: i32 = 10;
/// How many times a port is tried before the bind is reported as failed.
const BIND_ATTEMPTS: u32 = 5;
/// How long to wait between those attempts.
const BIND_RETRY: Duration = Duration::from_millis(200);
/// Values retained per channel, so a late subscriber sees recent history.
const CHANNEL_HISTORY: usize = 100;
/// How long a receive loop blocks before it looks at the stop flag again.
const POLL_INTERVAL_MS: i32 = 100;
const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The PUB topic subscribe_to_logs listens on.
const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";

const DEFAULT_REP_PORT: u16 = ports::DEFAULT_REQ_REP_PORT;
const DEFAULT_PUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PULL_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

/// The TARWYN server: the value map, and the sockets that serve it.
///
/// Three ZeroMQ sockets carry the reliable traffic — PULL for publishes, PUB for
/// subscriptions, REP for reads and the control plane — alongside a UDP socket
/// for the telemetry plane. Nothing is bound until [`start`](Self::start).
///
/// The server answers reads rather than forwarding them: it owns the table, so a
/// read is one round trip, not two.
pub struct TarwynServer {
    pub_socket: Arc<Mutex<zmq::Socket>>,
    pull_socket: Arc<Mutex<zmq::Socket>>,
    rep_socket: Arc<Mutex<zmq::Socket>>,
    cached_messages: Arc<Mutex<HashMap<String, RingBuffer<supported_values::Kind>>>>,
    telemetry_subscribers: Arc<ArcSwap<HashMap<u32, Vec<SocketAddr>>>>,
    telemetry_registry: Arc<Mutex<HashMap<u32, HashMap<SocketAddr, Instant>>>>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    started: Instant,
    telemetry_socket: Arc<UdpSocket>,
    telemetry_port: u16,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    dropped_publishes: Arc<AtomicU64>,
}

/// Why a server could not take the ports it was asked for.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// A ZeroMQ socket could not be created.
    #[error("could not create the {socket} socket")]
    Socket {
        /// Which of the three sockets failed.
        socket: &'static str,
        /// The underlying ZeroMQ error.
        source: zmq::Error,
    },
    /// A socket was created but could not be configured.
    #[error("could not configure the {socket} socket")]
    Configure {
        /// Which of the three sockets failed.
        socket: &'static str,
        /// The underlying ZeroMQ error.
        source: zmq::Error,
    },
    /// A port was still taken after every attempt.
    #[error("could not bind the {socket} socket to port {port}")]
    Bind {
        /// Which of the three sockets failed.
        socket: &'static str,
        /// The port it was asked for.
        port: u16,
        /// The underlying ZeroMQ error.
        source: zmq::Error,
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

/// Take a port, retrying briefly before giving up.
///
/// A port can be held for a moment by something that is on its way out - a
/// previous instance still closing, or an outbound connection the kernel
/// allocated. Retrying costs a second at worst and turns that into a start
/// rather than a failure. A port held by something that is staying still fails,
/// with which socket and which port in the error.
fn bind_retrying(socket: &zmq::Socket, name: &'static str, port: u16) -> Result<(), BindError> {
    let endpoint = format!("tcp://*:{port}");
    let mut remaining = BIND_ATTEMPTS;
    loop {
        match socket.bind(&endpoint) {
            Ok(()) => return Ok(()),
            Err(source) => {
                remaining -= 1;
                if remaining == 0 {
                    return Err(BindError::Bind {
                        socket: name,
                        port,
                        source,
                    });
                }
                std::thread::sleep(BIND_RETRY);
            }
        }
    }
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

/// Turn off `ZMQ_XPUB_NODROP`'s default, so a full subscriber queue reports
/// `EAGAIN` rather than discarding the message.
///
/// `zmq` 0.10 has no setter for this option, so it is set through the raw socket
/// the crate hands out for exactly this purpose. `PUB` inherits the option from
/// `XPUB`, which is why the socket type does not have to change.
///
/// # Panics
///
/// If libzmq rejects the option, which would leave the socket silently lossy.
fn deny_dropping(socket: zmq::Socket) -> zmq::Socket {
    let enabled: std::os::raw::c_int = 1;
    let raw = socket.into_raw();
    let code = unsafe {
        zmq_sys::zmq_setsockopt(
            raw,
            zmq_sys::ZMQ_XPUB_NODROP as std::os::raw::c_int,
            std::ptr::addr_of!(enabled).cast(),
            std::mem::size_of::<std::os::raw::c_int>(),
        )
    };
    let socket = unsafe { zmq::Socket::from_raw(raw) };
    assert_eq!(code, 0, "could not set ZMQ_XPUB_NODROP on the PUB socket");
    socket
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
    /// Each port is retried for about a second before it is given up on, so a
    /// port held by something on its way out does not stop the server starting.
    pub fn try_with_ports_and_telemetry(
        pub_port: u16,
        pull_port: u16,
        rep_port: u16,
        telemetry_port: u16,
    ) -> Result<Self, BindError> {
        let context = Context::new();

        let cached_messages = Arc::new(Mutex::new(HashMap::new()));
        let telemetry_subscribers = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let telemetry_registry = Arc::new(Mutex::new(HashMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let socket = |kind, name: &'static str| {
            context.socket(kind).map_err(|source| BindError::Socket {
                socket: name,
                source,
            })
        };
        let configure = |name: &'static str| {
            move |source| BindError::Configure {
                socket: name,
                source,
            }
        };

        let publisher = socket(PUB, "PUB")?;
        publisher
            .set_sndhwm(PUB_HIGH_WATER_MARK)
            .map_err(configure("PUB"))?;
        publisher
            .set_sndtimeo(PUB_SEND_TIMEOUT_MS)
            .map_err(configure("PUB"))?;
        let publisher = deny_dropping(publisher);
        bind_retrying(&publisher, "PUB", pub_port)?;

        let puller = socket(PULL, "PULL")?;
        puller
            .set_rcvtimeo(POLL_INTERVAL_MS)
            .map_err(configure("PULL"))?;
        bind_retrying(&puller, "PULL", pull_port)?;

        let replier = socket(REP, "REP")?;
        replier
            .set_rcvtimeo(POLL_INTERVAL_MS)
            .map_err(configure("REP"))?;
        bind_retrying(&replier, "REP", rep_port)?;

        let telemetry_socket = UdpSocket::bind(("0.0.0.0", telemetry_port)).map_err(|source| {
            BindError::Telemetry {
                port: telemetry_port,
                source,
            }
        })?;
        telemetry::tune(&telemetry_socket);
        let _ =
            telemetry_socket.set_read_timeout(Some(Duration::from_millis(POLL_INTERVAL_MS as u64)));

        let pub_socket = Arc::new(Mutex::new(publisher));
        let pull_socket = Arc::new(Mutex::new(puller));
        let rep_socket = Arc::new(Mutex::new(replier));

        Ok(TarwynServer {
            pub_socket,
            pull_socket,
            rep_socket,
            cached_messages,
            telemetry_subscribers,
            telemetry_registry,
            stop,
            initialized,
            started: Instant::now(),
            telemetry_socket: Arc::new(telemetry_socket),
            telemetry_port,
            threads: Mutex::new(Vec::new()),
            dropped_publishes: Arc::new(AtomicU64::new(0)),
        })
    }

    /// How many fan-out messages the PUB socket refused because a subscriber's
    /// queue was full. Zero unless a subscriber cannot keep up.
    ///
    /// Publishes are still stored before they are fanned out, so a value counted
    /// here is readable through a REQ/REP read - it was only missed by the live
    /// subscription.
    pub fn dropped_publishes(&self) -> u64 {
        self.dropped_publishes.load(Ordering::Relaxed)
    }

    /// Fan a topic and its payload out as one two-part message.
    ///
    /// With `ZMQ_XPUB_NODROP` set a refused send reports `EAGAIN` instead of
    /// discarding silently, so the message can be counted instead of vanishing.
    ///
    /// libzmq charges a whole multi-part message to the high-water mark on its
    /// last frame, so a queue with room for the topic has room for the payload
    /// too and the two frames are refused together. Counting both is what keeps
    /// the count honest if that ever stops being true.
    fn fan_out(socket: &zmq::Socket, topic: &str, message: Vec<u8>, dropped: &AtomicU64) {
        if socket.send(topic, SNDMORE).is_err() || socket.send(message, 0).is_err() {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
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

    fn publish_logs(logs: Vec<String>) -> Vec<u8> {
        Publish {
            payload: Some(publish::Payload::Logs(SendLogsCommand { logs })),
        }
        .encode_to_vec()
    }

    fn publish_data(channel: &str, data: supported_values::Kind) -> Vec<u8> {
        Publish {
            payload: Some(publish::Payload::Data(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues { kind: Some(data) }),
            })),
        }
        .encode_to_vec()
    }

    /// Bind the sockets and start the receive loops.
    ///
    /// Calling it again after [`stop`](Self::stop) resumes; calling it on a running
    /// server does nothing. A malformed message is logged and dropped rather than
    /// taking a loop down, and a malformed request is still answered, since REQ/REP
    /// is lock-step and a silent request would wedge the client's socket.
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

        {
            let cached_messages = self.cached_messages.clone();
            let pull_socket = self.pull_socket.clone();
            let pub_socket = self.pub_socket.clone();
            let dropped_publishes = self.dropped_publishes.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            let handle = std::thread::spawn(move || {
                let pull_socket = pull_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let bytes = match pull_socket.recv_bytes(0) {
                        Ok(bytes) => bytes,
                        Err(zmq::Error::EAGAIN) => continue,
                        Err(zmq::Error::ETERM) => break,
                        Err(error) => {
                            info!("dropping a push that could not be received: {error}");
                            continue;
                        }
                    };

                    let Some(payload) = Push::decode(&bytes[..]).ok().and_then(|push| push.payload)
                    else {
                        info!("dropping a malformed push of {} bytes", bytes.len());
                        continue;
                    };

                    match payload {
                        push::Payload::Send(command) => {
                            let channel = command.channel;
                            let Some(data) = command.value.and_then(|value| value.kind) else {
                                info!("dropping a push on '{channel}' that carried no value");
                                continue;
                            };

                            let Ok(mut cached) = cached_messages.lock() else {
                                continue;
                            };
                            let ring_buffer = cached
                                .entry(channel.clone())
                                .or_insert_with(|| RingBuffer::new(CHANNEL_HISTORY));

                            let message = Self::publish_data(&channel, data.clone());
                            ring_buffer.push(data);
                            drop(cached);

                            let Ok(pub_socket) = pub_socket.lock() else {
                                continue;
                            };
                            Self::fan_out(&pub_socket, &channel, message, &dropped_publishes);
                        }
                    }
                }
            });
            self.track(handle);
        }

        self.start_telemetry_relay();
        self.start_log_relay();

        {
            let cached_buffers = self.cached_messages.clone();
            let telemetry_subscribers = self.telemetry_subscribers.clone();
            let rep_socket = self.rep_socket.clone();
            let dropped_publishes = self.dropped_publishes.clone();
            let stop = self.stop.clone();
            let started = self.started;

            let handle = std::thread::spawn(move || {
                let rep_socket = rep_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }

                    let bytes = match rep_socket.recv_bytes(0) {
                        Ok(bytes) => bytes,
                        Err(zmq::Error::EAGAIN) => continue,
                        Err(zmq::Error::ETERM) => break,
                        Err(error) => {
                            info!("dropping a request that could not be received: {error}");
                            continue;
                        }
                    };

                    let Some(payload) = Request::decode(&bytes[..])
                        .ok()
                        .and_then(|request| request.payload)
                    else {
                        info!(
                            "answering a malformed request of {} bytes with no data",
                            bytes.len()
                        );
                        let _ = rep_socket.send(Self::data_reply(None), 0);
                        continue;
                    };

                    match payload {
                        request::Payload::Data(command) => {
                            let data = match cached_buffers.lock() {
                                Ok(cached) => Self::read(&cached, &command.channel),
                                Err(_) => None,
                            };

                            let _ = rep_socket.send(Self::data_reply(data), 0);
                        }
                        request::Payload::Delete(command) => {
                            let deleted = match cached_buffers.lock() {
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

                            let message = Reply {
                                payload: Some(reply::Payload::Delete(ReplyDeleteCommand {
                                    deleted: deleted as u32,
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Tables(command) => {
                            let channels = match cached_buffers.lock() {
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

                            let message = Reply {
                                payload: Some(reply::Payload::Tables(ReplyTablesCommand {
                                    channels,
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Ping(command) => {
                            let message = Reply {
                                payload: Some(reply::Payload::Ping(ReplyPingCommand {
                                    sent_nanos: command.sent_nanos,
                                    server_nanos: Self::now_nanos(),
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Statistics(_) => {
                            let (channels, values) = match cached_buffers.lock() {
                                Ok(cached) => (
                                    cached.len() as u64,
                                    cached.values().map(|ring| ring.items.len() as u64).sum(),
                                ),
                                Err(_) => (0, 0),
                            };
                            let subscribers = telemetry_subscribers
                                .load()
                                .values()
                                .map(|addresses| addresses.len() as u64)
                                .sum();

                            let message = Reply {
                                payload: Some(reply::Payload::Statistics(ReplyStatisticsCommand {
                                    channels,
                                    values,
                                    telemetry_subscribers: subscribers,
                                    uptime_seconds: started.elapsed().as_secs(),
                                    version: env!("CARGO_PKG_VERSION").to_string(),
                                    dropped_publishes: dropped_publishes.load(Ordering::Relaxed),
                                    dropped_logs: LOGGER.dropped(),
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Json(command) => {
                            let json = match cached_buffers.lock() {
                                Ok(cached) => Self::to_json(&cached, &command.prefix),
                                Err(_) => String::from("{}"),
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::Json(ReplyJsonCommand { json })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::CompareAndSet(command) => {
                            let (swapped, current) = match cached_buffers.lock() {
                                Ok(mut cached) => Self::compare_and_set(&mut cached, command),
                                Err(_) => (false, None),
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::CompareAndSet(
                                    ReplyCompareAndSetCommand {
                                        swapped,
                                        current: current.map(|kind| {
                                            Box::new(SupportedValues { kind: Some(kind) })
                                        }),
                                    },
                                )),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Logs(_) => {
                            let logs = LOGGER.get_logs();
                            if let Some(logs) = logs {
                                info!("Sending logs in response to request.");
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand { logs })),
                                }
                                .encode_to_vec();

                                let _ = rep_socket.send(message, 0);
                            } else {
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand {
                                        logs: vec![],
                                    })),
                                }
                                .encode_to_vec();

                                let _ = rep_socket.send(message, 0);
                            }
                        }
                    }
                }
            });
            self.track(handle);
        }
    }

    /// Route `channel_hash` to `address`, and sweep every lease that has expired.
    ///
    /// `address` is the source of the registration datagram, never a name the
    /// caller chose. A caller that could name its own destination could name
    /// somebody else's, and have the server aim a channel's whole fan-out at a
    /// machine that never asked for it.
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
        registry
            .entry(channel_hash)
            .or_default()
            .insert(address, now);

        for addresses in registry.values_mut() {
            addresses.retain(|_, seen| now.duration_since(*seen) < TELEMETRY_TTL);
        }
        registry.retain(|_, addresses| !addresses.is_empty());

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
                    let _ = socket.send_to(&buf[..len], target);
                }
            }
        });
        self.track(handle);
    }

    /// Relays retained log lines onto the PUB socket.
    ///
    /// `subscribe_to_logs` has always subscribed to this topic, and until now
    /// nothing published to it: the server only answered a REQ/REP request, so a
    /// subscriber received one batch and then silence.
    fn start_log_relay(&self) {
        let pub_socket = self.pub_socket.clone();
        let dropped_publishes = self.dropped_publishes.clone();
        let stop = self.stop.clone();

        let handle = std::thread::spawn(move || {
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                match crate::utils::log::LOGGER.read_unread_logs() {
                    Some(logs) => {
                        let message = Self::publish_logs(logs);
                        let Ok(socket) = pub_socket.lock() else {
                            break;
                        };
                        Self::fan_out(&socket, LOG_TOPIC, message, &dropped_publishes);
                    }
                    None => std::thread::sleep(Duration::from_millis(100)),
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
        join_running(&self.threads);
        info!("Tarwyn server has been stopped.");
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
    use tarwyn_protobuf::protobuf::GetDataCommand;

    fn valid_push(channel: &str, value: &str) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues {
                    kind: Some(supported_values::Kind::String(value.to_string())),
                }),
            })),
        }
        .encode_to_vec()
    }

    fn valueless_push(channel: &str) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: None,
            })),
        }
        .encode_to_vec()
    }

    fn get_request(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Data(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    fn read_string(bytes: &[u8]) -> String {
        let reply = Reply::decode(bytes).expect("the server sent something that is not a Reply");
        match reply.payload {
            Some(reply::Payload::Data(command)) => match command.value.and_then(|value| value.kind)
            {
                Some(supported_values::Kind::String(value)) => value,
                other => panic!("expected a string, got {other:?}"),
            },
            other => panic!("expected a data reply, got {other:?}"),
        }
    }

    fn requester(context: &Context, port: u16) -> zmq::Socket {
        let socket = context.socket(zmq::SocketType::REQ).unwrap();
        socket.set_rcvtimeo(3000).unwrap();
        socket.set_sndtimeo(3000).unwrap();
        socket.connect(&format!("tcp://127.0.0.1:{port}")).unwrap();
        socket
    }

    fn string(value: &str) -> supported_values::Kind {
        supported_values::Kind::String(value.to_string())
    }

    fn wrap(kind: supported_values::Kind) -> Option<Box<SupportedValues>> {
        Some(Box::new(SupportedValues { kind: Some(kind) }))
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
    fn a_malformed_push_does_not_stop_the_write_path() {
        let server = TarwynServer::with_ports_and_telemetry(21841, 21842, 21843, 21844);
        server.start();

        let context = Context::new();
        let push = context.socket(zmq::SocketType::PUSH).unwrap();
        push.connect("tcp://127.0.0.1:21842").unwrap();
        std::thread::sleep(Duration::from_millis(200));

        push.send(&[][..], 0).unwrap();
        push.send(&[0xff, 0xff, 0xff][..], 0).unwrap();
        push.send(valueless_push("survives"), 0).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        push.send(valid_push("survives", "still here"), 0).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let req = requester(&context, 21843);
        req.send(get_request("survives"), 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("the server stopped answering after a malformed push");

        assert_eq!(read_string(&bytes), "still here");
        server.stop();
    }

    #[test]
    fn stop_stops_answering_rather_than_serving_one_more_request() {
        let server = TarwynServer::with_ports_and_telemetry(21901, 21902, 21903, 21904);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let context = Context::new();
        let req = requester(&context, 21903);
        req.send(get_request("anything"), 0).unwrap();
        req.recv_bytes(0)
            .expect("the server did not answer while it was running");

        server.stop();
        std::thread::sleep(Duration::from_millis(2 * POLL_INTERVAL_MS as u64));

        req.set_rcvtimeo(500).unwrap();
        req.set_req_relaxed(true).unwrap();
        req.send(get_request("anything"), 0).unwrap();
        assert!(
            req.recv_bytes(0).is_err(),
            "the server kept answering after stop(), so a blocking recv only \
             looks at the stop flag once the next message arrives"
        );
    }

    #[test]
    fn stop_joins_its_loops_so_the_sockets_can_be_picked_up_again() {
        let server = TarwynServer::with_ports_and_telemetry(21905, 21906, 21907, 21908);
        server.start();
        std::thread::sleep(Duration::from_millis(200));
        server.stop();

        assert!(
            server.rep_socket.try_lock().is_ok(),
            "a receive loop still held the REP socket after stop() returned, so a \
             later start() would block on it for good"
        );
        assert!(
            server.threads.lock().unwrap().is_empty(),
            "stop() left thread handles behind"
        );
    }

    #[test]
    fn a_malformed_request_is_answered_so_the_socket_stays_usable() {
        let server = TarwynServer::with_ports_and_telemetry(21851, 21852, 21853, 21854);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let context = Context::new();
        let req = requester(&context, 21853);

        req.send(&[][..], 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("a malformed request went unanswered, wedging the REQ/REP pair");
        assert_eq!(read_string(&bytes), NO_DATA_SENTINEL);

        req.send(get_request("anything"), 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("the server stopped answering after a malformed request");
        assert_eq!(read_string(&bytes), NO_DATA_SENTINEL);

        server.stop();
    }

    /// `ZMQ_XPUB_NODROP` is what turns a full subscriber queue from silent loss
    /// into a counted refusal, and `zmq` 0.10 cannot report whether it took, so
    /// the only honest check is to stall a subscriber and watch the counter.
    ///
    /// The same run without the option is asserted to stay silent, otherwise this
    /// would pass for a socket that was never configured at all.
    ///
    /// Both run over `inproc`, where the high-water mark is the whole queue. Over
    /// TCP the kernel's own socket buffers sit underneath it and hold far more
    /// than they can be asked to, by an amount that differs per platform, so the
    /// queue that has to fill here would not reliably fill at all.
    #[test]
    fn a_stalled_subscriber_is_counted_rather_than_dropped_silently() {
        fn publish_into_a_stalled_subscriber(endpoint: &str, nodrop: bool) -> u64 {
            let context = Context::new();
            let publisher = context.socket(PUB).unwrap();
            publisher.set_sndhwm(2).unwrap();
            publisher.set_sndtimeo(PUB_SEND_TIMEOUT_MS).unwrap();
            publisher.set_linger(0).unwrap();
            let publisher = if nodrop {
                deny_dropping(publisher)
            } else {
                publisher
            };
            publisher.bind(endpoint).unwrap();

            let subscriber = context.socket(zmq::SocketType::SUB).unwrap();
            subscriber.set_rcvhwm(1).unwrap();
            subscriber.set_linger(0).unwrap();
            subscriber.set_subscribe(b"stalled").unwrap();
            subscriber.connect(endpoint).unwrap();
            std::thread::sleep(Duration::from_millis(200));

            let dropped = AtomicU64::new(0);
            for _ in 0..64 {
                TarwynServer::fan_out(&publisher, "stalled", vec![7u8; 64], &dropped);
            }
            dropped.load(Ordering::Relaxed)
        }

        assert!(
            publish_into_a_stalled_subscriber("inproc://nodrop-counted", true) > 0,
            "a subscriber that never reads should make the fan-out report EAGAIN, \
             not discard the message where nobody can see it"
        );
        assert_eq!(
            publish_into_a_stalled_subscriber("inproc://nodrop-absent", false),
            0,
            "without ZMQ_XPUB_NODROP libzmq drops silently, so a counter that still \
             moves here is counting something other than the option under test"
        );
    }

    /// A refused fan-out has to refuse the whole two-part message. If libzmq
    /// ever accepted the topic and refused the payload, the socket would be left
    /// mid-message and the next publish would be spliced onto it, handing
    /// subscribers a topic frame where the payload belongs.
    #[test]
    fn a_publish_after_a_refused_one_arrives_whole() {
        let context = Context::new();
        let publisher = context.socket(PUB).unwrap();
        publisher.set_sndhwm(2).unwrap();
        publisher.set_sndtimeo(PUB_SEND_TIMEOUT_MS).unwrap();
        publisher.set_linger(0).unwrap();
        let publisher = deny_dropping(publisher);
        publisher.bind("inproc://nodrop-splice").unwrap();

        let subscriber = context.socket(zmq::SocketType::SUB).unwrap();
        subscriber.set_rcvhwm(1).unwrap();
        subscriber.set_linger(0).unwrap();
        subscriber.set_subscribe(b"").unwrap();
        subscriber.set_rcvtimeo(500).unwrap();
        subscriber.connect("inproc://nodrop-splice").unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let dropped = AtomicU64::new(0);
        for _ in 0..64 {
            TarwynServer::fan_out(&publisher, "spliced", vec![7u8; 64], &dropped);
        }
        assert!(
            dropped.load(Ordering::Relaxed) > 0,
            "the queue never filled"
        );

        while subscriber.recv_bytes(0).is_ok() {}
        std::thread::sleep(Duration::from_millis(200));
        TarwynServer::fan_out(&publisher, "recovered", b"payload".to_vec(), &dropped);

        let topic = subscriber
            .recv_string(0)
            .expect("nothing arrived after the queue drained")
            .expect("the topic frame was not valid UTF-8");
        assert_eq!(
            topic, "recovered",
            "the first frame after a refused payload must open a new message, not \
             continue the one that failed"
        );
        assert!(
            subscriber.get_rcvmore().unwrap(),
            "the topic frame arrived without its payload"
        );
        assert_eq!(subscriber.recv_bytes(0).unwrap(), b"payload".to_vec());
    }

    /// A server that goes out of scope has to release its ports, or a process
    /// that builds one per run leaks a set of bound sockets and a set of loops
    /// each time.
    #[test]
    fn dropping_a_server_releases_its_ports() {
        {
            let server = TarwynServer::with_ports_and_telemetry(21991, 21992, 21993, 21994);
            server.start();
            std::thread::sleep(Duration::from_millis(200));
        }

        let context = Context::new();
        for port in [21991, 21992, 21993] {
            let socket = context.socket(REP).unwrap();
            socket.set_linger(0).unwrap();
            assert!(
                socket.bind(&format!("tcp://127.0.0.1:{port}")).is_ok(),
                "port {port} was still bound after the server was dropped"
            );
        }
        assert!(
            std::net::UdpSocket::bind(("127.0.0.1", 21994)).is_ok(),
            "the telemetry port was still bound after the server was dropped"
        );
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

    /// A port the server cannot take used to panic out of the constructor, which
    /// on a coprocessor means a service that dies at boot with a backtrace
    /// instead of a line saying which port it wanted.
    #[test]
    fn a_port_that_stays_taken_is_reported_rather_than_panicking() {
        let context = Context::new();
        let squatter = context.socket(REP).unwrap();
        squatter.set_linger(0).unwrap();
        squatter.bind("tcp://*:22021").unwrap();

        let error = TarwynServer::try_with_ports_and_telemetry(22021, 22022, 22023, 22024)
            .expect_err("the PUB port was already bound, so this cannot succeed");

        assert!(
            matches!(
                error,
                BindError::Bind {
                    socket: "PUB",
                    port: 22021,
                    ..
                }
            ),
            "the error has to name the socket and the port, got {error:?}"
        );
    }

    /// A port held by something on its way out - a previous instance closing, or
    /// an outbound connection the kernel allocated - should cost a start-up
    /// delay, not the start-up.
    #[test]
    fn a_port_freed_while_the_bind_retries_is_still_taken() {
        let context = Context::new();
        let squatter = context.socket(REP).unwrap();
        squatter.set_linger(0).unwrap();
        squatter.bind("tcp://*:22031").unwrap();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(squatter);
        });

        TarwynServer::try_with_ports_and_telemetry(22031, 22032, 22033, 22034)
            .expect("the port was released well inside the retry window");
    }
}
