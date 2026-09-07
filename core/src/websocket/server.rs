//! The NT4 server accept loop and connection wiring.
//!
//! [`WebsocketServer`] binds a [`TcpListener`], accepts NT4 clients over `/nt/<path>`,
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

use crate::value::XtValue;
use crate::websocket::frame::{Payload, WebsocketConnection, WebsocketReader};
use crate::websocket::message::{CtMessage, RTT_TOPIC_ID, ValueMessage};
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
/// How often persistent topics are written to disk.
const PERSIST_INTERVAL: Duration = Duration::from_secs(5);
/// Where persistent topics are saved when no path is given.
///
/// The file follows the same shape as ntcore's `networktables.json` but is
/// named separately, so running alongside a real NetworkTables server on one
/// host cannot leave the two overwriting each other.
const DEFAULT_PERSISTENCE_FILE: &str = "tarwyn.json";
/// How long the nonblocking accept loop sleeps between polls for a new
/// connection. This is not on the data path, it only paces idle retries, so
/// it stays lazy (100 ms) to avoid busy-waiting when no client is connecting.
const ACCEPT_POLL_SLEEP: Duration = Duration::from_millis(100);

/// A callback that answers a control-plane request (Task 7 seam).
///
/// The TARWYN control plane (get/delete/tables/ping/stats/json/CAS/logs) rides
/// the WebSocket connection as binary protobuf `Request`/`Reply` frames. The WebSocket layer
/// stays protobuf-free: it hands the raw inbound bytes to this callback and
/// writes whatever bytes it returns back to the same connection. `None` means
/// the bytes were not a valid control request, and the connection is closed.
pub type ControlHandler = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// A callback that stores a WebSocket-originated value into the server's read cache.
///
/// A value that arrives over WebSocket has already been fanned out to NT4 subscribers
/// by the registry; this sink only writes the server's `cached_messages` so the
/// control plane can read it back. It must NOT fan out again (that would
/// double-broadcast).
pub type ValueSink = Arc<dyn Fn(&str, &XtValue) + Send + Sync>;

/// A control handler that answers nothing (used by the plain `bind`).
fn noop_handler() -> ControlHandler {
    Arc::new(|_| None)
}

/// A value sink that stores nothing (used by the plain `bind`).
fn noop_sink() -> ValueSink {
    Arc::new(|_, _| {})
}

/// The NT4 server.
pub struct WebsocketServer {
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
    persistence_path: PathBuf,
}

impl std::fmt::Debug for WebsocketServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebsocketServer")
            .field("listener", &self.listener)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

impl WebsocketServer {
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
    /// Setting the flag alone only ends the accept loop: a connection's reader
    /// thread is blocked in `recv` with no timeout and would keep serving a
    /// stopped server, holding its port state and answering clients that think
    /// they are talking to a live one. Shutting the socket down is what ends
    /// that read.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let sockets = self.sockets.lock().unwrap_or_else(|p| p.into_inner());
        for socket in sockets.values() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
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
            )
        })
    }

    /// The file persistent topics are written to and reloaded from.
    pub fn persistence_path(&self) -> &Path {
        &self.persistence_path
    }

    /// Sets where persistent topics are saved, before [`WebsocketServer::start`].
    ///
    /// The default is relative to the working directory, which on a robot
    /// controller is wherever the program was launched from. Point this at an
    /// absolute path if the value has to outlive a redeploy.
    pub fn set_persistence_path(&mut self, path: impl Into<PathBuf>) {
        self.persistence_path = path.into();
    }

    /// Restores saved topics, then saves them again every
    /// [`PERSIST_INTERVAL`] until the server stops.
    fn start_persistence(&self) {
        load_persistent(&self.registry, &self.persistence_path);
        let registry = self.registry.clone();
        let stop = self.stop.clone();
        let path = self.persistence_path.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(PERSIST_INTERVAL);
                let _ = save_persistent(&registry, &path);
            }
            let _ = save_persistent(&registry, &path);
        });
    }

    /// Fans a value out to subscribers of `name` (Task 7 seam).
    pub fn fan_out(&self, name: &str, value: &XtValue, ts_micros: u64) {
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
    pub fn fan_out_upsert(&self, name: &str, value: &XtValue, ts_micros: u64) {
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
fn value_to_json(value: &XtValue) -> serde_json::Value {
    use serde_json::json;
    match value {
        XtValue::Bool(v) => json!(v),
        XtValue::Double(v) => json!(v),
        XtValue::Float(v) => json!(v),
        XtValue::Int8(v) => json!(v),
        XtValue::Int16(v) => json!(v),
        XtValue::Int32(v) => json!(v),
        XtValue::Int64(v) => json!(v),
        XtValue::Uint8(v) => json!(v),
        XtValue::Uint16(v) => json!(v),
        XtValue::Uint32(v) => json!(v),
        XtValue::Uint64(v) => json!(v),
        XtValue::String(v) => json!(v),
        XtValue::BoolArray(v) => json!(v),
        XtValue::DoubleArray(v) => json!(v),
        XtValue::FloatArray(v) => json!(v),
        XtValue::Int8Array(v) => json!(v),
        XtValue::Int16Array(v) => json!(v),
        XtValue::Int32Array(v) => json!(v),
        XtValue::Int64Array(v) => json!(v),
        XtValue::Uint8Array(v) => json!(v),
        XtValue::Uint16Array(v) => json!(v),
        XtValue::Uint32Array(v) => json!(v),
        XtValue::Uint64Array(v) => json!(v),
        XtValue::StringArray(v) => json!(v),
        XtValue::Bytes(v) | XtValue::BytesList(v) | XtValue::Coordinate(v) | XtValue::Bezier(v) => {
            serde_json::Value::String(data_encoding::BASE64.encode(v))
        }
    }
}

/// Rebuilds a value from its type string and the JSON [`value_to_json`] wrote.
fn value_from_json(type_str: &str, value: &serde_json::Value) -> Option<XtValue> {
    let numbers = |v: &serde_json::Value| -> Option<Vec<f64>> {
        v.as_array()?
            .iter()
            .map(serde_json::Value::as_f64)
            .collect()
    };
    match type_str {
        "boolean" => Some(XtValue::Bool(value.as_bool()?)),
        "double" => Some(XtValue::Double(value.as_f64()?)),
        "float" => Some(XtValue::Float(value.as_f64()? as f32)),
        "int" => Some(XtValue::Int64(value.as_i64()?)),
        "string" | "json" => Some(XtValue::String(value.as_str()?.to_owned())),
        "boolean[]" => Some(XtValue::BoolArray(
            value
                .as_array()?
                .iter()
                .map(serde_json::Value::as_bool)
                .collect::<Option<Vec<bool>>>()?,
        )),
        "double[]" => Some(XtValue::DoubleArray(numbers(value)?)),
        "float[]" => Some(XtValue::FloatArray(
            numbers(value)?.into_iter().map(|v| v as f32).collect(),
        )),
        "int[]" => Some(XtValue::Int64Array(
            value
                .as_array()?
                .iter()
                .map(serde_json::Value::as_i64)
                .collect::<Option<Vec<i64>>>()?,
        )),
        "string[]" => Some(XtValue::StringArray(
            value
                .as_array()?
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<String>>>()?,
        )),
        _ => Some(XtValue::Bytes(
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
/// Accepted sockets are put back into blocking mode: macOS and Windows hand
/// back a socket that inherited the listener's non-blocking flag, where Linux
/// hands back a blocking one, and the handshake read needs to block.
#[allow(clippy::too_many_arguments)]
fn accept_loop(
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
) {
    let _ = listener.set_nonblocking(true);
    let client_ids = AtomicU64::new(0);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((tcp, _)) => {
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
                );
            }
            Err(_) => thread::sleep(ACCEPT_POLL_SLEEP),
        }
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
/// polling a socket timeout the kernel rounds up to milliseconds.
#[allow(clippy::too_many_arguments)]
fn spawn_connection(
    tcp: TcpStream,
    id: ClientId,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    sockets: Arc<Mutex<HashMap<ClientId, TcpStream>>>,
    live: Arc<AtomicUsize>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
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
        // Counted like any other queued frame: the writer thread decrements once
        // per message it takes, whoever put it there.
        let Ok((mut reader, writer)) = conn.split(Box::new(move |bytes| {
            sink_queued.fetch_add(1, Ordering::AcqRel);
            if sink_tx.try_send(RouteMsg::Raw(bytes)).is_err() {
                sink_queued.fetch_sub(1, Ordering::AcqRel);
            }
        })) else {
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
    loop {
        let Ok(payload) = reader.recv() else {
            return;
        };
        let outcome = match &payload {
            Payload::Binary(bytes) => {
                route_binary(id, bytes, registry, control_handler, value_sink)
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
fn route_binary(
    id: ClientId,
    payload: &[u8],
    registry: &Arc<Mutex<NtRegistry>>,
    control_handler: &ControlHandler,
    value_sink: &ValueSink,
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
                drop(reg);
                value_sink(&name, &vm.value);
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
    let Ok(messages) = CtMessage::from_json_batch(text) else {
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
fn route_control(id: ClientId, msg: CtMessage, registry: &Arc<Mutex<NtRegistry>>) -> RouteOutcome {
    match msg {
        CtMessage::Publish {
            name,
            pubuid,
            data_type,
            properties,
        } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_publish(id, &name, pubuid, &data_type, properties))
        }
        CtMessage::Unpublish { pubuid } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_unpublish(id, pubuid))
        }
        CtMessage::Subscribe {
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
        CtMessage::SetProperties { name, update } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_setproperties(id, &name, update))
        }
        CtMessage::Unsubscribe { subuid } => {
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_unsubscribe(id, subuid))
        }
        CtMessage::Timestamp { value, .. } => {
            let server_ts = now_micros();
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_timestamp(id, json_to_xtvalue(&value), server_ts))
        }
        CtMessage::Announce { .. }
        | CtMessage::Unannounce { .. }
        | CtMessage::PropertiesUpdate { .. }
        | CtMessage::ControlValue { .. }
        | CtMessage::KeepAlive => RouteOutcome::Dispatch(Vec::new()),
    }
}

/// The current time in microseconds since the Unix epoch.
fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Converts a JSON value to an [`XtValue`] (best-effort).
fn json_to_xtvalue(v: &serde_json::Value) -> XtValue {
    match v {
        serde_json::Value::Bool(b) => XtValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                XtValue::Double(f)
            } else if let Some(i) = n.as_i64() {
                XtValue::Int64(i)
            } else {
                XtValue::Int64(0)
            }
        }
        serde_json::Value::String(s) => XtValue::String(s.clone()),
        _ => XtValue::String(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::WebsocketServer;
    use crate::value::XtValue;
    use crate::websocket::frame::RTT_SUBPROTOCOL;
    use crate::websocket::message::{RTT_TOPIC_ID, ValueMessage};

    /// The RFC 6455 example key.
    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";

    /// Sends an RFC 6455 GET and returns the server's raw response.
    fn client_handshake(stream: &mut TcpStream, path: &str) -> String {
        handshake_with(stream, path, NT4_SUBPROTOCOL)
    }

    /// Opens a handshake offering one specific subprotocol.
    fn handshake_with(stream: &mut TcpStream, path: &str, subprotocol: &str) -> String {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Protocol: {subprotocol}\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0, "server closed during handshake");
            resp.extend_from_slice(&buf[..n]);
            if resp.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(resp).unwrap()
    }

    /// Writes a masked client frame with the given opcode and payload.
    fn write_masked_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut header = vec![0x80 | opcode];
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

    /// Writes a masked binary frame.
    fn write_masked_binary(stream: &mut TcpStream, payload: &[u8]) {
        write_masked_frame(stream, 0x2, payload);
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

    /// Reads one server frame, distinguishing a clean close from a timeout.
    ///
    /// Returns `Ok(Some((opcode, payload)))` for a full frame, `Ok(None)` when
    /// the server closed the connection (EOF), and `Err` when the read timed
    /// out or otherwise failed (the connection is still open).
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

    /// Connects a client to the server and completes the handshake.
    fn connect(server: &WebsocketServer) -> TcpStream {
        let addr = server.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/nt/test");
        assert!(resp.starts_with("HTTP/1.1 101"), "handshake failed: {resp}");
        client
    }

    /// Writes a masked text frame.
    fn write_masked_text(stream: &mut TcpStream, payload: &str) {
        write_masked_frame(stream, 0x1, payload.as_bytes());
    }

    #[test]
    fn unknown_control_methods_are_ignored_not_fatal() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let batch = concat!(
            r#"[{"method":"somethingfromthefuture","params":{}},"#,
            r#"{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}]"#
        );
        write_masked_text(&mut client, batch);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(
            opcode, 0x1,
            "an unrecognized method must be skipped, not close the connection"
        );
        let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(frame[0]["method"], "announce");
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn setproperties_updates_the_topic_and_acks() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let publish = r#"[{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}]"#;
        write_masked_text(&mut client, publish);
        read_server_frame(&mut client);

        let set =
            r#"[{"method":"setproperties","params":{"name":"gyro","update":{"persistent":true}}}]"#;
        write_masked_text(&mut client, set);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1);
        let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(frame[0]["method"], "properties");
        assert_eq!(frame[0]["params"]["update"]["persistent"], true);
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn nt4_text_frame_publish_drives_announce() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let publish = r#"[{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}]"#;
        write_masked_text(&mut client, publish);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "announce must be a text frame");
        let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(frame[0]["method"], "announce");
        assert_eq!(frame[0]["params"]["name"], "gyro");
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn nt4_text_frame_batch_applies_every_message() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let batch = r#"[
            {"method":"publish","params":{"name":"a","pubuid":1,"type":"double","properties":{}}},
            {"method":"publish","params":{"name":"b","pubuid":2,"type":"double","properties":{}}}
        ]"#;
        write_masked_text(&mut client, batch);

        let mut names = Vec::new();
        for _ in 0..2 {
            let (opcode, payload) = read_server_frame(&mut client);
            assert_eq!(opcode, 0x1);
            let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            names.push(frame[0]["params"]["name"].as_str().unwrap().to_owned());
        }
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn rtt_message_with_topic_id_minus_one_is_answered() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let mut rtt = Vec::new();
        ValueMessage {
            topic_id: RTT_TOPIC_ID,
            timestamp_micros: 0,
            data_type: 2,
            value: XtValue::Int64(1234),
        }
        .encode(&mut rtt);
        assert_eq!(rtt[1], 0xff, "topic id must go out as msgpack -1");
        write_masked_binary(&mut client, &rtt);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "an rtt reply is a binary frame");
        let reply = ValueMessage::decode(&payload).unwrap();
        assert_eq!(reply.topic_id, RTT_TOPIC_ID);
        assert_eq!(reply.value.as_u64_any(), Some(1234));
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn batched_binary_frame_applies_every_value_message() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        const PUBUID: u32 = 7;
        let publish = r#"[{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}]"#;
        write_masked_text(&mut client, publish);
        let (_, announce) = read_server_frame(&mut client);
        let frame: serde_json::Value = serde_json::from_slice(&announce).unwrap();
        assert_ne!(
            frame[0]["params"]["id"].as_u64().unwrap(),
            u64::from(PUBUID),
            "the test is only meaningful when the topic id differs from the pubuid"
        );

        let subscribe =
            r#"[{"method":"subscribe","params":{"topics":["gyro"],"subuid":1,"options":{}}}]"#;
        write_masked_text(&mut client, subscribe);

        let mut batch = Vec::new();
        for v in [1.5_f64, 2.5] {
            ValueMessage {
                topic_id: PUBUID,
                timestamp_micros: 10,
                data_type: 1,
                value: XtValue::Double(v),
            }
            .encode(&mut batch);
        }
        write_masked_binary(&mut client, &batch);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2);
        let values: Vec<f64> = ValueMessage::decode_all(&payload)
            .unwrap()
            .into_iter()
            .filter_map(|m| match m.value {
                XtValue::Double(d) => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            vec![1.5, 2.5],
            "both messages in one frame must be routed"
        );

        let mut unknown = Vec::new();
        ValueMessage {
            topic_id: PUBUID + 100,
            timestamp_micros: 20,
            data_type: 1,
            value: XtValue::Double(9.5),
        }
        .encode(&mut unknown);
        write_masked_binary(&mut client, &unknown);
        write_masked_binary(&mut client, &batch);

        let (_, payload) = read_server_frame(&mut client);
        let values: Vec<f64> = ValueMessage::decode_all(&payload)
            .unwrap()
            .into_iter()
            .filter_map(|m| match m.value {
                XtValue::Double(d) => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            vec![1.5, 2.5],
            "a value on an unassigned publisher uid must be ignored, not fanned out"
        );
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn publish_round_trip_drives_announce() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let publish = r#"{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}"#;
        write_masked_binary(&mut client, publish.as_bytes());

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "announce must be a text frame");
        let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let json = &frame[0];
        assert_eq!(json["method"], "announce");
        assert_eq!(json["params"]["name"], "gyro");
        assert_eq!(json["params"]["type"], "double");
        assert_eq!(json["params"]["pubuid"], 7);

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn malformed_input_closes_connection_without_panicking() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        // Garbage: not valid msgpack, not valid JSON.
        write_masked_binary(&mut client, b"\xff\xfe\xfd\xfc not json or msgpack");

        // The server must close the connection: a WebSocket close frame or EOF.
        let _ = client.set_read_timeout(Some(Duration::from_secs(2)));
        let closed = match try_read_server_frame(&mut client) {
            Ok(Some((opcode, _))) => opcode == 0x8,
            Ok(None) => true, // EOF: the server dropped the connection.
            Err(_) => false,  // Timeout: the connection stayed open.
        };
        assert!(
            closed,
            "server must close the connection on malformed input"
        );

        // The server did not panic: a fresh client still gets a normal round-trip.
        let mut client2 = connect(&server);
        let publish = r#"{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}"#;
        write_masked_binary(&mut client2, publish.as_bytes());
        let (opcode, payload) = read_server_frame(&mut client2);
        assert_eq!(opcode, 0x1, "announce must be a text frame");
        let frame: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let json = &frame[0];
        assert_eq!(json["method"], "announce");

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn consecutive_values_arrive_exactly_once_without_a_ping() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();

        // Client A publishes a topic.
        let mut a = connect(&server);
        let publish = r#"{"method":"publish","params":{"name":"child","pubuid":1,"type":"double","properties":{}}}"#;
        write_masked_binary(&mut a, publish.as_bytes());
        let (opcode, _) = read_server_frame(&mut a);
        assert_eq!(opcode, 0x1, "publisher announce");

        // Client B subscribes and gets the announce.
        let mut b = connect(&server);
        let subscribe =
            r#"{"method":"subscribe","params":{"topics":["child"],"subuid":10,"options":{}}}"#;
        write_masked_binary(&mut b, subscribe.as_bytes());
        let (opcode, _) = read_server_frame(&mut b);
        assert_eq!(opcode, 0x1, "subscriber announce");

        for ts in 100..103 {
            server.fan_out("child", &XtValue::Double(1.0), ts);
        }
        let mut values = 0;
        let mut frames = 0;
        for attempt in 0..3 {
            let timeout = if attempt == 0 {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(300)
            };
            let _ = b.set_read_timeout(Some(timeout));
            let frame = try_read_server_frame(&mut b).unwrap();
            let Some((opcode, payload)) = frame else {
                break;
            };
            assert_eq!(opcode, 0x2, "values must arrive as binary frames");
            let mut rest = payload.as_slice();
            while !rest.is_empty() {
                let (items, consumed) = crate::websocket::msgpack::decode_array(rest).unwrap();
                assert_eq!(items.len(), 4, "each value is a 4-tuple");
                rest = &rest[consumed..];
                values += 1;
            }
            frames += 1;
            if values >= 3 {
                break;
            }
        }
        assert_eq!(
            values, 3,
            "all three values must arrive (batched across {frames} frame(s))"
        );
        assert!(
            (1..=3).contains(&frames),
            "values should arrive in 1-3 batch frames, got {frames}"
        );

        // No ping on short idleness: nothing arrives before the keepalive interval.
        let mut buf = [0u8; 8];
        let _ = b.set_read_timeout(Some(Duration::from_millis(200)));
        let n = b.read(&mut buf).unwrap_or(0);
        assert_eq!(
            n, 0,
            "no ping (or extra frame) before the keepalive interval"
        );

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn two_clients_receive_published_value_via_fan_out() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();

        // Client A publishes.
        let mut a = connect(&server);
        let publish = r#"{"method":"publish","params":{"name":"child","pubuid":1,"type":"double","properties":{}}}"#;
        write_masked_binary(&mut a, publish.as_bytes());
        let (opcode, _) = read_server_frame(&mut a);
        assert_eq!(opcode, 0x1, "publisher announce");

        // Client A subscribes; already announced as publisher, so no frame.
        let subscribe =
            r#"{"method":"subscribe","params":{"topics":["child"],"subuid":10,"options":{}}}"#;
        write_masked_binary(&mut a, subscribe.as_bytes());

        // Client B subscribes and gets the announce.
        let mut b = connect(&server);
        write_masked_binary(&mut b, subscribe.as_bytes());
        let (opcode, _) = read_server_frame(&mut b);
        assert_eq!(opcode, 0x1, "subscriber announce");

        // Server fans a value out to subscribers.
        server.fan_out("child", &XtValue::Double(1.5), 100);

        // Both clients receive the value frame.
        let (opcode_a, payload_a) = read_server_frame(&mut a);
        assert_eq!(opcode_a, 0x2, "publisher value frame");
        let (opcode_b, payload_b) = read_server_frame(&mut b);
        assert_eq!(opcode_b, 0x2, "subscriber value frame");
        assert_eq!(payload_a, payload_b);

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn stop_flag_terminates_accept_loop() {
        let server = WebsocketServer::bind_loopback().unwrap();
        let handle = server.start();
        let client = connect(&server);

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();

        // The connection is still open but the accept loop has exited.
        let _ = client;
    }

    #[test]
    fn an_rtt_connection_answers_timestamps_and_joins_no_client_list() {
        let server = WebsocketServer::bind_loopback().unwrap();
        server.start();

        let mut rtt = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        let resp = handshake_with(&mut rtt, "/nt/rtt", RTT_SUBPROTOCOL);
        assert!(resp.starts_with("HTTP/1.1 101"), "handshake failed: {resp}");
        assert!(
            resp.contains(RTT_SUBPROTOCOL),
            "the server must echo the rtt subprotocol: {resp}"
        );

        let mut ping = Vec::new();
        ValueMessage {
            topic_id: RTT_TOPIC_ID,
            timestamp_micros: 0,
            data_type: 1,
            value: XtValue::Double(1234.5),
        }
        .encode(&mut ping);
        write_masked_binary(&mut rtt, &ping);

        let (opcode, payload) = read_server_frame(&mut rtt);
        assert_eq!(opcode, 0x2, "expected a binary reply");
        let replies = ValueMessage::decode_all(&payload).unwrap();
        assert_eq!(replies.len(), 1, "one ping earns one reply");
        assert_eq!(replies[0].topic_id, RTT_TOPIC_ID);
        assert_eq!(
            replies[0].value,
            XtValue::Double(1234.5),
            "the client's own value must come back unchanged"
        );
        assert!(
            replies[0].timestamp_micros > 0,
            "the reply carries the server's time"
        );
    }
}
