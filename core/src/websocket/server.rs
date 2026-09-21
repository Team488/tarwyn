//! The NT4 server accept loop and connection wiring.
//!
//! [`Server`] binds a [`TcpListener`], accepts NT4 clients over `/nt/<path>`,
//! and runs one reader thread per connection. Inbound binary payloads are
//! decoded and routed to the shared [`NtRegistry`]; the returned fan-out routes
//! are dispatched through a shared [`ConnectionMap`] to per-client writer
//! threads. The registry and connection map are each behind their own
//! [`Mutex`]; the two locks are never held together, and every acquire recovers
//! from poisoning via [`Mutex::into_inner`].

use std::collections::HashMap;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::value::Value;
use crate::websocket::frame::{Payload, WebsocketConnection, WebsocketReader};
use crate::websocket::message::{ControlMessage, RTT_TOPIC_ID, ValueMessage};
use crate::websocket::protocol::{ClientId, NtRegistry, Outbound, PersistentTopic, encode_once};
use crate::websocket::transport::{
    Client, ConnectionMap, KEEPALIVE_INTERVAL_MS, PUB_HIGH_WATER_MARK, RouteMsg, deliver,
    writer_loop,
};

/// How many times a port is tried before the bind is reported as failed.
const BIND_ATTEMPTS: u32 = 5;
/// How long to wait between bind attempts.
const BIND_RETRY: Duration = Duration::from_millis(200);
/// Connections the server will serve at once.
///
/// Each one costs a reader and a writer thread, so an uncapped accept loop is
/// a way to exhaust the process. ntcore's own server keeps its client list in
/// a vector it linear-scans, noting the count "is typically small (<10)"; a
/// real robot has the driver station, a dashboard or two and a few
/// coprocessors. This leaves room for that plus reconnect churn, where a
/// dropped connection can still hold its slot for a moment.
pub const MAX_CONNECTIONS: usize = 32;
/// The address the server listens on when a caller does not narrow it.
///
/// Every interface, matching the UDP telemetry plane. NT4's clients are the
/// driver station and the coprocessors, none of which are on this host.
pub const DEFAULT_BIND_HOST: &str = "0.0.0.0";
/// How often persistent topics are checked for changes worth writing to disk.
const PERSIST_INTERVAL: Duration = Duration::from_secs(5);
/// Where persistent topics are saved when no path is given.
///
/// The file follows the same shape as ntcore's `networktables.json` but is
/// named separately, so running alongside a real NetworkTables server on one
/// host cannot leave the two overwriting each other.
const DEFAULT_PERSISTENCE_FILE: &str = "tarwyn.json";
/// How long the accept loop waits after a failed accept before trying again,
/// so a listener in an error state does not spin.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// A callback that answers a control-plane request.
///
/// The tarwyn control plane (get/delete/tables/ping/stats/json/CAS/logs) rides
/// the WebSocket connection as binary protobuf `Request`/`Reply` frames. The WebSocket layer
/// stays protobuf-free: it hands the raw inbound bytes to this callback and
/// writes whatever bytes it returns back to the same connection. `None` means
/// the bytes were not a valid control request, and the connection is closed.
pub type ControlHandler = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// A callback that stores a WebSocket-originated value into the server's read cache.
///
/// A value that arrives over WebSocket has already been fanned out to NT4 subscribers
/// by the registry; this sink only writes the server's `cached_messages` so the
/// control plane can read it back. It must not fan out again, or every value
/// reaches its subscribers twice.
pub type ValueSink = Arc<dyn Fn(&str, &Value) + Send + Sync>;

/// A control handler that answers nothing (used by the plain `bind`).
fn noop_handler() -> ControlHandler {
    Arc::new(|_| None)
}

/// A value sink that stores nothing (used by the plain `bind`).
fn noop_sink() -> ValueSink {
    Arc::new(|_, _| {})
}

/// The NT4 server.
pub struct Server {
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
    persistence_path: PathBuf,
    busy_poll_micros: Arc<AtomicU64>,
    predict_micros: Arc<AtomicU64>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("listener", &self.listener)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Binds the server to `port`, retrying up to `BIND_ATTEMPTS` times.
    ///
    /// # Errors
    ///
    /// Returns the last [`io::Error`] if the port cannot be bound after all
    /// attempts.
    pub fn bind(port: u16) -> io::Result<Self> {
        Self::bind_with_handler(DEFAULT_BIND_HOST, port, noop_handler(), noop_sink())
    }

    /// Binds to an OS-assigned loopback port (for tests).
    pub fn bind_loopback() -> io::Result<Self> {
        Self::bind_loopback_with_handler(noop_handler(), noop_sink())
    }

    /// Binds the server to `host:port` with a control-plane handler and value sink.
    ///
    /// `host` is [`DEFAULT_BIND_HOST`] unless a caller narrows it. Binding
    /// loopback would leave the driver station and every coprocessor unable to
    /// reach the server, which is the whole point of the port.
    ///
    /// `control_handler` answers binary protobuf control requests; `value_sink`
    /// stores WebSocket-originated values into the server's read cache. See the type
    /// aliases for the exact contracts.
    pub fn bind_with_handler(
        host: &str,
        port: u16,
        control_handler: ControlHandler,
        value_sink: ValueSink,
    ) -> io::Result<Self> {
        let addr = format!("{host}:{port}");
        let mut last_err = None;
        for _ in 0..BIND_ATTEMPTS {
            match TcpListener::bind(&addr) {
                Ok(listener) => {
                    return Ok(Self {
                        listener,
                        registry: Arc::new(Mutex::new(NtRegistry::new())),
                        conns: Arc::new(Mutex::new(ConnectionMap::new())),
                        sockets: Arc::new(Mutex::new(HashMap::new())),
                        live: Arc::new(AtomicUsize::new(0)),
                        stop: Arc::new(AtomicBool::new(false)),
                        control_handler,
                        value_sink,
                        persistence_path: PathBuf::from(DEFAULT_PERSISTENCE_FILE),
                        busy_poll_micros: Arc::new(AtomicU64::new(0)),
                        predict_micros: Arc::new(AtomicU64::new(default_predict_micros())),
                    });
                }
                Err(e) => {
                    last_err = Some(e);
                    thread::sleep(BIND_RETRY);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| io::Error::other("bind failed")))
    }

    /// Binds to an OS-assigned loopback port with a handler and sink (for tests).
    pub fn bind_loopback_with_handler(
        control_handler: ControlHandler,
        value_sink: ValueSink,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        Ok(Self {
            listener,
            registry: Arc::new(Mutex::new(NtRegistry::new())),
            conns: Arc::new(Mutex::new(ConnectionMap::new())),
            sockets: Arc::new(Mutex::new(HashMap::new())),
            live: Arc::new(AtomicUsize::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
            control_handler,
            value_sink,
            persistence_path: PathBuf::from(DEFAULT_PERSISTENCE_FILE),
            busy_poll_micros: Arc::new(AtomicU64::new(0)),
            predict_micros: Arc::new(AtomicU64::new(default_predict_micros())),
        })
    }

    /// The bound local address.
    pub fn local_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    /// The shared stop flag.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Stops accepting and shuts every established connection down.
    ///
    /// The accept loop blocks in `accept` and is woken by a connection from
    /// this process to its own port, which it drops once it sees the flag. A
    /// connection's reader thread is likewise blocked in `recv` with no
    /// timeout and would keep serving a stopped server, holding its port
    /// state and answering clients that think they are talking to a live one;
    /// shutting the socket down is what ends that read.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(local) = self.listener.local_addr() {
            let loopback = std::net::SocketAddr::new(loopback_for(local), local.port());
            let _ = TcpStream::connect_timeout(&loopback, BIND_RETRY);
        }
        let sockets = self.sockets.lock().unwrap_or_else(|p| p.into_inner());
        for socket in sockets.values() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }

    /// How long a connection's reader spins on its socket before it blocks.
    ///
    /// Zero, the default, blocks at once. A window covering the publish
    /// interval roughly halves one-way latency at 500 Hz and keeps a core
    /// busy. Applies to connections accepted after the call.
    pub fn set_busy_poll(&self, window: Duration) {
        let micros = u64::try_from(window.as_micros()).unwrap_or(u64::MAX);
        self.busy_poll_micros.store(micros, Ordering::Relaxed);
    }

    /// The reader spin window; see [`Server::set_busy_poll`].
    pub fn busy_poll(&self) -> Duration {
        Duration::from_micros(self.busy_poll_micros.load(Ordering::Relaxed))
    }

    /// How far around a predicted arrival a connection's reader spins; see
    /// [`Predictor`](crate::websocket::pacing::Predictor).
    ///
    /// Defaults to [`DEFAULT_MARGIN`](crate::websocket::pacing::DEFAULT_MARGIN);
    /// zero turns it off. Applies to connections accepted after the call.
    pub fn set_predict(&self, margin: Duration) {
        let micros = u64::try_from(margin.as_micros()).unwrap_or(u64::MAX);
        self.predict_micros.store(micros, Ordering::Relaxed);
    }

    /// The prediction margin; see [`Server::set_predict`].
    pub fn predict(&self) -> Duration {
        Duration::from_micros(self.predict_micros.load(Ordering::Relaxed))
    }

    /// Starts the accept loop, returning its thread handle.
    pub fn start(&self) -> JoinHandle<()> {
        let registry = self.registry.clone();
        let conns = self.conns.clone();
        let sockets = self.sockets.clone();
        let live = self.live.clone();
        let stop = self.stop.clone();
        let control_handler = self.control_handler.clone();
        let value_sink = self.value_sink.clone();
        let busy_poll = self.busy_poll_micros.clone();
        let predict = self.predict_micros.clone();
        let listener = self
            .listener
            .try_clone()
            .expect("cloning a bound listener is infallible");
        self.start_persistence();
        thread::spawn(move || {
            accept_loop(
                listener,
                registry,
                conns,
                sockets,
                live,
                stop,
                control_handler,
                value_sink,
                busy_poll,
                predict,
            )
        })
    }

    /// The file persistent topics are written to and reloaded from.
    pub fn persistence_path(&self) -> &Path {
        &self.persistence_path
    }

    /// Sets where persistent topics are saved, before [`Server::start`].
    ///
    /// The default is relative to the working directory, which on a robot
    /// controller is wherever the program was launched from. Point this at an
    /// absolute path if the value has to outlive a redeploy.
    pub fn set_persistence_path(&mut self, path: impl Into<PathBuf>) {
        self.persistence_path = path.into();
    }

    /// Restores saved topics, then checks every [`PERSIST_INTERVAL`] whether
    /// they changed and saves them if so, until the server stops.
    ///
    /// The file is written only when the registry's persistent generation has
    /// moved since the last write: on a controller booting from an SD card, a
    /// rewrite every few seconds for the life of the robot is wear for nothing.
    fn start_persistence(&self) {
        load_persistent(&self.registry, &self.persistence_path);
        let registry = self.registry.clone();
        let stop = self.stop.clone();
        let path = self.persistence_path.clone();
        thread::spawn(move || {
            let mut saved = generation(&registry);
            loop {
                let stopping = stop.load(Ordering::Relaxed);
                if !stopping {
                    thread::sleep(PERSIST_INTERVAL);
                }
                let current = generation(&registry);
                if current != saved && save_persistent(&registry, &path).is_ok() {
                    saved = current;
                }
                if stopping {
                    return;
                }
            }
        });
    }

    /// Fans a value out to subscribers of `name`.
    pub fn fan_out(&self, name: &str, value: &Value, ts_micros: u64) {
        let routes = {
            let mut reg = self.registry.lock().unwrap_or_else(|p| p.into_inner());
            let Some(id) = reg.topic_id(name) else {
                return;
            };
            reg.handle_topic_value(id, value, ts_micros)
        };
        let (plan, dropped) = {
            let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
            (map.plan(routes), map.drop_counter())
        };
        deliver(plan, &dropped);
    }

    /// Fans a value out to subscribers of `name`, creating the topic if needed.
    ///
    /// Used by the control plane (CAS) where a value may be assigned to a
    /// channel no NT4 client has published yet. The topic is created with the
    /// value's data type so it is readable and subscribeable.
    pub fn fan_out_upsert(&self, name: &str, value: &Value, ts_micros: u64) {
        let routes = {
            let mut reg = self.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.handle_upsert_value(name, value.clone(), ts_micros)
        };
        let (plan, dropped) = {
            let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
            (map.plan(routes), map.drop_counter())
        };
        deliver(plan, &dropped);
    }

    /// How many fan-out frames were dropped because a subscriber's channel was
    /// full.
    pub fn dropped_publishes(&self) -> u64 {
        self.conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dropped()
            .load(Ordering::Relaxed)
    }
}

/// Encodes one value the way ntcore writes it to `networktables.json`.
///
/// Scalars and arrays are native JSON so the file stays hand-editable; raw
/// types become base64, matching ntcore's `DumpValue`.
fn value_to_json(value: &Value) -> serde_json::Value {
    use serde_json::json;
    match value {
        Value::Bool(v) => json!(v),
        Value::Double(v) => json!(v),
        Value::Float(v) => json!(v),
        Value::Int8(v) => json!(v),
        Value::Int16(v) => json!(v),
        Value::Int32(v) => json!(v),
        Value::Int64(v) => json!(v),
        Value::Uint8(v) => json!(v),
        Value::Uint16(v) => json!(v),
        Value::Uint32(v) => json!(v),
        Value::Uint64(v) => json!(v),
        Value::String(v) => json!(v),
        Value::BoolArray(v) => json!(v),
        Value::DoubleArray(v) => json!(v),
        Value::FloatArray(v) => json!(v),
        Value::Int8Array(v) => json!(v),
        Value::Int16Array(v) => json!(v),
        Value::Int32Array(v) => json!(v),
        Value::Int64Array(v) => json!(v),
        Value::Uint8Array(v) => json!(v),
        Value::Uint16Array(v) => json!(v),
        Value::Uint32Array(v) => json!(v),
        Value::Uint64Array(v) => json!(v),
        Value::StringArray(v) => json!(v),
        Value::Bytes(v) | Value::BytesList(v) | Value::Coordinate(v) | Value::Bezier(v) => {
            serde_json::Value::String(data_encoding::BASE64.encode(v))
        }
    }
}

/// Rebuilds a value from its type string and the JSON [`value_to_json`] wrote.
fn value_from_json(type_str: &str, value: &serde_json::Value) -> Option<Value> {
    let numbers = |v: &serde_json::Value| -> Option<Vec<f64>> {
        v.as_array()?
            .iter()
            .map(serde_json::Value::as_f64)
            .collect()
    };
    match type_str {
        "boolean" => Some(Value::Bool(value.as_bool()?)),
        "double" => Some(Value::Double(value.as_f64()?)),
        "float" => Some(Value::Float(value.as_f64()? as f32)),
        "int" => Some(Value::Int64(value.as_i64()?)),
        "string" | "json" => Some(Value::String(value.as_str()?.to_owned())),
        "boolean[]" => Some(Value::BoolArray(
            value
                .as_array()?
                .iter()
                .map(serde_json::Value::as_bool)
                .collect::<Option<Vec<bool>>>()?,
        )),
        "double[]" => Some(Value::DoubleArray(numbers(value)?)),
        "float[]" => Some(Value::FloatArray(
            numbers(value)?.into_iter().map(|v| v as f32).collect(),
        )),
        "int[]" => Some(Value::Int64Array(
            value
                .as_array()?
                .iter()
                .map(serde_json::Value::as_i64)
                .collect::<Option<Vec<i64>>>()?,
        )),
        "string[]" => Some(Value::StringArray(
            value
                .as_array()?
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<String>>>()?,
        )),
        _ => Some(Value::Bytes(
            data_encoding::BASE64
                .decode(value.as_str()?.as_bytes())
                .ok()?,
        )),
    }
}

/// Encodes persistent topics as the `networktables.json` ntcore writes.
fn persistent_to_json(entries: &[PersistentTopic]) -> String {
    let rows: Vec<serde_json::Value> = entries
        .iter()
        .map(|(name, type_str, value, properties)| {
            serde_json::json!({
                "name": name,
                "type": type_str,
                "value": value_to_json(value),
                "properties": properties,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::Value::Array(rows)).unwrap_or_else(|_| "[]".into())
}

/// Decodes a `networktables.json`, skipping any entry it cannot read.
fn persistent_from_json(text: &str) -> Vec<PersistentTopic> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?.to_owned();
            let type_str = row.get("type")?.as_str()?.to_owned();
            let value = value_from_json(&type_str, row.get("value")?)?;
            let properties = row
                .get("properties")
                .and_then(|p| p.as_object().cloned())
                .unwrap_or_default();
            Some((name, type_str, value, properties))
        })
        .collect()
}

/// The registry's persistent generation; see
/// [`NtRegistry::persistent_generation`].
fn generation(registry: &Arc<Mutex<NtRegistry>>) -> u64 {
    registry
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .persistent_generation()
}

/// Writes persistent topics to `path`, replacing whatever was there.
///
/// # Errors
///
/// Returns the [`io::Error`] from writing the file.
pub fn save_persistent(registry: &Arc<Mutex<NtRegistry>>, path: &Path) -> io::Result<()> {
    let entries = registry
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .persistent_snapshot();
    let mut scratch = path.as_os_str().to_owned();
    scratch.push(".tmp");
    let scratch = PathBuf::from(scratch);
    std::fs::write(&scratch, persistent_to_json(&entries))?;
    std::fs::rename(scratch, path)
}

/// Loads persistent topics from `path`, if it exists and parses.
pub fn load_persistent(registry: &Arc<Mutex<NtRegistry>>, path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let entries = persistent_from_json(&text);
    if entries.is_empty() {
        return;
    }
    registry
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .restore_persistent(entries, now_micros());
}

/// Runs the accept loop until `stop` is set.
///
/// Blocks in `accept`; [`Server::stop`] wakes it with a connection of its own,
/// which is dropped here. Accepted sockets are put into blocking mode
/// explicitly, since macOS and Windows hand back a socket that inherits the
/// listener's flags, and the handshake read needs to block.
#[expect(clippy::too_many_arguments)]
fn accept_loop(
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
    busy_poll: Arc<AtomicU64>,
    predict: Arc<AtomicU64>,
) {
    let _ = listener.set_nonblocking(false);
    let client_ids = AtomicU64::new(0);
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((tcp, _)) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let _ = tcp.set_nonblocking(false);
                if live.fetch_add(1, Ordering::Relaxed) >= MAX_CONNECTIONS {
                    live.fetch_sub(1, Ordering::Relaxed);
                    let _ = tcp.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let id = client_ids.fetch_add(1, Ordering::Relaxed);
                spawn_connection(
                    tcp,
                    id,
                    registry.clone(),
                    conns.clone(),
                    sockets.clone(),
                    live.clone(),
                    control_handler.clone(),
                    value_sink.clone(),
                    Duration::from_micros(busy_poll.load(Ordering::Relaxed)),
                    Duration::from_micros(predict.load(Ordering::Relaxed)),
                );
            }
            Err(_) => thread::sleep(ACCEPT_RETRY),
        }
    }
}

/// [`DEFAULT_MARGIN`](crate::websocket::pacing::DEFAULT_MARGIN) in
/// microseconds, the form the setting is stored in.
fn default_predict_micros() -> u64 {
    u64::try_from(crate::websocket::pacing::DEFAULT_MARGIN.as_micros()).unwrap_or(u64::MAX)
}

/// The loopback address of the same family as `bound`, for reaching a
/// listener bound to the unspecified address from its own process.
fn loopback_for(bound: std::net::SocketAddr) -> std::net::IpAddr {
    if bound.ip().is_unspecified() {
        match bound {
            std::net::SocketAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
            std::net::SocketAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
        }
    } else {
        bound.ip()
    }
}

/// Holds one of [`MAX_CONNECTIONS`] slots, releasing it however the thread ends.
struct ConnectionSlot(Arc<AtomicUsize>);

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Spawns the reader and writer threads for a freshly accepted connection.
///
/// tungstenite answers pings and closes from inside `read`, so the connection
/// cannot be split into two independent halves. Instead the writer thread owns
/// the socket outright and the reader routes its own outgoing bytes through
/// the same channel, which leaves both threads blocked on an event rather than
/// polling a socket timeout the kernel rounds up to milliseconds. Those bytes
/// are counted like any other queued frame: the writer thread decrements once
/// per message it takes, whoever put it there.
#[expect(clippy::too_many_arguments)]
fn spawn_connection(
    tcp: TcpStream,
    id: ClientId,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
    busy_poll: Duration,
    predict: Duration,
) {
    thread::spawn(move || {
        let _slot = ConnectionSlot(live);
        let Ok(conn) = WebsocketConnection::accept(tcp) else {
            return;
        };
        if conn.is_rtt_only() {
            serve_rtt(conn);
            return;
        }
        if let Ok(socket) = conn.try_clone_socket() {
            sockets
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(id, socket);
        }
        let client_name = conn.client_name().to_owned();
        let peer = conn.peer().to_owned();
        let (tx, rx) = sync_channel(PUB_HIGH_WATER_MARK);
        let queued = Arc::new(AtomicUsize::new(0));
        let sink_tx = tx.clone();
        let sink_queued = Arc::clone(&queued);
        let emit = Box::new(move |bytes| {
            sink_queued.fetch_add(1, Ordering::AcqRel);
            if sink_tx.try_send(RouteMsg::Raw(bytes)).is_err() {
                sink_queued.fetch_sub(1, Ordering::AcqRel);
            }
        });
        let Ok((mut reader, writer)) = conn.split(emit, busy_poll, predict) else {
            return;
        };

        let writer = Arc::new(Mutex::new(writer));
        let client = Client::new(tx, Arc::clone(&writer), Arc::clone(&queued));
        conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .add_client(id, client);
        let connect_routes = {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.on_connect(id, &client_name, &peer)
        };
        conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dispatch(connect_routes);

        let writer_thread = thread::spawn(move || {
            writer_loop(
                &writer,
                &rx,
                &queued,
                Duration::from_millis(KEEPALIVE_INTERVAL_MS),
            );
        });

        serve_connection(
            &mut reader,
            id,
            &registry,
            &conns,
            &control_handler,
            &value_sink,
        );

        sockets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove_client(id);
        let disconnect_routes = {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.on_disconnect(id)
        };
        conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dispatch(disconnect_routes);
        drop(reader);
        let _ = writer_thread.join();
    });
}

/// Reads and routes for one connection until the peer goes away.
///
/// Blocks in `recv` with no timeout; outbound frames are the writer thread's
/// concern, so nothing here polls.
fn serve_connection(
    reader: &mut WebsocketReader,
    id: ClientId,
    registry: &Arc<Mutex<NtRegistry>>,
    conns: &Arc<Mutex<ConnectionMap>>,
    control_handler: &ControlHandler,
    value_sink: &ValueSink,
) {
    let mut mirrored = Vec::new();
    loop {
        let Ok(payload) = reader.recv() else {
            return;
        };
        let outcome = match &payload {
            Payload::Binary(bytes) => {
                route_binary(id, bytes, registry, control_handler, &mut mirrored)
            }
            Payload::Text(text) => route_text(id, text, registry),
        };
        match outcome {
            RouteOutcome::Dispatch(routes) if routes.is_empty() => {}
            RouteOutcome::Dispatch(routes) => {
                conns
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .dispatch(routes);
            }
            RouteOutcome::ControlReply(reply) => {
                let map = conns.lock().unwrap_or_else(|p| p.into_inner());
                map.dispatch(vec![(id, Outbound::Value(Arc::from(reply)))]);
            }
            RouteOutcome::Close => {
                let map = conns.lock().unwrap_or_else(|p| p.into_inner());
                map.send_close(id, 1002, "malformed payload");
                return;
            }
        }
        for (name, value) in mirrored.drain(..) {
            value_sink(&name, &value);
        }
    }
}

/// Answers timestamp messages on a connection accepted for RTT only.
///
/// Kept off the registry and the connection map entirely: this connection has no
/// topics, no subscriptions and no place in the client list, and a reply is
/// written on the reading thread rather than handed to a writer, since the reply
/// is the only thing this connection ever sends.
fn serve_rtt(mut conn: WebsocketConnection) {
    loop {
        let Ok(payload) = conn.recv() else {
            return;
        };
        let Payload::Binary(bytes) = payload else {
            continue;
        };
        let Ok(messages) = ValueMessage::decode_all(&bytes) else {
            continue;
        };
        let mut answered = false;
        for message in messages {
            if message.topic_id != RTT_TOPIC_ID {
                continue;
            }
            conn.write_batched(&encode_once(&message.value, now_micros(), RTT_TOPIC_ID));
            answered = true;
        }
        if answered && conn.flush().is_err() {
            return;
        }
    }
}

/// The outcome of routing one inbound payload.
enum RouteOutcome {
    /// Fan-out routes to dispatch to subscribers (possibly empty).
    Dispatch(Vec<(ClientId, Outbound)>),
    /// A binary control reply to write back to the same connection.
    ControlReply(Vec<u8>),
    /// The payload was malformed; the caller must close the connection.
    Close,
}

/// Routes one inbound binary frame to the registry.
///
/// An NT4 binary frame carries one or more MessagePack value messages, so
/// every message in the frame is decoded and routed. A frame that is not
/// MessagePack is offered to the binary control plane, then parsed as JSON
/// for clients that send control messages over binary frames.
///
/// Every accepted value is pushed to `mirrored` with its topic name, for the
/// caller to hand to the read cache once the routes have been dispatched: the
/// cache is not on a subscriber's path and should not be paid for on it.
fn route_binary(
    id: ClientId,
    payload: &[u8],
    registry: &Arc<Mutex<NtRegistry>>,
    control_handler: &ControlHandler,
    mirrored: &mut Vec<(String, Value)>,
) -> RouteOutcome {
    if let Ok(messages) = ValueMessage::decode_all(payload) {
        let mut routes = Vec::new();
        for vm in messages {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            if vm.topic_id == RTT_TOPIC_ID {
                let server_ts = now_micros();
                routes.extend(reg.handle_timestamp(id, vm.value, server_ts));
                continue;
            }
            let Some(topic_id) = reg.topic_id_for_pubuid(id, vm.topic_id) else {
                continue;
            };
            let accepted = reg.accepts_value(topic_id, &vm.value);
            routes.extend(reg.handle_topic_value(topic_id, &vm.value, vm.timestamp_micros));
            if accepted && let Some(name) = reg.topic_name(topic_id) {
                mirrored.push((name, vm.value));
            }
        }
        return RouteOutcome::Dispatch(routes);
    }
    if let Some(reply) = control_handler(payload) {
        return RouteOutcome::ControlReply(reply);
    }
    match std::str::from_utf8(payload) {
        Ok(text) => route_text(id, text, registry),
        Err(_) => RouteOutcome::Close,
    }
}

/// Routes one inbound text frame to the registry.
///
/// An NT4 text frame is a JSON array of control messages; every message in
/// the frame is routed.
fn route_text(id: ClientId, text: &str, registry: &Arc<Mutex<NtRegistry>>) -> RouteOutcome {
    let Ok(messages) = ControlMessage::from_json_batch(text) else {
        return RouteOutcome::Close;
    };
    let mut routes = Vec::new();
    for msg in messages {
        match route_control(id, msg, registry) {
            RouteOutcome::Dispatch(r) => routes.extend(r),
            other => return other,
        }
    }
    RouteOutcome::Dispatch(routes)
}

/// Applies one decoded control message to the registry.
///
/// Server-to-client and non-standard control messages (`Announce`,
/// `PropertiesUpdate`, `KeepAlive`, `ControlValue`) are ignored and keep the
/// connection open. An unknown data-type string is not an error: NT4 carries
/// it as binary.
fn route_control(
    id: ClientId,
    msg: ControlMessage,
    registry: &Arc<Mutex<NtRegistry>>,
) -> RouteOutcome {
    match msg {
        ControlMessage::Publish {
            name,
            pubuid,
            data_type,
            properties,
        } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_publish(id, &name, pubuid, &data_type, properties))
        }
        ControlMessage::Unpublish { pubuid } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_unpublish(id, pubuid))
        }
        ControlMessage::Subscribe {
            topics,
            subuid,
            options,
        } => {
            let flag = |key: &str| {
                options
                    .get(key)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            };
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_subscribe(
                id,
                &topics,
                subuid,
                flag("prefix"),
                flag("topicsonly"),
                options,
            ))
        }
        ControlMessage::SetProperties { name, update } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_setproperties(id, &name, update))
        }
        ControlMessage::Unsubscribe { subuid } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_unsubscribe(id, subuid))
        }
        ControlMessage::Timestamp { value, .. } => {
            let server_ts = now_micros();
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_timestamp(id, json_to_xtvalue(&value), server_ts))
        }
        ControlMessage::Announce { .. }
        | ControlMessage::Unannounce { .. }
        | ControlMessage::PropertiesUpdate { .. }
        | ControlMessage::ControlValue { .. }
        | ControlMessage::KeepAlive => RouteOutcome::Dispatch(Vec::new()),
    }
}

/// The current time in microseconds since the Unix epoch.
fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Converts a JSON value to an [`Value`] (best-effort).
fn json_to_xtvalue(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Value::Double(f)
            } else if let Some(i) = n.as_i64() {
                Value::Int64(i)
            } else {
                Value::Int64(0)
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        _ => Value::String(String::new()),
    }
}

#[cfg(test)]
mod tests;
