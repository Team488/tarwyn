//! The client itself: connecting, publishing, reading, subscribing.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{Sender, SyncSender, sync_channel},
    },
    time::Duration,
};

use prost::Message;
use serde_json::Map;
use slotmap::SlotMap;

use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BezierCurvesList, BoolList, BytesList, CompareAndSetCommand,
    CoordinateList, DeleteCommand, DoubleList, FloatList, GetDataCommand, GetLogsCommand,
    IntegerList, JsonCommand, ListTablesCommand, LongList, PingCommand, Reply,
    ReplyStatisticsCommand, Request, StatisticsCommand, StringList, SupportedValues, reply,
    request, supported_values,
};
use tarwyn_protobuf::telemetry;

use tarwyn_server::value::Value;
use tarwyn_server::websocket::message::ControlMessage;
use tarwyn_server::websocket::protocol::{
    VALUE_FRAME_HINT, encode_into, type_string, xt_data_type,
};

use crate::config::{Config, ConnectError};
use crate::connection::{SharedWriter, write_frame};
use crate::listeners::{
    BufferedListener, LogListenerMap, SessionState, SubscribeListenerMap, TopicNames,
};
use crate::reader::reader_loop;
use crate::subscriber::CachedSubscriber;
use crate::telemetry::{TelemetryListenerMap, resolve_telemetry_target};

pub(crate) const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";
/// The WebSocket topic the server relays log lines on.
pub(crate) const LOG_TOPIC: &str = "TARWYN_INTERNAL_LOG";
/// The NT4 subprotocol this client speaks, the same one the server's
/// `frame.rs` accepts.
pub(crate) const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The WebSocket endpoint the server accepts NT4 connections on.
pub(crate) const TABLE_PATH: &str = "/nt/test";
/// The timestamp every value is published with: NT4's "stamp it on arrival".
/// This client's clock is not synced to the server's.
pub(crate) const SERVER_TIME: u64 = 0;

/// Decode a value in the tagged byte layout.
///
/// Scalars are big-endian, lists and geometry protobuf, unknown tags raw.
/// `None` when a known tag carries invalid bytes.
pub(crate) fn decode_tarwyn_type(tag: i32, data: &[u8]) -> Option<supported_values::Kind> {
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

/// `host` as it goes in a URL: an IPv6 address needs brackets, or its colons
/// read as the port separator.
fn url_host(host: &str) -> std::borrow::Cow<'_, str> {
    if host.contains(':') && !host.starts_with('[') {
        std::borrow::Cow::Owned(format!("[{host}]"))
    } else {
        std::borrow::Cow::Borrowed(host)
    }
}

/// The request the client is waiting on, and where its reply goes. A reply
/// must carry its id, or 0 from a server that predates ids.
#[derive(Debug)]
pub(crate) struct PendingRequest {
    pub(crate) id: u64,
    pub(crate) reply: Sender<Vec<u8>>,
}

/// A connection to a tarwyn server. It is `Send + Sync`, and building one
/// never blocks.
///
/// ```no_run
/// use tarwyn_client::client::Client;
///
/// let client = Client::new();
/// let _unsubscribe = client.subscribe("test", |value| println!("{value:?}"));
/// client.start();
/// client.send_bool("test", true);
/// ```
pub struct Client {
    pub(crate) data_listeners: SubscribeListenerMap,
    pub(crate) log_listeners: LogListenerMap,
    pub(crate) outbound: Mutex<SyncSender<Vec<u8>>>,
    pub(crate) writer: SharedWriter,
    pub(crate) pending: Arc<Mutex<Option<PendingRequest>>>,
    pub(crate) topic_names: TopicNames,
    pub(crate) pubuids: Arc<Mutex<HashMap<String, u32>>>,
    pub(crate) session: SessionState,
    pub(crate) next_pubuid: Arc<AtomicU32>,
    pub(crate) next_subuid: Arc<AtomicU32>,
    pub(crate) next_request: AtomicU64,
    pub(crate) request_lock: Mutex<()>,
    pub(crate) request_timeout: Duration,
    pub(crate) send_high_water_mark: usize,
    pub(crate) busy_poll: Duration,
    pub(crate) predict: Duration,
    pub(crate) url: String,
    pub(crate) subprotocol: String,
    pub(crate) telemetry_socket: Arc<std::net::UdpSocket>,
    pub(crate) telemetry_target: std::net::SocketAddr,
    pub(crate) telemetry_listeners: TelemetryListenerMap,
    pub(crate) telemetry_started: Arc<AtomicBool>,
    pub(crate) telemetry_keepalive: Arc<AtomicBool>,
    pub(crate) threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    pub(crate) dropped: Arc<AtomicU64>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) initialized: Arc<AtomicBool>,
    pub(crate) reader_started: Arc<AtomicBool>,
    pub(crate) reader_alive: Arc<AtomicBool>,
    pub(crate) logger: std::sync::OnceLock<tarwyn_protobuf::wpilog::Logger>,
}

impl Client {
    /// Connect to a server on localhost with the default ports.
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Connect to a server on another machine, such as a coprocessor or the robot controller.
    ///
    /// ```no_run
    /// # use tarwyn_client::client::Client;
    /// let client = Client::connect("10.4.88.2");
    /// ```
    pub fn connect(host: &str) -> Self {
        Self::with_config(Config {
            host: host.to_string(),
            ..Default::default()
        })
    }

    /// Connect with the ports and timeout spelled out.
    ///
    /// # Panics
    ///
    /// Panics if the host cannot be resolved or a socket cannot be bound.
    /// [`try_with_config`](Self::try_with_config) returns the error instead.
    pub fn with_config(config: Config) -> Self {
        Self::try_with_config(config).expect("could not construct a tarwyn client")
    }

    /// As [`with_config`](Self::with_config), reporting setup failure instead of
    /// panicking.
    pub fn try_with_config(config: Config) -> Result<Self, ConnectError> {
        use std::net::ToSocketAddrs;

        let endpoint = format!(
            "ws://{}:{}{}",
            url_host(&config.host),
            config.port,
            TABLE_PATH
        );
        (config.host.as_str(), config.port)
            .to_socket_addrs()
            .map_err(|source| ConnectError::Connect {
                socket: "WebSocket",
                endpoint: endpoint.clone(),
                source,
            })?;

        let (tx, _rx) = sync_channel(config.send_high_water_mark.max(1) as usize);

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        Ok(Client {
            data_listeners: Arc::new(Mutex::new(HashMap::new())),
            log_listeners: Arc::new(Mutex::new(SlotMap::new())),
            outbound: Mutex::new(tx),
            writer: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(None)),
            topic_names: Arc::new(Mutex::new(HashMap::new())),
            session: Arc::new(Mutex::new(HashMap::new())),
            pubuids: Arc::new(Mutex::new(HashMap::new())),
            next_pubuid: Arc::new(AtomicU32::new(0)),
            next_subuid: Arc::new(AtomicU32::new(0)),
            next_request: AtomicU64::new(0),
            request_lock: Mutex::new(()),
            request_timeout: config.request_timeout,
            send_high_water_mark: config.send_high_water_mark.max(1) as usize,
            busy_poll: config.busy_poll,
            predict: config.predict,
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

    /// Spawn the reader thread if it is not already running. Every call that
    /// touches the wire does this, so [`start`](Self::start) is optional.
    pub(crate) fn ensure_reader(&self) {
        if self.stop.load(Ordering::SeqCst) || self.reader_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = sync_channel(self.send_high_water_mark);
        *self.outbound.lock().unwrap_or_else(|p| p.into_inner()) = tx;

        let url = self.url.clone();
        let subprotocol = self.subprotocol.clone();
        let data_listeners = Arc::clone(&self.data_listeners);
        let log_listeners = Arc::clone(&self.log_listeners);
        let topic_names = Arc::clone(&self.topic_names);
        let session = Arc::clone(&self.session);
        let pending = Arc::clone(&self.pending);
        let stop = Arc::clone(&self.stop);
        let dropped = Arc::clone(&self.dropped);
        let reader_alive = Arc::clone(&self.reader_alive);
        let writer = Arc::clone(&self.writer);
        let busy_poll = self.busy_poll;
        let predict = self.predict;

        let handle = std::thread::spawn(move || {
            reader_loop(
                rx,
                url,
                subprotocol,
                data_listeners,
                log_listeners,
                topic_names,
                pending,
                session,
                stop,
                dropped,
                reader_alive,
                writer,
                busy_poll,
                predict,
            );
        });
        self.track(handle);
    }

    /// Sends one control-plane request and waits for its reply. Ids start at
    /// 1, because 0 is what older servers echo.
    fn request(&self, payload: request::Payload) -> Option<reply::Payload> {
        let _guard = self.request_lock.lock().ok()?;
        self.ensure_reader();
        let id = self.next_request.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut pending = self.pending.lock().ok()?;
            *pending = Some(PendingRequest { id, reply: tx });
        }
        let message = Request {
            id,
            payload: Some(payload),
        }
        .encode_to_vec();
        if !self.dispatch_frame(message) {
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

    /// Publish an already-built value, for callers that hold a
    /// [`Kind`](supported_values::Kind) instead of a Rust primitive.
    pub fn send_message_public(&self, channel: &str, value: Value) {
        self.send_message(channel, value);
    }

    /// Send one encoded frame, on this thread where the connection allows it,
    /// else through the queue. Returns false if it could be neither.
    pub(crate) fn dispatch_frame(&self, frame: Vec<u8>) -> bool {
        let Some(frame) = write_frame(&self.writer, frame) else {
            return true;
        };
        self.outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_send(frame)
            .is_ok()
    }

    pub(crate) fn send_message(&self, channel: &str, value: impl Into<Value>) {
        let value = value.into();
        if let Some(logger) = self.logger.get() {
            logger.record(channel, supported_values::Kind::from(value.clone()));
        }
        self.ensure_reader();
        let pubuid = self.ensure_pubuid(channel, &value);
        let mut frame = Vec::with_capacity(VALUE_FRAME_HINT);
        encode_into(&value, SERVER_TIME, pubuid, &mut frame);
        if !self.dispatch_frame(frame) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Make sure a channel has a publisher UID, publishing it on first use.
    fn ensure_pubuid(&self, channel: &str, value: &Value) -> u32 {
        self.ensure_pubuid_typed(channel, value, None, Map::new())
    }

    pub(crate) fn ensure_pubuid_typed(
        &self,
        channel: &str,
        value: &Value,
        declared_type: Option<&str>,
        properties: Map<String, serde_json::Value>,
    ) -> u32 {
        let mut pubuids = self.pubuids.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(&pubuid) = pubuids.get(channel) {
            return pubuid;
        }
        let pubuid = self.next_pubuid.fetch_add(1, Ordering::Relaxed);
        pubuids.insert(channel.to_string(), pubuid);
        let data_type = match declared_type {
            Some(name) => name.to_string(),
            None => type_string(xt_data_type(value))
                .unwrap_or("bin")
                .to_string(),
        };
        let publish = ControlMessage::Publish {
            name: channel.to_string(),
            pubuid,
            data_type,
            properties,
        };
        let frame = publish.to_json().into_bytes();
        self.remember(format!("publish:{channel}"), frame.clone());
        self.dispatch_frame(frame);
        pubuid
    }

    /// Keep a control frame to replay if the connection is remade.
    pub(crate) fn remember(&self, key: String, frame: Vec<u8>) {
        if let Ok(mut session) = self.session.lock() {
            session.insert(key, frame);
        }
    }

    /// The WPILib struct schemas a `Pose2d` topic depends on, innermost first,
    /// as a dashboard needs them to decode the bytes.
    pub(crate) const POSE2D_SCHEMAS: &'static [(&'static str, &'static str)] = &[
        ("struct:Translation2d", "double x;double y"),
        ("struct:Rotation2d", "double value"),
        (
            "struct:Pose2d",
            "Translation2d translation;Rotation2d rotation",
        ),
    ];

    /// The WPILib struct schemas a `Pose3d` topic depends on, innermost first.
    pub(crate) const POSE3D_SCHEMAS: &'static [(&'static str, &'static str)] = &[
        ("struct:Translation3d", "double x;double y;double z"),
        ("struct:Quaternion", "double w;double x;double y;double z"),
        ("struct:Rotation3d", "Quaternion q"),
        (
            "struct:Pose3d",
            "Translation3d translation;Rotation3d rotation",
        ),
    ];

    /// Mirror every published value into a [WPILOG](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
    /// file for AdvantageScope, Elastic or the DataLogTool, without ever
    /// blocking a publish.
    ///
    /// # Errors
    ///
    /// Returns [`AlreadyExists`](std::io::ErrorKind::AlreadyExists) if a log is
    /// already open, which is checked before the file is touched, and otherwise
    /// the error from creating the file.
    pub fn log_to(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.refuse_second_log()?;
        let logger = tarwyn_protobuf::wpilog::Logger::open(path)?;
        self.logger
            .set(logger)
            .map_err(|_| std::io::ErrorKind::AlreadyExists.into())
    }

    /// As [`log_to`](Self::log_to), on the first writable mount under `/media`,
    /// `/run/media` or `/mnt`, and returns the path it chose.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`log_to`](Self::log_to), and
    /// [`NotFound`](std::io::ErrorKind::NotFound) when there is no mount to try.
    pub fn log_to_drive(&self, filename: &str) -> std::io::Result<std::path::PathBuf> {
        self.refuse_second_log()?;
        let (logger, path) = tarwyn_protobuf::wpilog::Logger::open_on_drive(filename)?;
        self.logger
            .set(logger)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::AlreadyExists))?;
        Ok(path)
    }

    /// `AlreadyExists` once a log is open.
    fn refuse_second_log(&self) -> std::io::Result<()> {
        match self.logger.get() {
            Some(_) => Err(std::io::ErrorKind::AlreadyExists.into()),
            None => Ok(()),
        }
    }

    /// How many log records were dropped because the writer queue was full. Zero if
    /// logging was never started.
    pub fn log_dropped(&self) -> u64 {
        self.logger
            .get()
            .map(|logger| logger.dropped())
            .unwrap_or(0)
    }

    /// Whether the log writer still succeeds. The first I/O error stops it for
    /// good. `true` when logging was never started.
    pub fn logging_healthy(&self) -> bool {
        self.logger.get().is_none_or(|logger| logger.is_healthy())
    }

    /// How many publishes were dropped because a queue was full, across both
    /// transports.
    pub fn dropped_publishes(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Read the current value of a channel from the server. `None` if unset or
    /// if the server does not answer in [`request_timeout`](Config::request_timeout).
    pub fn get(&self, channel: &str) -> Option<Value> {
        match self.request(request::Payload::Data(GetDataCommand {
            channel: channel.to_string(),
        }))? {
            reply::Payload::Data(command) => {
                let kind = command.value?.kind?;
                if kind == supported_values::Kind::String(NO_DATA_SENTINEL.to_string()) {
                    None
                } else {
                    Some(Value::from(kind))
                }
            }
            _ => None,
        }
    }

    /// Delete a channel. Returns how many were removed, 0 or 1.
    pub fn delete(&self, channel: &str) -> u32 {
        let request = request::Payload::Delete(DeleteCommand {
            channel: channel.to_string(),
        });
        match self.request(request) {
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
        let request = request::Payload::Tables(ListTablesCommand {
            prefix: prefix.to_string(),
        });
        match self.request(request) {
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
        let request = request::Payload::Ping(PingCommand { sent_nanos: sent });
        match self.request(request)? {
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

    /// Server counters: uptime, channel count, messages handled. `None` if the
    /// server does not answer.
    pub fn statistics(&self) -> Option<ReplyStatisticsCommand> {
        let request = request::Payload::Statistics(StatisticsCommand {});
        match self.request(request)? {
            reply::Payload::Statistics(command) => Some(command),
            _ => None,
        }
    }

    /// The channels beginning with `prefix`, as a JSON document. `"{}"` if the server
    /// does not answer.
    pub fn raw_json(&self, prefix: &str) -> String {
        let request = request::Payload::Json(JsonCommand {
            prefix: prefix.to_string(),
        });
        match self.request(request) {
            Some(reply::Payload::Json(command)) => command.json,
            _ => String::from("{}"),
        }
    }

    /// Set a channel only if it holds `expected` (`None`: only if empty), and
    /// report whether it swapped. Atomic on the server, unlike `get` then publish.
    ///
    /// ```no_run
    /// # use tarwyn_client::{Client, Value};
    /// # let client = Client::new();
    /// let won = client.compare_and_set("path-lock", None, Value::String("agent-a".into()));
    /// ```
    pub fn compare_and_set(&self, channel: &str, expected: Option<Value>, value: Value) -> bool {
        let wire = |value: Value| {
            Box::new(SupportedValues {
                kind: Some(value.into()),
            })
        };
        let request = request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: channel.to_string(),
            expect_absent: expected.is_none(),
            expected: expected.map(wire),
            value: Some(wire(value)),
        });
        match self.request(request) {
            Some(reply::Payload::CompareAndSet(command)) => command.swapped,
            _ => false,
        }
    }

    fn get_logs(&self) -> Vec<String> {
        match self.request(request::Payload::Logs(GetLogsCommand {})) {
            Some(reply::Payload::Logs(command)) => command.logs,
            _ => Vec::new(),
        }
    }

    /// Run `callback` for every value published to a channel.
    ///
    /// The current value is delivered before this returns, and values arrive
    /// once [`start`](Self::start) runs. Call the returned closure to
    /// unsubscribe. Nothing is missed, but a value may arrive twice.
    pub fn subscribe<F>(&self, channel: &str, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&Value) + Send + Sync + 'static,
    {
        let listener = Arc::new(BufferedListener::new(callback));
        let buffered = Arc::clone(&listener);

        self.ensure_reader();
        let subuid = self.next_subuid.fetch_add(1, Ordering::Relaxed);
        let subscribe = ControlMessage::Subscribe {
            topics: vec![channel.to_string()],
            subuid,
            options: Map::new(),
        };
        let frame = subscribe.to_json().into_bytes();
        let session_key = format!("subscribe:{subuid}");
        self.remember(session_key.clone(), frame.clone());
        self.dispatch_frame(frame);

        let key = self.data_listeners.lock().ok().map(|mut listeners| {
            listeners
                .entry(channel.to_string())
                .or_default()
                .insert(Arc::new(move |value: &Value| {
                    listener.deliver(value);
                }))
        });

        // Subscribed before this read, so nothing published meanwhile is lost.
        if let Some(initial_value) = self.get(channel) {
            buffered.call(&initial_value);
        }
        buffered.open();

        let listeners = Arc::clone(&self.data_listeners);
        let unsubscribe = self.unsubscriber(subuid, session_key);
        let channel = channel.to_string();

        move || {
            unsubscribe();
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

    /// The closure that ends a subscription: it drops the session's replay
    /// frame and tells the server, which would otherwise keep sending.
    fn unsubscriber(&self, subuid: u32, session_key: String) -> impl FnOnce() + Send + 'static {
        let session = Arc::clone(&self.session);
        let writer = Arc::clone(&self.writer);
        let outbound = self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        move || {
            if let Ok(mut session) = session.lock() {
                session.remove(&session_key);
            }
            let frame = ControlMessage::Unsubscribe { subuid }
                .to_json()
                .into_bytes();
            if let Some(frame) = write_frame(&writer, frame) {
                let _ = outbound.try_send(frame);
            }
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
        let subscribe = ControlMessage::Subscribe {
            topics: vec![LOG_TOPIC.to_string()],
            subuid,
            options: Map::new(),
        };
        let frame = subscribe.to_json().into_bytes();
        let session_key = format!("subscribe:{subuid}");
        self.remember(session_key.clone(), frame.clone());
        self.dispatch_frame(frame);

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
        let unsubscribe = self.unsubscriber(subuid, session_key);

        move || {
            unsubscribe();
            let (Some(key), Ok(mut listeners)) = (key, listeners.lock()) else {
                return;
            };
            listeners.remove(key);
        }
    }

    /// Start the receive threads, so subscriptions begin delivering.
    ///
    /// Publishing and [`get`](Self::get) work without this. Calling it again after
    /// [`stop`](Self::stop) resumes. Calling it on a running client does nothing.
    pub fn start(&self) {
        if !self.initialized.swap(true, Ordering::SeqCst) {
            self.stop.store(false, Ordering::SeqCst);
        } else if !self.stop.swap(false, Ordering::SeqCst) {
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

    /// Stop and join the receive threads, which takes up to 100 ms.
    /// Subscriptions resume on the next [`start`](Self::start).
    ///
    /// From a callback it skips joining its own thread, which stops once the
    /// callback returns.
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

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Client")
            .field("telemetry_target", &self.telemetry_target)
            .field("running", &!self.stop.load(Ordering::SeqCst))
            .field("dropped_publishes", &self.dropped_publishes())
            .finish_non_exhaustive()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// Stops the receive threads, so a client that goes out of scope does not leave
/// them decoding into listeners nobody holds.
impl Drop for Client {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
