use std::{
    collections::{HashMap, VecDeque},
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{Receiver, Sender, SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};

use prost::Message;
use serde_json::Map;
use slotmap::{DefaultKey, SlotMap};
use tungstenite::{
    Message as WsMessage, WebSocket, http::Request as HttpRequest, stream::MaybeTlsStream,
};

use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BezierCurvesList, BoolList, BytesList, CompareAndSetCommand,
    Coordinate, CoordinateList, DeleteCommand, DoubleList, FloatList, GetDataCommand,
    GetLogsCommand, IntegerList, JsonCommand, ListTablesCommand, LongList, PingCommand, Reply,
    ReplyStatisticsCommand, Request, StatisticsCommand, StringList, SupportedValues, reply,
    request, supported_values,
};
use tarwyn_protobuf::telemetry;

use tarwyn_server::value::XtValue;
use tarwyn_server::websocket::message::{CtMessage, ValueMessage};
use tarwyn_server::websocket::protocol::{encode_once, type_string};

use crate::ports;

const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The WS topic the server relays log lines on.
const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";
/// The NT4 subprotocol this client speaks. Mirrors the server's `frame.rs`.
const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The WS endpoint the server accepts NT4 connections on.
const TABLE_PATH: &str = "/nt/test";

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
/// How long the reader loop blocks on the socket before it looks at the stop
/// flag and drains the outbound queue again.
const POLL_INTERVAL: Duration = Duration::from_millis(POLL_INTERVAL_MS as u64);

/// A subscription callback that holds values back until its snapshot has been
/// delivered.
///
/// `subscribe` has to tell the server about the topic before it reads the
/// current value, or a value published in between reaches nobody and the
/// subscriber is left behind the server for as long as the channel stays quiet.
/// Subscribing first opens the opposite race - a live value arriving before the
/// snapshot - so values that arrive early are buffered here and replayed once
/// the snapshot is through.
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
/// Every variant is a failure to set up the connection before any traffic is
/// attempted. Once a client exists, a server that is absent or unreachable is
/// not an error - publishes drop and reads return `None`.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// The host could not be resolved to an address for the WebSocket.
    #[error("could not connect the {socket} socket to {endpoint}")]
    Connect {
        /// Which socket failed.
        socket: &'static str,
        /// The endpoint it was given.
        endpoint: String,
        /// The underlying resolver error.
        source: std::io::Error,
    },
    /// No UDP socket could be bound for the telemetry plane.
    #[error("could not bind a telemetry socket")]
    Telemetry(#[from] std::io::Error),
    /// The host could not be resolved to an address for the telemetry plane.
    #[error("could not resolve {host} for the telemetry plane")]
    Resolve {
        /// The host that was given.
        host: String,
        /// The underlying resolver error.
        source: std::io::Error,
    },
}

#[derive(Clone, Debug)]
/// Where the client dials and how patient it is.
///
/// [`Default`] points at `127.0.0.1` on the standard ports with a 500 ms
/// request timeout.
pub struct TarwynConfig {
    /// Host running the server. An address, not a URL.
    pub host: String,
    /// PUSH/PULL port. Retained for compatibility; the WebSocket carries
    /// publishes, so this is unused.
    pub push_port: u16,
    /// REQ/REP port, used by [`get`](TarwynClient::get), the control plane and
    /// every publish and subscription - the WebSocket binds here.
    pub req_port: u16,
    /// PUB/SUB port. Retained for compatibility; the WebSocket carries
    /// subscriptions, so this is unused.
    pub sub_port: u16,
    /// How long a request waits for its reply before giving up and returning `None`.
    pub request_timeout: Duration,
    /// High-water mark on the outbound queue. Publishes past it are dropped,
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

/// Resolve where telemetry datagrams are sent.
///
/// The WebSocket resolves names itself, so the control plane accepts a hostname
/// and this has to as well. Parsing the host as an address and quietly falling
/// back to loopback is what makes a client whose reads and publishes all work
/// send its telemetry nowhere.
fn resolve_telemetry_target(host: &str, port: u16) -> Result<std::net::SocketAddr, ConnectError> {
    use std::net::ToSocketAddrs;

    (host, port)
        .to_socket_addrs()
        .map_err(|source| ConnectError::Resolve {
            host: host.to_string(),
            source,
        })?
        .next()
        .ok_or_else(|| ConnectError::Resolve {
            host: host.to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the name resolved to no addresses",
            ),
        })
}

/// Maps an [`XtValue`] to its NT4 data-type byte.
///
/// This duplicates the server's identical table in
/// `tarwyn_server::websocket::protocol::xt_data_type` because that
/// function is crate-private. The tables are byte-for-byte identical;
/// if the server's copy is ever made `pub`, this one should be removed.
fn xt_data_type(v: &XtValue) -> u32 {
    match v {
        XtValue::Bool(_) => 0,
        XtValue::Double(_) => 1,
        XtValue::Float(_) => 3,
        XtValue::Int8(_) | XtValue::Uint8(_) => 2,
        XtValue::BoolArray(_) => 16,
        XtValue::DoubleArray(_) => 17,
        XtValue::FloatArray(_) => 19,
        XtValue::StringArray(_) => 20,
        XtValue::String(_) => 4,
        XtValue::Bytes(_) | XtValue::BytesList(_) | XtValue::Coordinate(_) | XtValue::Bezier(_) => {
            5
        }
        XtValue::Int16(_)
        | XtValue::Uint16(_)
        | XtValue::Int32(_)
        | XtValue::Uint32(_)
        | XtValue::Int64(_)
        | XtValue::Uint64(_) => 2,
        XtValue::Int8Array(_)
        | XtValue::Uint8Array(_)
        | XtValue::Int16Array(_)
        | XtValue::Uint16Array(_)
        | XtValue::Int32Array(_)
        | XtValue::Uint32Array(_)
        | XtValue::Int64Array(_)
        | XtValue::Uint64Array(_) => 18,
    }
}

fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Establish the NT4 WebSocket connection, requesting the NT4 subprotocol.
fn connect_ws(
    url: &str,
    subprotocol: &str,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, tungstenite::Error> {
    let host = url
        .strip_prefix("ws://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    let request = HttpRequest::builder()
        .method("GET")
        .uri(url)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Protocol", subprotocol)
        .body(())?;
    let (ws, _response) = tungstenite::connect(request)?;
    Ok(ws)
}

/// Give the reader loop a bounded read so it can drain outbound and check stop.
fn set_read_timeout(ws: &WebSocket<MaybeTlsStream<TcpStream>>, timeout: Duration) {
    if let MaybeTlsStream::Plain(stream) = ws.get_ref() {
        let _ = stream.set_read_timeout(Some(timeout));
    }
}

fn is_timeout(e: &tungstenite::Error) -> bool {
    matches!(
        e,
        tungstenite::Error::Io(io)
            if io.kind() == std::io::ErrorKind::WouldBlock
                || io.kind() == std::io::ErrorKind::TimedOut
    )
}

/// Send every queued outbound frame. Returns `false` if the connection died.
fn drain_outbound(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    outbound: &Receiver<Vec<u8>>,
) -> bool {
    while let Ok(frame) = outbound.try_recv() {
        if ws.send(WsMessage::binary(frame)).is_err() {
            return false;
        }
    }
    true
}

/// Drop every queued outbound frame while disconnected, counting them.
fn drain_outbound_dropped(outbound: &Receiver<Vec<u8>>, dropped: &AtomicU64) {
    while outbound.try_recv().is_ok() {
        dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// Route a decoded value message to the right listeners by topic name.
fn fan_out_value(
    vm: ValueMessage,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_ids: &Arc<Mutex<HashMap<String, u32>>>,
) {
    let name = {
        let ids = topic_ids.lock().unwrap_or_else(|p| p.into_inner());
        ids.iter()
            .find(|&(_, &id)| id == vm.topic_id)
            .map(|(name, _)| name.clone())
    };
    let Some(name) = name else {
        return;
    };

    if name == LOG_TOPIC {
        if let XtValue::StringArray(lines) = vm.value {
            let callbacks: Vec<LogListener> = log_listeners
                .lock()
                .ok()
                .map(|l| l.values().map(Arc::clone).collect())
                .unwrap_or_default();
            for line in &lines {
                for callback in &callbacks {
                    callback(line);
                }
            }
        }
        return;
    }

    let kind = supported_values::Kind::from(vm.value);
    let callbacks: Vec<SubscribeListener> = data_listeners
        .lock()
        .ok()
        .map(|l| {
            l.get(&name)
                .map(|slotmap| slotmap.values().map(Arc::clone).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    for callback in callbacks {
        callback(&kind);
    }
}

/// Handle one binary frame: a value message, a control reply, or noise.
fn handle_binary(
    payload: Vec<u8>,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_ids: &Arc<Mutex<HashMap<String, u32>>>,
    pending: &Arc<Mutex<Option<Sender<Vec<u8>>>>>,
) {
    if let Ok(vm) = ValueMessage::decode(&payload) {
        fan_out_value(vm, data_listeners, log_listeners, topic_ids);
        return;
    }
    if Reply::decode(&payload[..]).is_ok()
        && let Some(tx) = pending.lock().ok().and_then(|mut p| p.take())
    {
        let _ = tx.send(payload);
    }
}

/// Handle one text frame: an NT4 announcement, which corrects the topic map.
fn handle_text(text: String, topic_ids: &Arc<Mutex<HashMap<String, u32>>>) {
    if let Ok(CtMessage::Announce { name, id, .. }) = CtMessage::from_json(&text) {
        topic_ids
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(name, id);
    }
}

/// The single connection owner: connects (retrying), drains outbound, and
/// demuxes inbound frames until told to stop.
#[allow(clippy::too_many_arguments)]
fn reader_loop(
    outbound: Receiver<Vec<u8>>,
    url: String,
    subprotocol: String,
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    topic_ids: Arc<Mutex<HashMap<String, u32>>>,
    pending: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    reader_alive: Arc<AtomicBool>,
) {
    reader_alive.store(true, Ordering::SeqCst);

    'outer: loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let mut ws = match connect_ws(&url, &subprotocol) {
            Ok(ws) => ws,
            Err(_) => {
                drain_outbound_dropped(&outbound, &dropped);
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
        };
        set_read_timeout(&ws, POLL_INTERVAL);

        loop {
            if stop.load(Ordering::SeqCst) {
                break 'outer;
            }
            if !drain_outbound(&mut ws, &outbound) {
                break;
            }
            match ws.read() {
                Ok(WsMessage::Binary(payload)) => {
                    handle_binary(
                        payload.to_vec(),
                        &data_listeners,
                        &log_listeners,
                        &topic_ids,
                        &pending,
                    );
                }
                Ok(WsMessage::Text(text)) => handle_text(text.to_string(), &topic_ids),
                Ok(WsMessage::Ping(payload)) => {
                    let _ = ws.send(WsMessage::Pong(payload));
                }
                Ok(WsMessage::Pong(_)) => {}
                Ok(WsMessage::Close(_)) => break,
                Ok(WsMessage::Frame(_)) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => break,
            }
        }
    }

    reader_alive.store(false, Ordering::SeqCst);
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
/// never blocks — the WebSocket dials in the background, so a client may be
/// built before the server exists. Nothing is received until [`start`](Self::start)
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
    outbound: Mutex<SyncSender<Vec<u8>>>,
    pending: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
    topic_ids: Arc<Mutex<HashMap<String, u32>>>,
    next_topic_id: Arc<AtomicU32>,
    next_pubuid: Arc<AtomicU32>,
    next_subuid: Arc<AtomicU32>,
    request_lock: Mutex<()>,
    request_timeout: Duration,
    send_high_water_mark: usize,
    url: String,
    subprotocol: String,
    telemetry_socket: Arc<std::net::UdpSocket>,
    telemetry_target: std::net::SocketAddr,
    telemetry_listeners: TelemetryListenerMap,
    telemetry_started: Arc<AtomicBool>,
    telemetry_keepalive: Arc<AtomicBool>,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    dropped: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    reader_started: Arc<AtomicBool>,
    reader_alive: Arc<AtomicBool>,
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
    /// If the host cannot be resolved or a socket cannot be bound. Use
    /// [`try_with_config`](Self::try_with_config) to handle that instead.
    pub fn with_config(config: TarwynConfig) -> Self {
        Self::try_with_config(config).expect("could not construct an Tarwyn client")
    }

    /// As [`with_config`](Self::with_config), reporting setup failure instead of
    /// panicking.
    pub fn try_with_config(config: TarwynConfig) -> Result<Self, ConnectError> {
        use std::net::ToSocketAddrs;

        let endpoint = format!("ws://{}:{}{}", config.host, config.req_port, TABLE_PATH);
        (config.host.as_str(), config.req_port)
            .to_socket_addrs()
            .map_err(|source| ConnectError::Connect {
                socket: "WS",
                endpoint: endpoint.clone(),
                source,
            })?;

        let (tx, _rx) = sync_channel(config.send_high_water_mark.max(1) as usize);

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        Ok(TarwynClient {
            data_listeners: Arc::new(Mutex::new(HashMap::new())),
            log_listeners: Arc::new(Mutex::new(SlotMap::new())),
            outbound: Mutex::new(tx),
            pending: Arc::new(Mutex::new(None)),
            topic_ids: Arc::new(Mutex::new(HashMap::new())),
            next_topic_id: Arc::new(AtomicU32::new(0)),
            next_pubuid: Arc::new(AtomicU32::new(0)),
            next_subuid: Arc::new(AtomicU32::new(0)),
            request_lock: Mutex::new(()),
            request_timeout: config.request_timeout,
            send_high_water_mark: config.send_high_water_mark.max(1) as usize,
            url: endpoint,
            subprotocol: NT4_SUBPROTOCOL.to_string(),
            telemetry_socket: Arc::new(telemetry::bind_ephemeral()?),
            telemetry_target: resolve_telemetry_target(&config.host, config.telemetry_port)?,
            telemetry_listeners: Arc::new(Mutex::new(HashMap::new())),
            telemetry_started: Arc::new(AtomicBool::new(false)),
            telemetry_keepalive: Arc::new(AtomicBool::new(false)),
            threads: Mutex::new(Vec::new()),
            dropped: Arc::new(AtomicU64::new(0)),
            stop,
            initialized,
            reader_started: Arc::new(AtomicBool::new(false)),
            reader_alive: Arc::new(AtomicBool::new(false)),
            logger: std::sync::OnceLock::new(),
        })
    }

    /// Spawn the reader thread if it is not already running.
    ///
    /// Called lazily by every operation that touches the wire, so a client works
    /// without an explicit [`start`](Self::start). The reader owns the WebSocket,
    /// drains the outbound queue, and demuxes inbound frames.
    fn ensure_reader(&self) {
        if self.stop.load(Ordering::SeqCst) || self.reader_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = sync_channel(self.send_high_water_mark);
        *self.outbound.lock().unwrap_or_else(|p| p.into_inner()) = tx;

        let url = self.url.clone();
        let subprotocol = self.subprotocol.clone();
        let data_listeners = Arc::clone(&self.data_listeners);
        let log_listeners = Arc::clone(&self.log_listeners);
        let topic_ids = Arc::clone(&self.topic_ids);
        let pending = Arc::clone(&self.pending);
        let stop = Arc::clone(&self.stop);
        let dropped = Arc::clone(&self.dropped);
        let reader_alive = Arc::clone(&self.reader_alive);

        let handle = std::thread::spawn(move || {
            reader_loop(
                rx,
                url,
                subprotocol,
                data_listeners,
                log_listeners,
                topic_ids,
                pending,
                stop,
                dropped,
                reader_alive,
            );
        });
        self.track(handle);
    }

    fn request(&self, message: Vec<u8>) -> Option<reply::Payload> {
        let _guard = self.request_lock.lock().ok()?;
        self.ensure_reader();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut pending = self.pending.lock().ok()?;
            *pending = Some(tx);
        }
        if self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(message)
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut p) = self.pending.lock() {
                *p = None;
            }
            return None;
        }
        match rx.recv_timeout(self.request_timeout) {
            Ok(bytes) => Reply::decode(&bytes[..]).ok()?.payload,
            Err(_) => {
                if let Ok(mut p) = self.pending.lock() {
                    *p = None;
                }
                None
            }
        }
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
        self.ensure_reader();
        let value = XtValue::from(kind);
        let topic_id = self.ensure_topic(channel, &value);
        let frame = encode_once(&value, now_micros(), topic_id).to_vec();
        if self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(frame)
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Make sure a channel has a topic id, publishing it on first use.
    ///
    /// The server allocates topic ids deterministically from 0, so the client
    /// predicts the next id and records it; the server's announcement corrects
    /// it if the prediction is ever wrong.
    fn ensure_topic(&self, channel: &str, value: &XtValue) -> u32 {
        let mut topic_ids = self.topic_ids.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(&id) = topic_ids.get(channel) {
            return id;
        }
        let id = self.next_topic_id.fetch_add(1, Ordering::Relaxed);
        topic_ids.insert(channel.to_string(), id);
        let data_type = type_string(xt_data_type(value))
            .unwrap_or("bin")
            .to_string();
        let pubuid = self.next_pubuid.fetch_add(1, Ordering::Relaxed);
        let publish = CtMessage::Publish {
            name: channel.to_string(),
            pubuid,
            data_type,
            properties: Map::new(),
        };
        let _ = self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(publish.to_json().into_bytes());
        id
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
    /// Roughly 3.6x faster than the WebSocket path. Subscribers must register with
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
    /// another channel already claimed this one's topic hash — a collision is
    /// refused rather than silently cross-wired.
    ///
    /// Registration is a datagram on the telemetry plane, not a request, so this
    /// does not wait on the server and `Some` does not mean the server heard.
    /// It is resent on a keepalive until it does.
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
        let hash = telemetry::topic_hash(channel);
        let mut listeners = self.telemetry_listeners.lock().ok()?;
        let key = register_telemetry_listener(&mut listeners, channel, Arc::new(callback))?;
        drop(listeners);
        self.register_telemetry(hash);
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

    /// Ask the server to relay a channel to this client's telemetry socket.
    ///
    /// Sent from that socket, so the address the server routes to is the one the
    /// datagram arrived from - correct through NAT, and impossible to point at a
    /// machine that did not ask for it. UDP, so there is nothing to acknowledge;
    /// the keepalive resends until it lands.
    fn register_telemetry(&self, channel_hash: u32) {
        let mut buf = [0u8; telemetry::HEADER_LEN];
        let len = telemetry::encode_registration(&mut buf, channel_hash);
        let _ = self
            .telemetry_socket
            .send_to(&buf[..len], self.telemetry_target);
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

                let hashes: Vec<u32> = match listeners.lock() {
                    Ok(listeners) => listeners.keys().copied().collect(),
                    Err(_) => continue,
                };
                for hash in hashes {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut buf = [0u8; telemetry::HEADER_LEN];
                    let len = telemetry::encode_registration(&mut buf, hash);
                    let _ = telemetry_socket.send_to(&buf[..len], target);
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
    /// [`request_timeout`](TarwynConfig::request_timeout). Requests are serialized,
    /// so a reply to an abandoned request is never handed to the next caller.
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

        self.ensure_reader();
        let subuid = self.next_subuid.fetch_add(1, Ordering::Relaxed);
        let subscribe = CtMessage::Subscribe {
            topics: vec![channel.to_string()],
            subuid,
            options: Map::new(),
        };
        let _ = self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(subscribe.to_json().into_bytes());

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
        F: Fn(&String) + Send + Sync + 'static,
    {
        self.ensure_reader();
        let subuid = self.next_subuid.fetch_add(1, Ordering::Relaxed);
        let subscribe = CtMessage::Subscribe {
            topics: vec![LOG_TOPIC.to_string()],
            subuid,
            options: Map::new(),
        };
        let _ = self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(subscribe.to_json().into_bytes());

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

        self.ensure_reader();

        if self
            .telemetry_listeners
            .lock()
            .is_ok_and(|listeners| !listeners.is_empty())
        {
            self.start_telemetry_receiver();
            self.start_telemetry_keepalive();
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
        self.reader_started.store(false, Ordering::SeqCst);
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
    fn a_bad_endpoint_is_reported_rather_than_panicking() {
        let built = TarwynClient::try_with_config(TarwynConfig {
            host: "no host here".to_string(),
            ..offline_config()
        });
        let Err(error) = built else {
            panic!("a host that cannot be resolved should not build a client");
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
    /// stored by the server, fanned out over the WebSocket, and delivered to a
    /// subscriber.
    ///
    /// The server does not add a publisher as a subscriber for a new topic, so
    /// the topic is created by a first publish, subscribed, then published again;
    /// the retained value from the subscribe is skipped in the receive loop.
    #[test]
    fn a_published_value_reaches_a_subscriber_through_a_real_server() {
        use std::sync::mpsc;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21881, 21883, 21882, 21884);
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

        // Create the topic first, so the publisher is not the only subscriber.
        client.send_double("round-trip", 1.0);
        std::thread::sleep(Duration::from_millis(200));

        let (sender, receiver) = mpsc::channel();
        let _unsubscribe = client.subscribe("round-trip", move |value| {
            let _ = sender.send(value.clone());
        });
        client.start();
        std::thread::sleep(Duration::from_millis(200));

        let mut seen = None;
        for _ in 0..40 {
            client.send_double("round-trip", 4.88);
            if let Ok(value) = receiver.recv_timeout(Duration::from_millis(200))
                && value == supported_values::Kind::Double(4.88)
            {
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

    /// The reader releases its listener map before running a callback, precisely
    /// so this is legal. Holding the map across the call deadlocks the reader and
    /// every subscription on the client with it.
    #[test]
    fn a_callback_may_subscribe_without_deadlocking_the_receive_thread() {
        use std::sync::atomic::AtomicBool;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21921, 21922, 21923, 21924);
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

        // Create the topic first, so the publisher is not the only subscriber.
        client.send_double("reentrant", 1.0);
        std::thread::sleep(Duration::from_millis(200));

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
        std::thread::sleep(Duration::from_millis(200));

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
                client.threads.lock().unwrap().is_empty(),
                "cycle {cycle}: stop() left thread handles behind"
            );
        }
    }

    #[test]
    fn cancelling_a_telemetry_subscription_removes_its_listener() {
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21931, 21932, 21933, 21934);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21932,
            req_port: 21933,
            sub_port: 21931,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: 21934,
        });

        let cancel = client
            .subscribe_telemetry("cancel-me", |_| {})
            .expect("the topic hash was free");
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
            .expect("the topic hash was free");
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
    ///
    /// The stub is a bare UDP socket, because registration is a datagram on the
    /// telemetry plane rather than a request on the control plane.
    #[test]
    fn a_telemetry_subscription_renews_its_lease() {
        use std::sync::atomic::AtomicUsize;

        let relay = std::net::UdpSocket::bind(("127.0.0.1", 21954)).unwrap();
        relay
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let registrations = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&registrations);
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);

        let server = std::thread::spawn(move || {
            let mut buf = [0u8; telemetry::MAX_DATAGRAM];
            while !server_stop.load(Ordering::SeqCst) {
                let Ok((len, _from)) = relay.recv_from(&mut buf) else {
                    continue;
                };
                if telemetry::decode_registration(&buf[..len]).is_some() {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
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
            .expect("the topic hash was free");

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

    /// A publish reaches the server over the WebSocket and is stored, so a read
    /// round-trips it back.
    #[test]
    fn publishes_reach_a_bound_peer() {
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21811, 21813, 21812, 21814);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21813,
            req_port: 21812,
            sub_port: 21811,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let mut received = None;
        for _ in 0..30 {
            client.send_double("probe", 1.5);
            if let Some(value) = client.get("probe") {
                received = Some(value);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        client.stop();
        server.stop();

        assert_eq!(
            received,
            Some(supported_values::Kind::Double(1.5)),
            "no publish reached the server within 3s"
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

    /// A list value survives the round trip to the server and back.
    #[test]
    fn list_types_survive_the_wire() {
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(21821, 21823, 21822, 21824);
        server.start();
        std::thread::sleep(Duration::from_millis(400));

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 21823,
            req_port: 21822,
            sub_port: 21821,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let expected = vec!["alpha".to_string(), "beta".to_string()];
        let mut received = None;
        for _ in 0..30 {
            client.send_string_list("paths", &expected);
            if let Some(value) = client.get("paths") {
                received = Some(value);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        client.stop();
        server.stop();

        match received {
            Some(supported_values::Kind::StringList(list)) => assert_eq!(list.values, expected),
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
            "subscribing after start() deadlocked, took {:?}",
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
    /// reach nobody: the topic was only handed to the server later, by the receive
    /// thread. On a channel that then goes quiet the subscriber stays behind the
    /// server for good, with nothing to say so.
    ///
    /// The stub answers the subscribe with an announcement, then answers the read
    /// by publishing a value before replying with no value at all, so the only way
    /// the callback can fire is if the subscription was already in place when the
    /// publish went out.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite's Callback trait mandates HttpResponse as the error type"
    )]
    fn a_value_published_while_subscribe_reads_the_current_one_is_not_lost() {
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:21961").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept_hdr(
                stream,
                |_req: &tungstenite::http::Request<()>,
                 mut resp: tungstenite::http::Response<()>| {
                    resp.headers_mut().insert(
                        "Sec-WebSocket-Protocol",
                        tungstenite::http::HeaderValue::from_static(NT4_SUBPROTOCOL),
                    );
                    Ok(resp)
                },
            )
            .unwrap();
            while !server_stop.load(Ordering::SeqCst) {
                let Ok(WsMessage::Binary(payload)) = ws.read() else {
                    continue;
                };
                if let Ok(request) = Request::decode(&payload[..]) {
                    let _ = request;
                    // This is the get. Publish a value during the read, then reply
                    // with no value at all.
                    std::thread::sleep(Duration::from_millis(300));
                    let vm = ValueMessage {
                        topic_id: 0,
                        timestamp_micros: 0,
                        data_type: 2,
                        value: XtValue::Int64(7),
                    };
                    let mut buf = Vec::new();
                    vm.encode(&mut buf);
                    let _ = ws.send(WsMessage::binary(buf));
                    std::thread::sleep(Duration::from_millis(100));
                    let reply = Reply {
                        payload: Some(reply::Payload::Data(
                            tarwyn_protobuf::protobuf::ReplyDataCommand { value: None },
                        )),
                    }
                    .encode_to_vec();
                    let _ = ws.send(WsMessage::binary(reply));
                } else if let Ok(CtMessage::Subscribe { .. }) =
                    CtMessage::from_json(&String::from_utf8_lossy(&payload))
                {
                    // Announce the topic so the client maps id 0 to "window".
                    let announce = CtMessage::Announce {
                        name: "window".to_string(),
                        id: 0,
                        data_type: "int".to_string(),
                        properties: Map::new(),
                        pubuid: None,
                    };
                    let _ = ws.send(WsMessage::text(announce.to_json()));
                }
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
            Some(supported_values::Kind::Uint32(7)),
            "the publish landed between subscribing and reading the current value, \
             and never reached the subscriber"
        );
    }

    /// A client that goes out of scope has to stop its receive threads. They hold
    /// clones of its sockets, so a client that is dropped without them being
    /// joined leaks a thread and a live connection per client built.
    #[test]
    fn dropping_a_client_stops_its_receive_threads() {
        let alive = {
            let client = TarwynClient::with_config(offline_config());
            client.start();
            std::thread::sleep(Duration::from_millis(200));
            Arc::clone(&client.reader_alive)
        };

        assert!(
            !alive.load(Ordering::SeqCst),
            "the reader thread is still running after the client was dropped"
        );
    }

    /// Stopping from inside a subscription callback asks a receive thread to join
    /// itself. It has to skip its own handle instead - and dropping the last
    /// handle to a client from a callback reaches the same path through `Drop`.
    #[test]
    fn stopping_from_a_callback_does_not_wait_for_the_thread_running_it() {
        use std::sync::atomic::AtomicBool;
        use tarwyn_server::tarwyn_server::TarwynServer;

        let server = TarwynServer::with_ports_and_telemetry(22001, 22002, 22003, 22004);
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

        // Create the topic first, so the publisher is not the only subscriber.
        client.send_double("stopper", 1.0);
        std::thread::sleep(Duration::from_millis(200));

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
        std::thread::sleep(Duration::from_millis(200));

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
