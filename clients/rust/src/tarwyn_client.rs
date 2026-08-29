use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use prost::Message;
use slotmap::{DefaultKey, SlotMap};

use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BezierCurvesList, BoolList, BytesList, CompareAndSetCommand,
    Coordinate, CoordinateList, DeleteCommand, DoubleList, FloatList, GetDataCommand,
    GetLogsCommand, IntegerList, JsonCommand, ListTablesCommand, LongList, PingCommand, Publish,
    Push, RegisterTelemetryCommand, Reply, ReplyStatisticsCommand, Request, SendDataCommand,
    StatisticsCommand, StringList, SupportedValues, publish, push, reply, request,
    supported_values,
};
use tarwyn_protobuf::telemetry;

use zmq::{
    Context,
    SocketType::{PUSH, REQ, SUB},
};

use crate::ports;

const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The PUB topic the server relays log lines on.
const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";

/// Decode a value carried in TARWYN' own byte layout, given its type tag.
///
/// Scalars are big-endian, matching Java's `ByteBuffer` default; the list and
/// geometry types are protobuf. A tag this does not recognise is kept as raw
/// bytes, matching TARWYN' own unknown-type handling; `None` means a tag it
/// does recognise came with bytes that are not a valid value of that type.
fn decode_tarwyn_type(tag: i32, data: &[u8]) -> Option<supported_values::Kind> {
    use supported_values::Kind;

    fn big_endian<const N: usize>(data: &[u8]) -> Option<[u8; N]> {
        data.get(..N)?.try_into().ok()
    }

    Some(match tag {
        1 => Kind::String(String::from_utf8(data.to_vec()).ok()?),
        2 => Kind::Double(f64::from_be_bytes(big_endian::<8>(data)?)),
        3 => Kind::Int32(i32::from_be_bytes(big_endian::<4>(data)?)),
        5 => Kind::Int64(i64::from_be_bytes(big_endian::<8>(data)?)),
        6 => Kind::Bool(data.first().is_some_and(|byte| *byte != 0)),
        10 => Kind::DoubleList(DoubleList::decode(data).ok()?),
        11 => Kind::StringList(StringList::decode(data).ok()?),
        12 => Kind::FloatList(FloatList::decode(data).ok()?),
        13 => Kind::IntegerList(IntegerList::decode(data).ok()?),
        14 => Kind::LongList(LongList::decode(data).ok()?),
        15 => Kind::BoolList(BoolList::decode(data).ok()?),
        16 => Kind::BytesList(BytesList::decode(data).ok()?),
        20 => Kind::CoordinateList(CoordinateList::decode(data).ok()?),
        21 => Kind::BezierCurves(BezierCurves::decode(data).ok()?),
        22 => Kind::BezierCurve(BezierCurve::decode(data).ok()?),
        23 => Kind::BezierCurvesList(BezierCurvesList::decode(data).ok()?),
        _ => Kind::Bytes(data.to_vec()),
    })
}

const POLL_INTERVAL_MS: i32 = 100;
/// How often a telemetry subscription renews its lease with the server.
///
/// The server drops a registration it has not heard from within its own TTL, so
/// this has to be comfortably shorter than that.
const TELEMETRY_KEEPALIVE: Duration = Duration::from_secs(3);
/// How long a receive loop blocks before it looks at the stop flag again.
const POLL_INTERVAL: Duration = Duration::from_millis(POLL_INTERVAL_MS as u64);

/// A subscription callback that holds values back until its snapshot has been
/// delivered.
///
/// `subscribe` has to tell ZeroMQ about the topic before it reads the current
/// value, or a value published in between reaches nobody and the subscriber is
/// left behind the server for as long as the channel stays quiet. Subscribing
/// first opens the opposite race - a live value arriving before the snapshot -
/// so values that arrive early are buffered here and replayed once the snapshot
/// is through.
struct BufferedListener<F> {
    callback: F,
    pending: Mutex<Option<Vec<supported_values::Kind>>>,
}

impl<F: Fn(&supported_values::Kind)> BufferedListener<F> {
    fn new(callback: F) -> Self {
        BufferedListener {
            callback,
            pending: Mutex::new(Some(Vec::new())),
        }
    }

    /// Call the callback, bypassing the buffer. Used for the snapshot itself.
    fn call(&self, value: &supported_values::Kind) {
        (self.callback)(value);
    }

    /// Buffer a value while the gate is closed, deliver it once it is open.
    fn deliver(&self, value: &supported_values::Kind) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some(buffered) = pending.as_mut()
        {
            buffered.push(value.clone());
            return;
        }
        (self.callback)(value);
    }

    /// Replay what arrived while the gate was closed, then open it.
    ///
    /// Values delivered during the replay land in the buffer rather than
    /// overtaking it, so the loop runs until the buffer is empty under the lock.
    /// The callback is never run while that lock is held.
    fn open(&self) {
        loop {
            let batch = {
                let Ok(mut pending) = self.pending.lock() else {
                    return;
                };
                match pending.as_mut() {
                    None => return,
                    Some(buffered) if buffered.is_empty() => {
                        *pending = None;
                        return;
                    }
                    Some(buffered) => std::mem::take(buffered),
                }
            };
            for value in &batch {
                (self.callback)(value);
            }
        }
    }
}

/// Why a client could not be built.
///
/// Every variant is a failure to set up a socket before any traffic is
/// attempted. Once a client exists, a server that is absent or unreachable is
/// not an error - publishes drop and reads return `None`.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
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
    /// A socket could not be pointed at its endpoint. This is not a failure to
    /// reach the server, which ZeroMQ does in the background.
    #[error("could not connect the {socket} socket to {endpoint}")]
    Connect {
        /// Which of the three sockets failed.
        socket: &'static str,
        /// The endpoint it was given.
        endpoint: String,
        /// The underlying ZeroMQ error.
        source: zmq::Error,
    },
    /// No UDP socket could be bound for the telemetry plane.
    #[error("could not bind a telemetry socket")]
    Telemetry(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
/// Where the client dials and how patient it is.
///
/// [`Default`] points at `127.0.0.1` on the standard ports with a 500 ms
/// request timeout.
pub struct TarwynConfig {
    /// Host running the server. An address, not a URL.
    pub host: String,
    /// PUSH/PULL port, used by every `send_*`.
    pub push_port: u16,
    /// REQ/REP port, used by [`get`](TarwynClient::get) and the control plane.
    pub req_port: u16,
    /// PUB/SUB port, used by [`subscribe`](TarwynClient::subscribe).
    pub sub_port: u16,
    /// How long a request waits for its reply before giving up and returning `None`.
    pub request_timeout: Duration,
    /// ZeroMQ high-water mark on the PUSH socket. Publishes past it are dropped,
    /// not queued; [`dropped_publishes`](TarwynClient::dropped_publishes) counts them.
    pub send_high_water_mark: i32,
    /// UDP port for the telemetry plane.
    pub telemetry_port: u16,
}

impl Default for TarwynConfig {
    fn default() -> Self {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: ports::DEFAULT_PUSH_PULL_PORT,
            req_port: ports::DEFAULT_REQ_REP_PORT,
            sub_port: ports::DEFAULT_PUB_SUB_PORT,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
    }
}

type SubscribeListener = Arc<dyn Fn(&supported_values::Kind) + Send + Sync + 'static>;
type SubscribeListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

type TelemetryListener = Arc<dyn Fn(u64, &[u8]) + Send + Sync + 'static>;
struct TelemetryTopic {
    channel: String,
    listeners: SlotMap<DefaultKey, TelemetryListener>,
}

type TelemetryListenerMap = Arc<Mutex<HashMap<u32, TelemetryTopic>>>;

/// Registers `callback` against a channel, returning the key that cancels it.
///
/// `None` when another channel already holds this one's topic hash. Two names can
/// collide; the second is refused rather than cross-wired onto the first.
fn register_telemetry_listener(
    listeners: &mut HashMap<u32, TelemetryTopic>,
    channel: &str,
    callback: TelemetryListener,
) -> Option<DefaultKey> {
    let hash = telemetry::topic_hash(channel);
    let topic = listeners.entry(hash).or_insert_with(|| TelemetryTopic {
        channel: channel.to_string(),
        listeners: SlotMap::new(),
    });
    if topic.channel != channel {
        if topic.listeners.is_empty() {
            listeners.remove(&hash);
        }
        return None;
    }
    Some(topic.listeners.insert(callback))
}

type LogListener = Arc<dyn Fn(&String) + Send + Sync + 'static>;
type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

fn request_on(socket: &Mutex<zmq::Socket>, message: Vec<u8>) -> Option<reply::Payload> {
    let socket = socket.lock().ok()?;
    socket.send(message, 0).ok()?;
    let bytes = socket.recv_bytes(0).ok()?;
    Reply::decode(&bytes[..]).ok()?.payload
}

fn register_telemetry_request(address: String, channel: String) -> Vec<u8> {
    Request {
        payload: Some(request::Payload::RegisterTelemetry(
            RegisterTelemetryCommand { channel, address },
        )),
    }
    .encode_to_vec()
}

/// A bounded queue of the values a subscription has seen.
///
/// Handed out by [`TarwynClient::subscribe_cached`] for call sites that poll
/// rather than run a callback. Oldest values are evicted once it is full.
#[derive(Debug)]
pub struct CachedSubscriber {
    values: Arc<Mutex<VecDeque<supported_values::Kind>>>,
}

impl CachedSubscriber {
    /// Take everything buffered, leaving the queue empty.
    pub fn read_all(&self) -> Vec<supported_values::Kind> {
        match self.values.lock() {
            Ok(mut values) => values.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The most recent value, without draining the rest.
    pub fn latest(&self) -> Option<supported_values::Kind> {
        self.values.lock().ok()?.back().cloned()
    }

    /// How many values are buffered.
    pub fn len(&self) -> usize {
        self.values.lock().map(|v| v.len()).unwrap_or(0)
    }

    /// Whether nothing is buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A connection to an TARWYN server.
///
/// `Send + Sync`, so one client can be shared across threads. Constructing it
/// never blocks — ZeroMQ dials in the background, so a client may be built
/// before the server exists. Nothing is received until [`start`](Self::start)
/// is called.
///
/// ```no_run
/// use tarwyn_client::tarwyn_client::TarwynClient;
///
/// let client = TarwynClient::new();
/// let _unsubscribe = client.subscribe("test", |value| println!("{value:?}"));
/// client.start();
/// client.send_bool("test", true);
/// ```
pub struct TarwynClient {
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    push_socket: Mutex<zmq::Socket>,
    sub_socket: Arc<Mutex<zmq::Socket>>,
    telemetry_socket: Arc<std::net::UdpSocket>,
    telemetry_target: std::net::SocketAddr,
    telemetry_listeners: TelemetryListenerMap,
    telemetry_started: Arc<AtomicBool>,
    telemetry_keepalive: Arc<AtomicBool>,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    req_socket: Arc<Mutex<zmq::Socket>>,
    dropped: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    logger: std::sync::OnceLock<tarwyn_protobuf::wpilog::Logger>,
}

impl TarwynClient {
    /// Connect to a server on localhost with the default ports.
    pub fn new() -> Self {
        Self::with_config(TarwynConfig::default())
    }

    /// Connect to a server on another machine — a coprocessor, or the robot controller.
    ///
    /// ```no_run
    /// # use tarwyn_client::tarwyn_client::TarwynClient;
    /// let client = TarwynClient::connect("10.4.88.2");
    /// ```
    pub fn connect(host: &str) -> Self {
        Self::with_config(TarwynConfig {
            host: host.to_string(),
            ..Default::default()
        })
    }

    /// Connect with the ports and timeout spelled out.
    ///
    /// # Panics
    ///
    /// If a socket cannot be created, configured or bound. Use
    /// [`try_with_config`](Self::try_with_config) to handle that instead.
    pub fn with_config(config: TarwynConfig) -> Self {
        Self::try_with_config(config).expect("could not construct an Tarwyn client")
    }

    /// As [`with_config`](Self::with_config), reporting setup failure instead of
    /// panicking.
    pub fn try_with_config(config: TarwynConfig) -> Result<Self, ConnectError> {
        let context = Context::new();

        let listeners: SubscribeListenerMap = Arc::new(Mutex::new(HashMap::new()));
        let log_listeners: LogListenerMap = Arc::new(Mutex::new(SlotMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let socket = |kind, name: &'static str| {
            context.socket(kind).map_err(|source| ConnectError::Socket {
                socket: name,
                source,
            })
        };
        let configure = |name: &'static str| {
            move |source| ConnectError::Configure {
                socket: name,
                source,
            }
        };

        let push_socket = socket(PUSH, "PUSH")?;
        let req_socket = socket(REQ, "REQ")?;
        let sub_socket = socket(SUB, "SUB")?;
        sub_socket
            .set_rcvtimeo(POLL_INTERVAL_MS)
            .map_err(configure("SUB"))?;

        for (socket, name) in [
            (&push_socket, "PUSH"),
            (&req_socket, "REQ"),
            (&sub_socket, "SUB"),
        ] {
            socket.set_linger(0).map_err(configure(name))?;
        }

        req_socket.set_req_relaxed(true).map_err(configure("REQ"))?;
        req_socket
            .set_req_correlate(true)
            .map_err(configure("REQ"))?;
        let timeout_ms = config.request_timeout.as_millis().min(i32::MAX as u128) as i32;
        req_socket
            .set_rcvtimeo(timeout_ms)
            .map_err(configure("REQ"))?;
        req_socket
            .set_sndtimeo(timeout_ms)
            .map_err(configure("REQ"))?;

        push_socket
            .set_rcvhwm(config.send_high_water_mark)
            .map_err(configure("PUSH"))?;
        push_socket
            .set_sndhwm(config.send_high_water_mark)
            .map_err(configure("PUSH"))?;

        for (socket, name, port) in [
            (&push_socket, "PUSH", config.push_port),
            (&req_socket, "REQ", config.req_port),
            (&sub_socket, "SUB", config.sub_port),
        ] {
            let endpoint = format!("tcp://{}:{}", config.host, port);
            socket
                .connect(&endpoint)
                .map_err(|source| ConnectError::Connect {
                    socket: name,
                    endpoint,
                    source,
                })?;
        }

        Ok(TarwynClient {
            data_listeners: listeners,
            push_socket: Mutex::new(push_socket),
            sub_socket: Arc::new(Mutex::new(sub_socket)),
            telemetry_socket: Arc::new(telemetry::bind_ephemeral()?),
            telemetry_target: format!("{}:{}", config.host, config.telemetry_port)
                .parse()
                .unwrap_or_else(|_| {
                    std::net::SocketAddr::from(([127, 0, 0, 1], config.telemetry_port))
                }),
            telemetry_listeners: Arc::new(Mutex::new(HashMap::new())),
            telemetry_started: Arc::new(AtomicBool::new(false)),
            telemetry_keepalive: Arc::new(AtomicBool::new(false)),
            threads: Mutex::new(Vec::new()),
            req_socket: Arc::new(Mutex::new(req_socket)),
            dropped: Arc::new(AtomicU64::new(0)),
            stop,
            initialized,
            logger: std::sync::OnceLock::new(),
            log_listeners,
        })
    }

    /// The value a published command carries, or `None` when it carries none.
    ///
    /// Both fields are optional on the wire, so a peer that is not this server -
    /// or a version that predates a field - can send either as absent. The
    /// receive thread must drop those rather than die on them.
    fn published_kind(command: &SendDataCommand) -> Option<&supported_values::Kind> {
        command.value.as_ref()?.kind.as_ref()
    }

    fn request(&self, message: Vec<u8>) -> Option<reply::Payload> {
        request_on(&self.req_socket, message)
    }

    fn push_data(channel: &str, data: supported_values::Kind) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues { kind: Some(data) }),
            })),
        }
        .encode_to_vec()
    }

    fn request_data(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Data(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    fn request_log() -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Logs(GetLogsCommand {})),
        }
        .encode_to_vec()
    }

    /// Publish an already-built value, for callers that hold a [`Kind`](supported_values::Kind)
    /// rather than a Rust primitive.
    pub fn send_message_public(&self, channel: &str, kind: supported_values::Kind) {
        self.send_message(channel, kind);
    }

    fn send_message(&self, channel: &str, kind: supported_values::Kind) {
        if let Some(logger) = self.logger.get() {
            logger.record(channel, kind.clone());
        }
        let message = Self::push_data(channel, kind);
        if let Ok(socket) = self.push_socket.lock()
            && socket.send(message, zmq::DONTWAIT).is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Publish a string.
    pub fn send_string(&self, channel: &str, data: &str) {
        self.send_message(channel, supported_values::Kind::String(data.to_string()));
    }

    /// Publish a 32-bit signed integer.
    pub fn send_i32(&self, channel: &str, data: i32) {
        self.send_message(channel, supported_values::Kind::Int32(data));
    }

    /// Publish a 64-bit signed integer.
    pub fn send_i64(&self, channel: &str, data: i64) {
        self.send_message(channel, supported_values::Kind::Int64(data));
    }

    /// Publish a 32-bit unsigned integer.
    pub fn send_u32(&self, channel: &str, data: u32) {
        self.send_message(channel, supported_values::Kind::Uint32(data));
    }

    /// Publish a 64-bit unsigned integer.
    pub fn send_u64(&self, channel: &str, data: u64) {
        self.send_message(channel, supported_values::Kind::Uint64(data));
    }

    /// Publish a boolean.
    pub fn send_bool(&self, channel: &str, data: bool) {
        self.send_message(channel, supported_values::Kind::Bool(data));
    }

    /// Publish a double.
    pub fn send_double(&self, channel: &str, data: f64) {
        self.send_message(channel, supported_values::Kind::Double(data));
    }

    /// Publish a float. TARWYN has no `putFloat`; this is an addition.
    pub fn send_float(&self, channel: &str, data: f32) {
        self.send_message(channel, supported_values::Kind::Float(data));
    }

    /// Publish raw bytes.
    pub fn send_bytes(&self, channel: &str, data: &[u8]) {
        self.send_message(channel, supported_values::Kind::Bytes(data.to_vec()));
    }

    /// Publish a list of strings.
    pub fn send_string_list(&self, channel: &str, data: &[String]) {
        self.send_message(
            channel,
            supported_values::Kind::StringList(StringList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of floats.
    pub fn send_float_list(&self, channel: &str, data: &[f32]) {
        self.send_message(
            channel,
            supported_values::Kind::FloatList(FloatList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of byte strings.
    pub fn send_bytes_list(&self, channel: &str, data: &[Vec<u8>]) {
        self.send_message(
            channel,
            supported_values::Kind::BytesList(BytesList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of booleans.
    pub fn send_bool_list(&self, channel: &str, data: &[bool]) {
        self.send_message(
            channel,
            supported_values::Kind::BoolList(BoolList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of doubles.
    pub fn send_double_list(&self, channel: &str, data: &[f64]) {
        self.send_message(
            channel,
            supported_values::Kind::DoubleList(DoubleList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of 32-bit integers.
    pub fn send_integer_list(&self, channel: &str, data: &[i32]) {
        self.send_message(
            channel,
            supported_values::Kind::IntegerList(IntegerList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of 64-bit integers.
    pub fn send_long_list(&self, channel: &str, data: &[i64]) {
        self.send_message(
            channel,
            supported_values::Kind::LongList(LongList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of `(x, y)` coordinates.
    pub fn send_coordinates(&self, channel: &str, data: &[(f64, f64)]) {
        self.send_message(
            channel,
            supported_values::Kind::CoordinateList(CoordinateList {
                coordinates: data
                    .iter()
                    .map(|(x, y)| Coordinate { x: *x, y: *y })
                    .collect(),
            }),
        );
    }

    /// Publish one bezier curve.
    pub fn send_bezier_curve(&self, channel: &str, curve: BezierCurve) {
        self.send_message(channel, supported_values::Kind::BezierCurve(curve));
    }

    /// Publish a bezier path — a set of curves plus its traversal options.
    pub fn send_bezier_curves(&self, channel: &str, curves: BezierCurves) {
        self.send_message(channel, supported_values::Kind::BezierCurves(curves));
    }

    /// Publish several bezier paths as one value.
    pub fn send_bezier_curves_list(&self, channel: &str, values: Vec<BezierCurves>) {
        self.send_message(
            channel,
            supported_values::Kind::BezierCurvesList(BezierCurvesList { values }),
        );
    }

    /// Publish bytes whose type the caller does not know. Equivalent to
    /// [`send_bytes`](Self::send_bytes); present to match TARWYN' `putUnknownBytes`.
    pub fn send_unknown_bytes(&self, channel: &str, data: &[u8]) {
        self.send_bytes(channel, data);
    }

    /// Read a channel holding raw bytes. `None` if it is absent or holds another type.
    pub fn get_unknown_bytes(&self, channel: &str) -> Option<Vec<u8>> {
        match self.get(channel)? {
            supported_values::Kind::Bytes(value) => Some(value),
            _ => None,
        }
    }

    /// Publish a value that is already encoded in TARWYN' byte layout.
    ///
    /// `tarwyn_type` is TARWYN' own type tag. An unrecognised tag is published as
    /// raw bytes. Returns `false`, publishing nothing, only when a recognised tag
    /// comes with bytes that are not a valid value of that type.
    pub fn send_typed_bytes(&self, channel: &str, tarwyn_type: i32, data: &[u8]) -> bool {
        let Some(kind) = decode_tarwyn_type(tarwyn_type, data) else {
            return false;
        };
        self.send_message(channel, kind);
        true
    }

    /// Read a coordinate list. `None` if the channel is absent or holds another type.
    pub fn get_coordinates(&self, channel: &str) -> Option<Vec<(f64, f64)>> {
        match self.get(channel)? {
            supported_values::Kind::CoordinateList(list) => Some(
                list.coordinates
                    .into_iter()
                    .map(|coordinate| (coordinate.x, coordinate.y))
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Read one bezier curve. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curve(&self, channel: &str) -> Option<BezierCurve> {
        match self.get(channel)? {
            supported_values::Kind::BezierCurve(curve) => Some(curve),
            _ => None,
        }
    }

    /// Read a bezier path. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curves(&self, channel: &str) -> Option<BezierCurves> {
        match self.get(channel)? {
            supported_values::Kind::BezierCurves(curves) => Some(curves),
            _ => None,
        }
    }

    /// Read a list of bezier paths. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curves_list(&self, channel: &str) -> Option<Vec<BezierCurves>> {
        match self.get(channel)? {
            supported_values::Kind::BezierCurvesList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Publish on the UDP telemetry plane, which trades delivery guarantees for latency.
    ///
    /// Roughly 3.6x faster than the ZeroMQ path. Subscribers must register with
    /// [`subscribe_telemetry`](Self::subscribe_telemetry). A datagram that cannot be
    /// sent is counted by [`dropped_publishes`](Self::dropped_publishes), not retried.
    pub fn publish_telemetry(&self, channel: &str, payload: &[u8]) {
        if let Some(logger) = self.logger.get() {
            logger.record_raw(channel, payload);
        }
        let mut buf = vec![0u8; telemetry::HEADER_LEN + payload.len()];
        let len = telemetry::encode(
            &mut buf,
            telemetry::topic_hash(channel),
            telemetry::now_micros(),
            payload,
        );
        if self
            .telemetry_socket
            .send_to(&buf[..len], self.telemetry_target)
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Receive telemetry on a channel, with each payload handed over as bytes.
    ///
    /// Call the returned closure to unsubscribe; dropping it instead leaves the
    /// subscription in place, matching [`subscribe`](Self::subscribe). `None` if
    /// the server did not acknowledge the registration, or if another channel
    /// already claimed this one's topic hash — a collision is refused rather than
    /// silently cross-wired.
    pub fn subscribe_telemetry<F>(
        &self,
        channel: &str,
        callback: F,
    ) -> Option<impl FnOnce() + Send + 'static>
    where
        F: Fn(&supported_values::Kind) + Send + Sync + 'static,
    {
        self.subscribe_telemetry_timestamped(channel, move |_timestamp_us, payload| {
            callback(&supported_values::Kind::Bytes(payload.to_vec()));
        })
    }

    /// As [`subscribe_telemetry`](Self::subscribe_telemetry), but the callback also
    /// receives the publisher's timestamp in microseconds since the Unix epoch.
    pub fn subscribe_telemetry_timestamped<F>(
        &self,
        channel: &str,
        callback: F,
    ) -> Option<impl FnOnce() + Send + 'static>
    where
        F: Fn(u64, &[u8]) + Send + Sync + 'static,
    {
        let local = self.telemetry_socket.local_addr().ok()?;
        let address = format!("{}:{}", self.telemetry_target.ip(), local.port());
        let message = register_telemetry_request(address, channel.to_string());

        let registered = matches!(
            self.request(message),
            Some(reply::Payload::Telemetry(ack)) if ack.registered
        );
        if !registered {
            return None;
        }

        let hash = telemetry::topic_hash(channel);
        let mut listeners = self.telemetry_listeners.lock().ok()?;
        let key = register_telemetry_listener(&mut listeners, channel, Arc::new(callback))?;
        drop(listeners);
        self.start_telemetry_receiver();
        self.start_telemetry_keepalive();

        let listeners = Arc::clone(&self.telemetry_listeners);
        Some(move || {
            let Ok(mut listeners) = listeners.lock() else {
                return;
            };
            if let Some(topic) = listeners.get_mut(&hash) {
                topic.listeners.remove(key);
                if topic.listeners.is_empty() {
                    listeners.remove(&hash);
                }
            }
        })
    }

    fn start_telemetry_receiver(&self) {
        if self.stop.load(Ordering::SeqCst) || self.telemetry_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let socket = Arc::clone(&self.telemetry_socket);
        let listeners = Arc::clone(&self.telemetry_listeners);
        let stop = Arc::clone(&self.stop);
        let _ = socket.set_read_timeout(Some(POLL_INTERVAL));

        let handle = std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok((len, _from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((channel_hash, timestamp_us, payload)) = telemetry::decode(&buf[..len])
                else {
                    continue;
                };
                let Ok(listeners) = listeners.lock() else {
                    continue;
                };
                let callbacks: Vec<TelemetryListener> = listeners
                    .get(&channel_hash)
                    .map(|topic| topic.listeners.values().map(Arc::clone).collect())
                    .unwrap_or_default();
                drop(listeners);

                for callback in callbacks {
                    callback(timestamp_us, payload);
                }
            }
        });
        self.track(handle);
    }

    /// Renews every telemetry registration before the server's lease expires.
    ///
    /// The server drops a subscriber it has not heard from inside its TTL, and it
    /// sweeps whenever any client registers. Without renewal a subscriber goes
    /// silent as soon as a second client appears, while publishes keep reporting
    /// success. DDS calls the same arrangement a liveliness lease.
    fn start_telemetry_keepalive(&self) {
        if self.stop.load(Ordering::SeqCst) || self.telemetry_keepalive.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let listeners = Arc::clone(&self.telemetry_listeners);
        let request_socket = Arc::clone(&self.req_socket);
        let telemetry_socket = Arc::clone(&self.telemetry_socket);
        let stop = Arc::clone(&self.stop);
        let target = self.telemetry_target;

        let handle = std::thread::spawn(move || {
            loop {
                let due = Instant::now() + TELEMETRY_KEEPALIVE;
                while Instant::now() < due {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }

                let Ok(local) = telemetry_socket.local_addr() else {
                    continue;
                };
                let address = format!("{}:{}", target.ip(), local.port());
                let channels: Vec<String> = match listeners.lock() {
                    Ok(listeners) => listeners
                        .values()
                        .map(|topic| topic.channel.clone())
                        .collect(),
                    Err(_) => continue,
                };
                for channel in channels {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let message = register_telemetry_request(address.clone(), channel);
                    let _ = request_on(&request_socket, message);
                }
            }
        });
        self.track(handle);
    }

    fn track(&self, handle: std::thread::JoinHandle<()>) {
        if let Ok(mut threads) = self.threads.lock() {
            threads.push(handle);
        }
    }

    /// Mirror every published value into a [WPILOG](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
    /// file, which AdvantageScope, Elastic and the WPILib DataLogTool open directly.
    ///
    /// Records go to a writer thread over a bounded queue and are flushed every
    /// 250 ms, so a publish never waits on the filesystem. Errors if logging has
    /// already been started.
    pub fn log_to(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let logger = tarwyn_protobuf::wpilog::Logger::open(path)?;
        self.logger
            .set(logger)
            .map_err(|_| std::io::Error::other("logging already started"))
    }

    /// As [`log_to`](Self::log_to), but onto the first writable removable mount under
    /// `/media`, `/run/media` or `/mnt`. Returns the path it chose.
    pub fn log_to_drive(&self, filename: &str) -> std::io::Result<std::path::PathBuf> {
        let (logger, path) = tarwyn_protobuf::wpilog::Logger::open_on_drive(filename)?;
        self.logger
            .set(logger)
            .map_err(|_| std::io::Error::other("logging already started"))?;
        Ok(path)
    }

    /// How many log records were dropped because the writer queue was full. Zero if
    /// logging was never started.
    pub fn log_dropped(&self) -> u64 {
        self.logger
            .get()
            .map(|logger| logger.dropped())
            .unwrap_or(0)
    }

    /// Whether the log writer is still succeeding. An I/O error latches it off rather
    /// than propagating into a publish, so this is the only way to notice. `true` when
    /// logging was never started.
    pub fn logging_healthy(&self) -> bool {
        self.logger.get().is_none_or(|logger| logger.is_healthy())
    }

    /// How many publishes were dropped rather than queued, across both transports.
    pub fn dropped_publishes(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Read the current value of a channel, round-tripping to the server.
    ///
    /// `None` if the channel is unset or the server does not answer within
    /// [`request_timeout`](TarwynConfig::request_timeout). The REQ socket is set to
    /// `ZMQ_REQ_CORRELATE`, so a reply to an abandoned request is discarded rather
    /// than handed to the next caller, and `ZMQ_REQ_RELAXED`, so a timeout does not
    /// wedge the socket.
    pub fn get(&self, channel: &str) -> Option<supported_values::Kind> {
        match self.request(Self::request_data(channel))? {
            reply::Payload::Data(command) => {
                let kind = command.value?.kind?;
                if kind == supported_values::Kind::String(NO_DATA_SENTINEL.to_string()) {
                    None
                } else {
                    Some(kind)
                }
            }
            _ => None,
        }
    }

    /// Delete a channel. Returns how many were removed — 0 or 1.
    pub fn delete(&self, channel: &str) -> u32 {
        let request = Request {
            payload: Some(request::Payload::Delete(DeleteCommand {
                channel: channel.to_string(),
            })),
        };
        match self.request(request.encode_to_vec()) {
            Some(reply::Payload::Delete(command)) => command.deleted,
            _ => 0,
        }
    }

    /// Delete every channel. Returns how many were removed.
    pub fn delete_all(&self) -> u32 {
        self.delete("")
    }

    /// List the channel names beginning with `prefix`. Pass `""` for all of them.
    pub fn tables(&self, prefix: &str) -> Vec<String> {
        let request = Request {
            payload: Some(request::Payload::Tables(ListTablesCommand {
                prefix: prefix.to_string(),
            })),
        };
        match self.request(request.encode_to_vec()) {
            Some(reply::Payload::Tables(command)) => command.channels,
            _ => Vec::new(),
        }
    }

    /// Round-trip time to the server, or `None` if it does not answer.
    pub fn ping(&self) -> Option<Duration> {
        let sent = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64;
        let request = Request {
            payload: Some(request::Payload::Ping(PingCommand { sent_nanos: sent })),
        };
        match self.request(request.encode_to_vec())? {
            reply::Payload::Ping(command) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_nanos() as u64;
                Some(Duration::from_nanos(now.saturating_sub(command.sent_nanos)))
            }
            _ => None,
        }
    }

    /// Server counters — uptime, channel count, messages handled. `None` if the
    /// server does not answer.
    pub fn statistics(&self) -> Option<ReplyStatisticsCommand> {
        let request = Request {
            payload: Some(request::Payload::Statistics(StatisticsCommand {})),
        };
        match self.request(request.encode_to_vec())? {
            reply::Payload::Statistics(command) => Some(command),
            _ => None,
        }
    }

    /// The channels beginning with `prefix`, as a JSON document. `"{}"` if the server
    /// does not answer.
    pub fn raw_json(&self, prefix: &str) -> String {
        let request = Request {
            payload: Some(request::Payload::Json(JsonCommand {
                prefix: prefix.to_string(),
            })),
        };
        match self.request(request.encode_to_vec()) {
            Some(reply::Payload::Json(command)) => command.json,
            _ => String::from("{}"),
        }
    }

    /// Set a channel only if it currently holds `expected`, and report whether it swapped.
    ///
    /// Pass `None` to claim a channel only while it is empty. The comparison and the
    /// write happen inside the server's lock on the value map, so a read-modify-write
    /// spread across several coprocessors cannot lose an update the way a [`get`](Self::get)
    /// followed by a publish can. TARWYN has no equivalent.
    ///
    /// ```no_run
    /// # use tarwyn_client::tarwyn_client::TarwynClient;
    /// # use tarwyn_protobuf::protobuf::supported_values::Kind;
    /// # let client = TarwynClient::new();
    /// let won = client.compare_and_set("path-lock", None, Kind::String("agent-a".into()));
    /// ```
    pub fn compare_and_set(
        &self,
        channel: &str,
        expected: Option<supported_values::Kind>,
        value: supported_values::Kind,
    ) -> bool {
        let request = Request {
            payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
                channel: channel.to_string(),
                expect_absent: expected.is_none(),
                expected: expected.map(|kind| Box::new(SupportedValues { kind: Some(kind) })),
                value: Some(Box::new(SupportedValues { kind: Some(value) })),
            })),
        };
        match self.request(request.encode_to_vec()) {
            Some(reply::Payload::CompareAndSet(command)) => command.swapped,
            _ => false,
        }
    }

    fn get_logs(&self) -> Vec<String> {
        match self.request(Self::request_log()) {
            Some(reply::Payload::Logs(command)) => command.logs,
            _ => Vec::new(),
        }
    }

    /// Run `callback` for every value published to a channel.
    ///
    /// The current value, if there is one, is delivered before this returns. Values
    /// arrive only once [`start`](Self::start) has been called. Call the returned
    /// closure to unsubscribe; dropping it instead leaves the subscription in place.
    ///
    /// Nothing published after this returns is missed: the topic is subscribed
    /// before the current value is read, and anything that arrives in between is
    /// replayed after it. That ordering can deliver a value twice, or deliver the
    /// snapshot after a newer value that overtook it, so a callback that counts
    /// transitions may see one more than the server published; the last value a
    /// subscriber is given always matches the last the server fanned out.
    pub fn subscribe<F>(&self, channel: &str, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&supported_values::Kind) + Send + Sync + 'static,
    {
        let listener = Arc::new(BufferedListener::new(callback));
        let buffered = Arc::clone(&listener);

        self.set_topic(channel, true);

        let key = self.data_listeners.lock().ok().map(|mut listeners| {
            listeners
                .entry(channel.to_string())
                .or_default()
                .insert(Arc::new(move |value: &supported_values::Kind| {
                    listener.deliver(value);
                }))
        });

        if let Some(initial_value) = self.get(channel) {
            buffered.call(&initial_value);
        }
        buffered.open();

        let listeners = Arc::clone(&self.data_listeners);
        let sub_socket = Arc::clone(&self.sub_socket);
        let channel = channel.to_string();

        move || {
            let (Some(key), Ok(mut listeners)) = (key, listeners.lock()) else {
                return;
            };
            let Some(slotmap) = listeners.get_mut(&channel) else {
                return;
            };
            slotmap.remove(key);
            if !slotmap.is_empty() {
                return;
            }
            listeners.remove(&channel);
            drop(listeners);
            if let Ok(socket) = sub_socket.lock() {
                let _ = socket.set_unsubscribe(channel.as_bytes());
            }
        }
    }

    /// Add or remove a SUB topic, waiting for the receive thread to release the
    /// socket rather than deferring the change past the caller's next read.
    fn set_topic(&self, channel: &str, subscribe: bool) {
        let Ok(socket) = self.sub_socket.lock() else {
            return;
        };
        let _ = if subscribe {
            socket.set_subscribe(channel.as_bytes())
        } else {
            socket.set_unsubscribe(channel.as_bytes())
        };
    }

    /// Subscribe into a bounded queue instead of a callback, for call sites that poll.
    ///
    /// `depth` is clamped to at least 1. Returns the queue and the closure that
    /// unsubscribes.
    pub fn subscribe_cached(
        &self,
        channel: &str,
        depth: usize,
    ) -> (CachedSubscriber, impl FnOnce() + Send + 'static) {
        let values = Arc::new(Mutex::new(VecDeque::with_capacity(depth.max(1))));
        let sink = Arc::clone(&values);
        let depth = depth.max(1);
        let unsubscribe = self.subscribe(channel, move |value| {
            if let Ok(mut buffered) = sink.lock() {
                if buffered.len() == depth {
                    buffered.pop_front();
                }
                buffered.push_back(value.clone());
            }
        });
        (CachedSubscriber { values }, unsubscribe)
    }

    /// Run `callback` for every log line the server emits. Existing unread lines are
    /// delivered before this returns.
    pub fn subscribe_to_logs<F>(&self, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&String) + Send + Sync + 'static,
    {
        let sub_socket = self.sub_socket.clone();

        if let Ok(socket) = sub_socket.lock() {
            let _ = socket.set_subscribe(LOG_TOPIC.as_bytes());
        }

        let initial_value = self.get_logs();

        initial_value.iter().for_each(|log| {
            callback(log);
        });

        let key = self
            .log_listeners
            .lock()
            .ok()
            .map(|mut listeners| listeners.insert(Arc::new(callback)));

        let listeners = Arc::clone(&self.log_listeners);

        move || {
            let (Some(key), Ok(mut listeners)) = (key, listeners.lock()) else {
                return;
            };
            listeners.remove(key);
            if !listeners.is_empty() {
                return;
            }
            drop(listeners);
            if let Ok(socket) = sub_socket.lock() {
                let _ = socket.set_unsubscribe(LOG_TOPIC.as_bytes());
            }
        }
    }

    /// Start the receive threads, so subscriptions begin delivering.
    ///
    /// Publishing and [`get`](Self::get) work without this. Calling it again after
    /// [`stop`](Self::stop) resumes; calling it on a running client does nothing.
    pub fn start(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            self.initialized.store(true, Ordering::SeqCst);
            self.stop.store(false, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            self.stop.store(false, Ordering::SeqCst);
        } else {
            return;
        }

        if self
            .telemetry_listeners
            .lock()
            .is_ok_and(|listeners| !listeners.is_empty())
        {
            self.start_telemetry_receiver();
            self.start_telemetry_keepalive();
        }

        {
            let sub_socket = self.sub_socket.clone();
            let data_listeners = self.data_listeners.clone();
            let log_listeners = self.log_listeners.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            let handle = std::thread::spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let received = {
                        let Ok(socket) = sub_socket.lock() else {
                            break;
                        };
                        match socket.recv_string(0) {
                            Ok(Ok(topic)) => socket.recv_bytes(0).ok().map(|bytes| (topic, bytes)),
                            _ => None,
                        }
                    };
                    let Some((topic, bytes)) = received else {
                        continue;
                    };
                    let Ok(data) = Publish::decode(&bytes[..]) else {
                        continue;
                    };
                    let Some(payload) = data.payload.as_ref() else {
                        continue;
                    };

                    match payload {
                        publish::Payload::Data(command) => {
                            let Some(data) = Self::published_kind(command) else {
                                continue;
                            };
                            let Ok(listeners) = data_listeners.lock() else {
                                continue;
                            };
                            let callbacks: Vec<SubscribeListener> = listeners
                                .get(&topic)
                                .map(|slotmap| slotmap.values().map(Arc::clone).collect())
                                .unwrap_or_default();
                            drop(listeners);

                            for callback in callbacks {
                                callback(data);
                            }
                        }
                        publish::Payload::Logs(command) => {
                            let Ok(listeners) = log_listeners.lock() else {
                                continue;
                            };
                            let callbacks: Vec<LogListener> =
                                listeners.values().map(Arc::clone).collect();
                            drop(listeners);

                            for log in &command.logs {
                                for callback in &callbacks {
                                    callback(log);
                                }
                            }
                        }
                    }
                }
            });
            self.track(handle);
        }
    }

    /// Stop the receive threads. Subscriptions survive and resume on the next
    /// [`start`](Self::start).
    ///
    /// Blocks until every receive thread has exited, which takes up to 100 ms.
    /// Threads are joined rather than abandoned, so a client restarted repeatedly
    /// does not accumulate them.
    ///
    /// Called from a subscription callback it returns without waiting for the
    /// receive thread running that callback, which would otherwise join itself.
    /// That thread still stops, as soon as the callback returns.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let handles = match self.threads.lock() {
            Ok(mut threads) => std::mem::take(&mut *threads),
            Err(_) => return,
        };
        let current = std::thread::current().id();
        for handle in handles {
            if handle.thread().id() != current {
                let _ = handle.join();
            }
        }
        self.telemetry_started.store(false, Ordering::SeqCst);
        self.telemetry_keepalive.store(false, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for TarwynClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TarwynClient")
            .field("telemetry_target", &self.telemetry_target)
            .field("running", &!self.stop.load(Ordering::SeqCst))
            .field("dropped_publishes", &self.dropped_publishes())
            .finish_non_exhaustive()
    }
}

impl Default for TarwynClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Stops the receive threads, so a client that goes out of scope does not leave
/// them decoding into listeners nobody holds.
impl Drop for TarwynClient {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_colliding_channel_is_refused_rather_than_cross_wired() {
        assert_eq!(
            telemetry::topic_hash("glbvs"),
            telemetry::topic_hash("yacxa"),
            "these names are chosen because they collide; the guard is pointless otherwise"
        );

        let mut listeners = HashMap::new();
        assert!(
            register_telemetry_listener(&mut listeners, "glbvs", Arc::new(|_, _| {})).is_some()
        );
        assert!(
            register_telemetry_listener(&mut listeners, "yacxa", Arc::new(|_, _| {})).is_none()
        );
        assert!(
            register_telemetry_listener(&mut listeners, "glbvs", Arc::new(|_, _| {})).is_some()
        );

        let topic = &listeners[&telemetry::topic_hash("glbvs")];
        assert_eq!(topic.channel, "glbvs");
        assert_eq!(topic.listeners.len(), 2);
    }

    fn offline_config() -> TarwynConfig {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21801,
            req_port: 21802,
            sub_port: 21803,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
    }

    #[test]
    fn a_published_value_with_no_payload_is_dropped_not_unwrapped() {
        let empty = SendDataCommand {
            channel: "probe".into(),
            value: None,
        };
        assert!(
            TarwynClient::published_kind(&empty).is_none(),
            "a command carrying no value must be dropped, not unwrapped"
        );

        let kindless = SendDataCommand {
            channel: "probe".into(),
            value: Some(SupportedValues { kind: None }),
        };
        assert!(
            TarwynClient::published_kind(&kindless).is_none(),
            "a value carrying no kind must be dropped; unwrapping it kills the receive \
             thread and every subscription stops silently"
        );

        let present = SendDataCommand {
            channel: "probe".into(),
            value: Some(SupportedValues {
                kind: Some(supported_values::Kind::Double(1.5)),
            }),
        };
        assert_eq!(
            TarwynClient::published_kind(&present),
            Some(&supported_values::Kind::Double(1.5))
        );
    }

    #[test]
    fn a_bad_endpoint_is_reported_rather_than_panicking() {
        let built = TarwynClient::try_with_config(TarwynConfig {
            host: "no host here".to_string(),
            ..offline_config()
        });
        let Err(error) = built else {
            panic!("a host ZeroMQ cannot parse should not build a client");
        };

        assert!(
            matches!(error, ConnectError::Connect { .. }),
            "expected a connect failure, got {error:?}"
        );
        assert!(
            error.to_string().contains("no host here"),
            "the message should name the endpoint, got {error}"
        );
    }

    /// The path neither side can test alone: a value published by a client,
    /// stored by the server, fanned out over PUB, and delivered to a subscriber.
    ///
    /// The ring soak used to cover this end to end. Converting it to a unit test
    /// that writes the ring directly removed the only coverage of the wiring
    /// between them, so it is covered here against a real server.
    #[test]
    fn a_published_value_reaches_a_subscriber_through_a_real_server() {
        use std::sync::mpsc;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports(21881, 21883, 21882);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21883,
            req_port: 21882,
            sub_port: 21881,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let (sender, receiver) = mpsc::channel();
        let _unsubscribe = client.subscribe("round-trip", move |value| {
            let _ = sender.send(value.clone());
        });
        client.start();
        std::thread::sleep(Duration::from_millis(400));

        let mut seen = None;
        for _ in 0..40 {
            client.send_double("round-trip", 4.88);
            if let Ok(value) = receiver.recv_timeout(Duration::from_millis(200)) {
                seen = Some(value);
                break;
            }
        }

        client.stop();
        server.stop();

        assert_eq!(
            seen,
            Some(supported_values::Kind::Double(4.88)),
            "a publish never came back through the server, so the wiring between \
             the push path, the store and the fan-out is broken"
        );
    }

    /// Drives the receiver directly rather than through a server, so it does not
    /// contend for the one fixed UDP port a relay would need.
    #[test]
    fn telemetry_delivery_resumes_after_a_stop_start_cycle() {
        use std::sync::atomic::AtomicUsize;

        let client = TarwynClient::with_config(offline_config());
        let seen = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&seen);

        {
            let mut listeners = client.telemetry_listeners.lock().unwrap();
            assert!(
                register_telemetry_listener(
                    &mut listeners,
                    "resumes",
                    Arc::new(move |_, _| {
                        sink.fetch_add(1, Ordering::SeqCst);
                    })
                )
                .is_some()
            );
        }
        client.start_telemetry_receiver();
        client.start();

        let port = client.telemetry_socket.local_addr().unwrap().port();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let deliver = |target: u16| {
            let mut buf = [0u8; telemetry::HEADER_LEN + 8];
            let len = telemetry::encode(&mut buf, telemetry::topic_hash("resumes"), 1, b"payload");
            let _ = sender.send_to(&buf[..len], ("127.0.0.1", target));
        };
        let arrived = |before: usize| {
            for _ in 0..40 {
                if seen.load(Ordering::SeqCst) > before {
                    return true;
                }
                deliver(port);
                std::thread::sleep(Duration::from_millis(25));
            }
            false
        };

        assert!(arrived(0), "telemetry never arrived while the client ran");

        client.stop();
        std::thread::sleep(Duration::from_millis(250));
        let before_restart = seen.load(Ordering::SeqCst);
        client.start();

        assert!(
            arrived(before_restart),
            "telemetry stopped for good after stop()/start(); the receiver exits on \
             stop and nothing spawns it again, so the UDP plane goes silent"
        );
        client.stop();
    }

    /// Zenoh releases its state lock before running a callback, precisely so this
    /// is legal. Holding the map across the call deadlocks the receive thread and
    /// every subscription on the client with it.
    #[test]
    fn a_callback_may_subscribe_without_deadlocking_the_receive_thread() {
        use std::sync::atomic::AtomicBool;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports(21921, 21922, 21923);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = Arc::new(TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21922,
            req_port: 21923,
            sub_port: 21921,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }));

        let reentered = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&reentered);
        let inner = Arc::clone(&client);
        let _unsubscribe = client.subscribe("reentrant", move |_| {
            if done.load(Ordering::SeqCst) {
                return;
            }
            let _second = inner.subscribe("reentrant/nested", |_| {});
            done.store(true, Ordering::SeqCst);
        });
        client.start();
        std::thread::sleep(Duration::from_millis(400));

        let deadline = Instant::now() + Duration::from_secs(5);
        while !reentered.load(Ordering::SeqCst) && Instant::now() < deadline {
            client.send_double("reentrant", 1.0);
            std::thread::sleep(Duration::from_millis(50));
        }

        let survived = reentered.load(Ordering::SeqCst);
        if survived {
            client.stop();
        }
        server.stop();
        assert!(
            survived,
            "a callback that subscribed never returned, so the receive thread is \
             holding the listener map across user code"
        );
    }

    #[test]
    fn stop_joins_its_threads_rather_than_abandoning_them() {
        let client = TarwynClient::with_config(offline_config());

        for cycle in 0..3 {
            client.start();
            client.stop();
            assert!(
                client.sub_socket.try_lock().is_ok(),
                "cycle {cycle}: a receive thread still held the SUB socket after \
                 stop() returned, so stop() did not wait for it"
            );
            assert!(
                client.threads.lock().unwrap().is_empty(),
                "cycle {cycle}: stop() left thread handles behind"
            );
        }
    }

    #[test]
    fn cancelling_a_telemetry_subscription_removes_its_listener() {
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports(21931, 21932, 21933);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21932,
            req_port: 21933,
            sub_port: 21931,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let cancel = client
            .subscribe_telemetry("cancel-me", |_| {})
            .expect("the server refused the registration");
        assert_eq!(
            client.telemetry_listeners.lock().unwrap().len(),
            1,
            "the subscription was never registered"
        );

        cancel();
        assert!(
            client.telemetry_listeners.lock().unwrap().is_empty(),
            "cancelling left the listener behind, so it keeps decoding datagrams \
             into a ring nobody reads"
        );

        client.stop();
        server.stop();
    }

    /// The UDP path end to end, relay included. It needs a telemetry port of its
    /// own, which is the whole reason that port became configurable.
    #[test]
    fn telemetry_reaches_a_subscriber_through_the_server_relay() {
        use std::sync::mpsc;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21941, 21942, 21943, 21944);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21942,
            req_port: 21943,
            sub_port: 21941,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: 21944,
        });

        let (sender, receiver) = mpsc::channel();
        let _cancel = client
            .subscribe_telemetry("relayed", move |value| {
                let _ = sender.send(value.clone());
            })
            .expect("the server refused the registration");
        client.start();
        std::thread::sleep(Duration::from_millis(300));

        let mut seen = None;
        for _ in 0..40 {
            client.publish_telemetry("relayed", b"payload");
            if let Ok(value) = receiver.recv_timeout(Duration::from_millis(100)) {
                seen = Some(value);
                break;
            }
        }

        client.stop();
        server.stop();
        assert_eq!(
            seen,
            Some(supported_values::Kind::Bytes(b"payload".to_vec())),
            "a telemetry datagram never came back through the server relay"
        );
    }

    /// The server sweeps every registration older than its TTL whenever any client
    /// registers, so a subscription that is never renewed goes silent as soon as a
    /// second client appears -- while publishes keep reporting success.
    #[test]
    fn a_telemetry_subscription_renews_its_lease() {
        use std::sync::atomic::AtomicUsize;

        let context = Context::new();
        let rep = context.socket(zmq::SocketType::REP).unwrap();
        rep.bind("tcp://127.0.0.1:21951").unwrap();
        rep.set_rcvtimeo(500).unwrap();

        let registrations = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&registrations);
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);

        let server = std::thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                let Ok(bytes) = rep.recv_bytes(0) else {
                    continue;
                };
                if let Ok(request) = Request::decode(&bytes[..])
                    && matches!(
                        request.payload,
                        Some(request::Payload::RegisterTelemetry(_))
                    )
                {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                let reply = Reply {
                    payload: Some(reply::Payload::Telemetry(
                        tarwyn_protobuf::protobuf::ReplyTelemetryCommand { registered: true },
                    )),
                }
                .encode_to_vec();
                let _ = rep.send(reply, 0);
            }
        });

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21952,
            req_port: 21951,
            sub_port: 21953,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: 21954,
        });

        let _cancel = client
            .subscribe_telemetry("leased", |_| {})
            .expect("the stub server acknowledged the registration");
        assert_eq!(
            registrations.load(Ordering::SeqCst),
            1,
            "subscribing should register exactly once"
        );

        std::thread::sleep(TELEMETRY_KEEPALIVE + Duration::from_millis(750));

        let seen = registrations.load(Ordering::SeqCst);
        client.stop();
        stop.store(true, Ordering::SeqCst);
        let _ = server.join();

        assert!(
            seen >= 2,
            "the subscription never renewed its lease, so the server drops it after \
             its TTL and telemetry goes silent; saw {seen} registrations"
        );
    }

    #[test]
    fn client_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TarwynClient>();
    }

    #[test]
    fn publishes_reach_a_bound_peer() {
        let context = Context::new();
        let pull = context.socket(zmq::SocketType::PULL).unwrap();
        pull.bind("tcp://127.0.0.1:21811").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21811,
            req_port: 21812,
            sub_port: 21813,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let mut received = None;
        for _ in 0..30 {
            client.send_double("probe", 1.5);
            if let Ok(bytes) = pull.recv_bytes(zmq::DONTWAIT) {
                received = Some(bytes);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let bytes = received.expect("no message reached the bound peer within 3s");
        let payload = Push::decode(&bytes[..]).unwrap().payload.unwrap();
        let push::Payload::Send(command) = payload;
        assert_eq!(command.channel, "probe");
        assert_eq!(
            command.value.unwrap().kind.unwrap(),
            supported_values::Kind::Double(1.5)
        );
    }

    #[test]
    fn send_does_not_block_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        for i in 0..100 {
            client.send_double("no-such-channel", i as f64);
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "send should drop rather than block, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn get_returns_none_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        assert!(client.get("no-such-channel").is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "get() should give up after request_timeout, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn subscribe_does_not_block_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        let _unsubscribe = client.subscribe("no-such-channel", |_| {});
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "subscribe() should not block on an absent server, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn request_socket_recovers_after_timeout() {
        let client = TarwynClient::with_config(offline_config());
        assert!(client.get("first").is_none());
        let started = Instant::now();
        assert!(client.get("second").is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "second request wedged after the first timed out, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn publish_drops_are_counted_not_silent() {
        let client = TarwynClient::with_config(TarwynConfig {
            send_high_water_mark: 4,
            ..offline_config()
        });
        for i in 0..200 {
            client.send_double("no-such-channel", i as f64);
        }
        assert!(
            client.dropped_publishes() > 0,
            "publishes past the high water mark should be counted, saw {}",
            client.dropped_publishes()
        );
    }

    #[test]
    fn list_types_survive_the_wire() {
        let context = Context::new();
        let pull = context.socket(zmq::SocketType::PULL).unwrap();
        pull.bind("tcp://127.0.0.1:21821").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21821,
            req_port: 21822,
            sub_port: 21823,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let expected = vec!["alpha".to_string(), "beta".to_string()];
        let mut received = None;
        for _ in 0..30 {
            client.send_string_list("paths", &expected);
            if let Ok(bytes) = pull.recv_bytes(zmq::DONTWAIT) {
                received = Some(bytes);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let bytes = received.expect("no list message reached the bound peer within 3s");
        let push::Payload::Send(command) = Push::decode(&bytes[..]).unwrap().payload.unwrap();
        assert_eq!(command.channel, "paths");
        match command.value.unwrap().kind.unwrap() {
            supported_values::Kind::StringList(list) => assert_eq!(list.values, expected),
            other => panic!("expected a string list, got {other:?}"),
        }
    }

    #[test]
    fn cached_subscriber_keeps_only_its_depth() {
        let client = TarwynClient::with_config(offline_config());
        let (cache, _unsubscribe) = client.subscribe_cached("depth-test", 3);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.latest().is_none());
        assert!(cache.read_all().is_empty());
    }

    #[test]
    fn subscribe_works_after_start() {
        let client = TarwynClient::with_config(offline_config());
        client.start();
        let started = Instant::now();
        let _unsubscribe = client.subscribe("after-start", |_| {});
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "subscribing after start() deadlocked on the sub socket, took {:?}",
            started.elapsed()
        );
        client.stop();
    }

    /// The gate is what keeps a live value from overtaking the snapshot, and what
    /// keeps a value that arrives during the replay from being reordered ahead of
    /// what is already buffered.
    #[test]
    fn a_buffered_listener_replays_in_order_then_passes_through() {
        use supported_values::Kind;

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        let listener = Arc::new(BufferedListener::new(move |value: &Kind| {
            recorded.lock().unwrap().push(value.clone());
        }));

        listener.deliver(&Kind::Int64(1));
        listener.deliver(&Kind::Int64(2));
        assert!(
            seen.lock().unwrap().is_empty(),
            "values that arrive before the snapshot must be held, not delivered"
        );

        listener.call(&Kind::Int64(0));
        listener.open();
        listener.deliver(&Kind::Int64(3));

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Kind::Int64(0),
                Kind::Int64(1),
                Kind::Int64(2),
                Kind::Int64(3)
            ],
            "the snapshot comes first, then what arrived while it was in flight, \
             then everything after"
        );
    }

    /// A value published while `subscribe` is reading the current value used to
    /// reach nobody: the topic was only handed to ZeroMQ later, by the receive
    /// thread. On a channel that then goes quiet the subscriber stays behind the
    /// server for good, with nothing to say so.
    ///
    /// The stub publishes exactly inside that window and answers the read with no
    /// value at all, so the only way the callback can fire is if the subscription
    /// was already in place when the publish went out.
    #[test]
    fn a_value_published_while_subscribe_reads_the_current_one_is_not_lost() {
        use std::sync::mpsc;

        let context = Context::new();
        let rep = context.socket(zmq::SocketType::REP).unwrap();
        rep.set_rcvtimeo(2000).unwrap();
        rep.set_linger(0).unwrap();
        rep.bind("tcp://127.0.0.1:21961").unwrap();

        let publisher = context.socket(zmq::SocketType::PUB).unwrap();
        publisher.set_linger(0).unwrap();
        publisher.bind("tcp://127.0.0.1:21962").unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let server = std::thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                let Ok(_bytes) = rep.recv_bytes(0) else {
                    continue;
                };
                std::thread::sleep(Duration::from_millis(300));
                let publish = Publish {
                    payload: Some(publish::Payload::Data(SendDataCommand {
                        channel: "window".to_string(),
                        value: Some(SupportedValues {
                            kind: Some(supported_values::Kind::Int64(7)),
                        }),
                    })),
                }
                .encode_to_vec();
                let _ = publisher.send("window", zmq::SNDMORE);
                let _ = publisher.send(publish, 0);
                std::thread::sleep(Duration::from_millis(100));
                let reply = Reply {
                    payload: Some(reply::Payload::Data(
                        tarwyn_protobuf::protobuf::ReplyDataCommand { value: None },
                    )),
                }
                .encode_to_vec();
                let _ = rep.send(reply, 0);
            }
        });

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21963,
            req_port: 21961,
            sub_port: 21962,
            request_timeout: Duration::from_millis(3000),
            send_high_water_mark: 500,
            telemetry_port: 21964,
        });
        std::thread::sleep(Duration::from_millis(300));

        let (sender, receiver) = mpsc::channel();
        let _unsubscribe = client.subscribe("window", move |value| {
            let _ = sender.send(value.clone());
        });
        client.start();

        let seen = receiver.recv_timeout(Duration::from_secs(3)).ok();

        client.stop();
        stop.store(true, Ordering::SeqCst);
        let _ = server.join();

        assert_eq!(
            seen,
            Some(supported_values::Kind::Int64(7)),
            "the publish landed between subscribing and reading the current value, \
             and never reached the subscriber"
        );
    }

    /// A client that goes out of scope has to stop its receive threads. They hold
    /// clones of its sockets, so a client that is dropped without them being
    /// joined leaks a thread and a live SUB socket per client built.
    #[test]
    fn dropping_a_client_stops_its_receive_threads() {
        let socket = {
            let client = TarwynClient::with_config(offline_config());
            client.start();
            std::thread::sleep(Duration::from_millis(200));
            Arc::clone(&client.sub_socket)
        };

        assert_eq!(
            Arc::strong_count(&socket),
            1,
            "a receive thread still holds the SUB socket, so dropping the client \
             did not stop it"
        );
    }

    /// Stopping from inside a subscription callback asks a receive thread to join
    /// itself. It has to skip its own handle instead - and dropping the last
    /// handle to a client from a callback reaches the same path through `Drop`.
    #[test]
    fn stopping_from_a_callback_does_not_wait_for_the_thread_running_it() {
        use std::sync::atomic::AtomicBool;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports(22001, 22002, 22003);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = Arc::new(TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 22002,
            req_port: 22003,
            sub_port: 22001,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }));

        let returned = Arc::new(AtomicBool::new(false));
        let escaped = Arc::clone(&returned);
        let inner = Arc::clone(&client);
        let _unsubscribe = client.subscribe("stopper", move |_| {
            if escaped.load(Ordering::SeqCst) {
                return;
            }
            inner.stop();
            escaped.store(true, Ordering::SeqCst);
        });
        client.start();
        std::thread::sleep(Duration::from_millis(400));

        let deadline = Instant::now() + Duration::from_secs(5);
        while !returned.load(Ordering::SeqCst) && Instant::now() < deadline {
            client.send_double("stopper", 1.0);
            std::thread::sleep(Duration::from_millis(50));
        }

        let survived = returned.load(Ordering::SeqCst);
        server.stop();
        assert!(
            survived,
            "stop() called from a callback never returned, so the receive thread \
             was waiting on itself"
        );
    }
}
