use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
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

enum TopicChange {
    Subscribe(String),
    Unsubscribe(String),
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

type SubscribeListener = Box<dyn Fn(&supported_values::Kind) + Send + 'static>;
type SubscribeListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

type TelemetryListener = Box<dyn Fn(u64, &[u8]) + Send + 'static>;
struct TelemetryTopic {
    channel: String,
    listeners: SlotMap<DefaultKey, TelemetryListener>,
}

type TelemetryListenerMap = Arc<Mutex<HashMap<u32, TelemetryTopic>>>;

fn register_telemetry_listener(
    listeners: &mut HashMap<u32, TelemetryTopic>,
    channel: &str,
    callback: TelemetryListener,
) -> bool {
    let topic = listeners
        .entry(telemetry::topic_hash(channel))
        .or_insert_with(|| TelemetryTopic {
            channel: channel.to_string(),
            listeners: SlotMap::new(),
        });
    if topic.channel != channel {
        return false;
    }
    topic.listeners.insert(callback);
    true
}

type LogListener = Box<dyn Fn(&String) + Send + 'static>;
type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

/// A bounded queue of the values a subscription has seen.
///
/// Handed out by [`TarwynClient::subscribe_cached`] for call sites that poll
/// rather than run a callback. Oldest values are evicted once it is full.
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
    topic_changes: Arc<Mutex<Vec<TopicChange>>>,
    telemetry_socket: Arc<std::net::UdpSocket>,
    telemetry_target: std::net::SocketAddr,
    telemetry_listeners: TelemetryListenerMap,
    telemetry_started: Arc<AtomicBool>,
    req_socket: Mutex<zmq::Socket>,
    request_timeout: Duration,
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
    /// If a socket cannot be created or a telemetry socket cannot be bound.
    pub fn with_config(config: TarwynConfig) -> Self {
        let context = Context::new();

        let listeners: SubscribeListenerMap = Arc::new(Mutex::new(HashMap::new()));
        let log_listeners: LogListenerMap = Arc::new(Mutex::new(SlotMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let push_socket = context.socket(PUSH).unwrap();
        let req_socket = context.socket(REQ).unwrap();
        let sub_socket = context.socket(SUB).unwrap();
        sub_socket.set_rcvtimeo(POLL_INTERVAL_MS).unwrap();

        for socket in [&push_socket, &req_socket, &sub_socket] {
            socket.set_linger(0).unwrap();
        }

        req_socket.set_req_relaxed(true).unwrap();
        req_socket.set_req_correlate(true).unwrap();
        let timeout_ms = config.request_timeout.as_millis().min(i32::MAX as u128) as i32;
        req_socket.set_rcvtimeo(timeout_ms).unwrap();
        req_socket.set_sndtimeo(timeout_ms).unwrap();

        push_socket.set_rcvhwm(config.send_high_water_mark).unwrap();
        push_socket.set_sndhwm(config.send_high_water_mark).unwrap();

        push_socket
            .connect(&format!("tcp://{}:{}", config.host, config.push_port))
            .unwrap();
        req_socket
            .connect(&format!("tcp://{}:{}", config.host, config.req_port))
            .unwrap();
        sub_socket
            .connect(&format!("tcp://{}:{}", config.host, config.sub_port))
            .unwrap();

        TarwynClient {
            data_listeners: listeners,
            push_socket: Mutex::new(push_socket),
            sub_socket: Arc::new(Mutex::new(sub_socket)),
            topic_changes: Arc::new(Mutex::new(Vec::new())),
            telemetry_socket: Arc::new(
                telemetry::bind_ephemeral().expect("could not bind a telemetry socket"),
            ),
            telemetry_target: format!("{}:{}", config.host, config.telemetry_port)
                .parse()
                .unwrap_or_else(|_| {
                    std::net::SocketAddr::from(([127, 0, 0, 1], config.telemetry_port))
                }),
            telemetry_listeners: Arc::new(Mutex::new(HashMap::new())),
            telemetry_started: Arc::new(AtomicBool::new(false)),
            req_socket: Mutex::new(req_socket),
            request_timeout: config.request_timeout,
            dropped: Arc::new(AtomicU64::new(0)),
            stop,
            initialized,
            logger: std::sync::OnceLock::new(),
            log_listeners,
        }
    }

    fn request(&self, message: Vec<u8>) -> Option<reply::Payload> {
        let socket = self.req_socket.lock().ok()?;
        socket.send(message, 0).ok()?;
        let bytes = socket.recv_bytes(0).ok()?;
        Reply::decode(&bytes[..]).ok()?.payload
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
    /// Returns `false` if the server did not acknowledge the registration, or if
    /// another channel already claimed this one's topic hash — a collision is
    /// refused rather than silently cross-wired.
    pub fn subscribe_telemetry<F>(&self, channel: &str, callback: F) -> bool
    where
        F: Fn(&supported_values::Kind) + Send + 'static,
    {
        self.subscribe_telemetry_timestamped(channel, move |_timestamp_us, payload| {
            callback(&supported_values::Kind::Bytes(payload.to_vec()));
        })
    }

    /// As [`subscribe_telemetry`](Self::subscribe_telemetry), but the callback also
    /// receives the publisher's timestamp in microseconds since the Unix epoch.
    pub fn subscribe_telemetry_timestamped<F>(&self, channel: &str, callback: F) -> bool
    where
        F: Fn(u64, &[u8]) + Send + 'static,
    {
        let Ok(local) = self.telemetry_socket.local_addr() else {
            return false;
        };
        let address = format!("{}:{}", self.telemetry_target.ip(), local.port());
        let message = Request {
            payload: Some(request::Payload::RegisterTelemetry(
                RegisterTelemetryCommand {
                    channel: channel.to_string(),
                    address,
                },
            )),
        }
        .encode_to_vec();

        let registered = matches!(
            self.request(message),
            Some(reply::Payload::Telemetry(ack)) if ack.registered
        );
        if !registered {
            return false;
        }

        if let Ok(mut listeners) = self.telemetry_listeners.lock()
            && !register_telemetry_listener(&mut listeners, channel, Box::new(callback))
        {
            return false;
        }
        self.start_telemetry_receiver();
        true
    }

    fn start_telemetry_receiver(&self) {
        if self.telemetry_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let socket = Arc::clone(&self.telemetry_socket);
        let listeners = Arc::clone(&self.telemetry_listeners);
        let stop = Arc::clone(&self.stop);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

        std::thread::spawn(move || {
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
                if let Ok(listeners) = listeners.lock()
                    && let Some(topic) = listeners.get(&channel_hash)
                {
                    for (_, callback) in topic.listeners.iter() {
                        callback(timestamp_us, payload);
                    }
                }
            }
        });
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
    pub fn subscribe<F>(&self, channel: &str, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&supported_values::Kind) + Send + 'static,
    {
        self.queue_topic_change(TopicChange::Subscribe(channel.to_string()));

        if let Some(initial_value) = self.get(channel) {
            callback(&initial_value);
        }

        let mut listeners = self.data_listeners.lock().unwrap();
        let callback = Box::new(callback);
        let key = listeners
            .entry(channel.to_string())
            .or_default()
            .insert(Box::new(callback));

        let listeners = Arc::clone(&self.data_listeners);
        let topic_changes = Arc::clone(&self.topic_changes);
        let channel = channel.to_string();

        move || {
            let mut listeners = listeners.lock().unwrap();
            if let Some(slotmap) = listeners.get_mut(&channel) {
                slotmap.remove(key);
                if slotmap.is_empty() {
                    listeners.remove(&channel);
                    if let Ok(mut pending) = topic_changes.lock() {
                        pending.push(TopicChange::Unsubscribe(channel.clone()));
                    }
                }
            }
        }
    }

    fn queue_topic_change(&self, change: TopicChange) {
        if let Ok(mut pending) = self.topic_changes.lock() {
            pending.push(change);
        }
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
        F: Fn(&String) + Send + 'static,
    {
        let sub_socket = self.sub_socket.clone();

        sub_socket
            .lock()
            .unwrap()
            .set_subscribe("TARWYN_INTERNAL_LOG".as_bytes())
            .unwrap();

        let initial_value = self.get_logs();

        initial_value.iter().for_each(|log| {
            callback(log);
        });

        let mut listeners = self.log_listeners.lock().unwrap();
        let callback = Box::new(callback);

        let key = listeners.insert(Box::new(callback));

        let listeners = Arc::clone(&self.log_listeners);

        move || {
            listeners.lock().unwrap().remove(key);
            if listeners.lock().unwrap().is_empty() {
                sub_socket
                    .lock()
                    .unwrap()
                    .set_unsubscribe("TARWYN_INTERNAL_LOG".as_bytes())
                    .unwrap();
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
        } else if self.stop.load(Ordering::SeqCst) {
            self.stop.store(false, Ordering::SeqCst);
        } else {
            return;
        }
        {
            let sub_socket = self.sub_socket.clone();
            let topic_changes = self.topic_changes.clone();
            let data_listeners = self.data_listeners.clone();
            let log_listeners = self.log_listeners.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            std::thread::spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let received = {
                        let Ok(socket) = sub_socket.lock() else {
                            break;
                        };
                        if let Ok(mut pending) = topic_changes.lock() {
                            for change in pending.drain(..) {
                                let _ = match change {
                                    TopicChange::Subscribe(topic) => {
                                        socket.set_subscribe(topic.as_bytes())
                                    }
                                    TopicChange::Unsubscribe(topic) => {
                                        socket.set_unsubscribe(topic.as_bytes())
                                    }
                                };
                            }
                        }
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
                            let mut listeners = data_listeners.lock().unwrap();
                            let data = command.value.clone().unwrap().kind.unwrap();

                            listeners
                                .entry(topic)
                                .or_default()
                                .iter()
                                .for_each(|(_, callback)| {
                                    callback(&data);
                                });
                        }
                        publish::Payload::Logs(command) => {
                            let listeners = log_listeners.lock().unwrap();

                            command.logs.iter().for_each(|log| {
                                listeners.iter().for_each(|(_, callback)| {
                                    callback(log);
                                });
                            });
                        }
                    }
                }
            });
        }
    }

    /// Stop the receive threads. Subscriptions survive and resume on the next
    /// [`start`](Self::start).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Default for TarwynClient {
    fn default() -> Self {
        Self::new()
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
        assert!(register_telemetry_listener(
            &mut listeners,
            "glbvs",
            Box::new(|_, _| {})
        ));
        assert!(!register_telemetry_listener(
            &mut listeners,
            "yacxa",
            Box::new(|_, _| {})
        ));
        assert!(register_telemetry_listener(
            &mut listeners,
            "glbvs",
            Box::new(|_, _| {})
        ));

        let topic = &listeners[&telemetry::topic_hash("glbvs")];
        assert_eq!(topic.channel, "glbvs");
        assert_eq!(topic.listeners.len(), 2);
    }

    fn offline_config() -> TarwynConfig {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47901,
            req_port: 47902,
            sub_port: 47903,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
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
        pull.bind("tcp://127.0.0.1:47911").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47911,
            req_port: 47912,
            sub_port: 47913,
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
        pull.bind("tcp://127.0.0.1:47921").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47921,
            req_port: 47922,
            sub_port: 47923,
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
}
