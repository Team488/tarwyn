//! The NT4 WebSocket server accept loop and connection wiring.
//!
//! [`WsServer`] binds a [`TcpListener`], accepts NT4 clients over `/nt/<path>`,
//! and runs one reader thread per connection. Inbound binary payloads are
//! decoded and routed to the shared [`NtRegistry`]; the returned fan-out routes
//! are dispatched through a shared [`ConnectionMap`] to per-client writer
//! threads. The registry and connection map are each behind their own
//! [`Mutex`]; the two locks are never held together, and every acquire recovers
//! from poisoning via [`Mutex::into_inner`].

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::websocket::frame::{FrameError, Payload, WsConnection};
use crate::websocket::message::{CtMessage, RTT_TOPIC_ID, ValueMessage, XtValue};
use crate::websocket::protocol::{ClientId, NtRegistry, Outbound};
use crate::websocket::transport::{
    ConnectionMap, KEEPALIVE_INTERVAL_MS, PUB_HIGH_WATER_MARK, RouteMsg,
};

/// How many times a port is tried before the bind is reported as failed.
const BIND_ATTEMPTS: u32 = 5;
/// How long to wait between bind attempts.
const BIND_RETRY: Duration = Duration::from_millis(200);
/// How long the reader waits for inbound data before draining the outbound
/// channel. This sits on the data path, so a short timeout keeps fan-out
/// latency low; at 50 µs the p50 is ≈ 28 µs on loopback. Idle cost is
/// ~20 K wakeups/s per connection.
const READ_TIMEOUT: Duration = Duration::from_micros(50);
/// How long the nonblocking accept loop sleeps between polls for a new
/// connection. This is not on the data path — it only paces idle retries, so
/// it stays lazy (100 ms) to avoid busy-waiting when no client is connecting.
const ACCEPT_POLL_SLEEP: Duration = Duration::from_millis(100);
/// The NT4 table path served by this server.
const TABLE_PATH: &str = "test";

/// A callback that answers a control-plane request (Task 7 seam).
///
/// The TARWYN control plane (get/delete/tables/ping/stats/json/CAS/logs) rides
/// the WS connection as binary protobuf `Request`/`Reply` frames. The WS layer
/// stays protobuf-free: it hands the raw inbound bytes to this callback and
/// writes whatever bytes it returns back to the same connection. `None` means
/// the bytes were not a valid control request, and the connection is closed.
pub type ControlHandler = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// A callback that stores a WS-originated value into the server's read cache.
///
/// A value that arrives over WS has already been fanned out to NT4 subscribers
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

/// The NT4 WebSocket server.
pub struct WsServer {
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
}

impl std::fmt::Debug for WsServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsServer")
            .field("listener", &self.listener)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

impl WsServer {
    /// Binds the server to `port`, retrying up to [`BIND_ATTEMPTS`] times.
    ///
    /// # Errors
    ///
    /// Returns the last [`io::Error`] if the port cannot be bound after all
    /// attempts.
    pub fn bind(port: u16) -> io::Result<Self> {
        Self::bind_with_handler(port, noop_handler(), noop_sink())
    }

    /// Binds to an OS-assigned loopback port (for tests).
    pub fn bind_loopback() -> io::Result<Self> {
        Self::bind_loopback_with_handler(noop_handler(), noop_sink())
    }

    /// Binds the server to `port` with a control-plane handler and value sink.
    ///
    /// `control_handler` answers binary protobuf control requests; `value_sink`
    /// stores WS-originated values into the server's read cache. See the type
    /// aliases for the exact contracts.
    pub fn bind_with_handler(
        port: u16,
        control_handler: ControlHandler,
        value_sink: ValueSink,
    ) -> io::Result<Self> {
        let addr = format!("127.0.0.1:{port}");
        let mut last_err = None;
        for _ in 0..BIND_ATTEMPTS {
            match TcpListener::bind(&addr) {
                Ok(listener) => {
                    return Ok(Self {
                        listener,
                        registry: Arc::new(Mutex::new(NtRegistry::new())),
                        conns: Arc::new(Mutex::new(ConnectionMap::new())),
                        stop: Arc::new(AtomicBool::new(false)),
                        control_handler,
                        value_sink,
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
            stop: Arc::new(AtomicBool::new(false)),
            control_handler,
            value_sink,
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

    /// Starts the accept loop, returning its thread handle.
    pub fn start(&self) -> JoinHandle<()> {
        let registry = self.registry.clone();
        let conns = self.conns.clone();
        let stop = self.stop.clone();
        let control_handler = self.control_handler.clone();
        let value_sink = self.value_sink.clone();
        let listener = self
            .listener
            .try_clone()
            .expect("cloning a bound listener is infallible");
        thread::spawn(move || {
            accept_loop(listener, registry, conns, stop, control_handler, value_sink)
        })
    }

    /// Fans a value out to subscribers of `name` (Task 7 seam).
    pub fn fan_out(&self, name: &str, value: &XtValue, ts_micros: u64) {
        let routes = {
            let mut reg = self.registry.lock().unwrap_or_else(|p| p.into_inner());
            let Some(id) = reg.topic_id(name) else {
                return;
            };
            reg.handle_value(0, id, value.clone(), ts_micros)
        };
        let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        map.dispatch(routes);
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
        let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        map.dispatch(routes);
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

/// Runs the accept loop until `stop` is set.
fn accept_loop(
    listener: TcpListener,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    stop: Arc<AtomicBool>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
) {
    let _ = listener.set_nonblocking(true);
    let client_ids = AtomicU64::new(0);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((tcp, _)) => {
                let id = client_ids.fetch_add(1, Ordering::Relaxed);
                spawn_connection(
                    tcp,
                    id,
                    registry.clone(),
                    conns.clone(),
                    control_handler.clone(),
                    value_sink.clone(),
                );
            }
            Err(_) => thread::sleep(ACCEPT_POLL_SLEEP),
        }
    }
}

/// Spawns one thread that owns a freshly accepted connection.
///
/// The thread both reads inbound payloads and writes outbound frames. A single
/// tungstenite [`WebSocket`] needs `&mut self` for read and send and cannot be
/// split, so a separate writer thread would require a `Mutex` that the reader
/// holds while blocked on `recv_binary` — starving the writer. Owning the
/// connection in one thread and draining the outbound channel on a read
/// timeout avoids that entirely.
fn spawn_connection(
    tcp: TcpStream,
    id: ClientId,
    registry: Arc<Mutex<NtRegistry>>,
    conns: Arc<Mutex<ConnectionMap>>,
    control_handler: ControlHandler,
    value_sink: ValueSink,
) {
    thread::spawn(move || {
        let Ok(mut conn) = WsConnection::accept(tcp, TABLE_PATH) else {
            return;
        };
        // A short read timeout lets the loop drain its outbound channel and
        // send keepalive pings while no inbound data is arriving.
        let _ = conn.set_read_timeout(READ_TIMEOUT);
        let (tx, rx) = sync_channel(PUB_HIGH_WATER_MARK);
        {
            let mut map = conns.lock().unwrap_or_else(|p| p.into_inner());
            map.add_client(id, tx);
        }
        let mut last_write = std::time::Instant::now();
        loop {
            match conn.recv() {
                Ok(payload) => {
                    let outcome = match &payload {
                        Payload::Binary(bytes) => {
                            route_binary(id, bytes, &registry, &control_handler, &value_sink)
                        }
                        Payload::Text(text) => route_text(id, text, &registry),
                    };
                    match outcome {
                        RouteOutcome::Dispatch(routes) => {
                            if !routes.is_empty() {
                                let map = conns.lock().unwrap_or_else(|p| p.into_inner());
                                map.dispatch(routes);
                            }
                        }
                        RouteOutcome::ControlReply(reply) => {
                            conn.write_batched(&Arc::from(reply));
                            let _ = conn.flush();
                        }
                        RouteOutcome::Close => {
                            let _ = conn.close(1002, "malformed payload");
                            break;
                        }
                    }
                    drain_channel(&mut conn, &rx, &mut last_write);
                }
                Err(FrameError::Closed) => break,
                Err(FrameError::Protocol(e)) if is_timeout(&e) => {
                    drain_channel(&mut conn, &rx, &mut last_write);
                    if last_write.elapsed() >= Duration::from_millis(KEEPALIVE_INTERVAL_MS) {
                        let _ = conn.send_ping();
                        last_write = std::time::Instant::now();
                    }
                }
                Err(_) => break,
            }
        }
        conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove_client(id);
    });
}

/// Writes every queued outbound frame to `conn`, batching consecutive values.
fn drain_channel(
    conn: &mut WsConnection,
    rx: &Receiver<RouteMsg>,
    last_write: &mut std::time::Instant,
) {
    loop {
        match rx.try_recv() {
            Ok(RouteMsg::Text(s)) => {
                if conn.flush().is_err() {
                    return;
                }
                if conn.send_text(&s).is_err() {
                    return;
                }
                *last_write = std::time::Instant::now();
            }
            Ok(RouteMsg::Value(arc)) => {
                conn.write_batched(&arc);
                *last_write = std::time::Instant::now();
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                let _ = conn.flush();
                return;
            }
        }
    }
}

/// Whether a tungstenite error is a read timeout (WouldBlock/TimedOut).
fn is_timeout(e: &tungstenite::Error) -> bool {
    matches!(
        e,
        tungstenite::Error::Io(io_err) if matches!(
            io_err.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        )
    )
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
            routes.extend(reg.handle_value(id, vm.topic_id, vm.value.clone(), vm.timestamp_micros));
            if let Some(name) = reg.topic_name(vm.topic_id) {
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
/// connection open; an invalid data-type string closes it.
fn route_control(id: ClientId, msg: CtMessage, registry: &Arc<Mutex<NtRegistry>>) -> RouteOutcome {
    match msg {
        CtMessage::Publish {
            name,
            pubuid,
            data_type,
            properties,
        } => {
            let Some(dt) = crate::websocket::protocol::data_type_from_string(&data_type) else {
                return RouteOutcome::Close;
            };
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_publish(id, &name, pubuid, dt, properties))
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
            let prefix = options
                .get("prefix")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut reg = registry.lock().unwrap_or_else(|p| p.into_inner());
            RouteOutcome::Dispatch(reg.handle_subscribe(id, &topics, subuid, prefix))
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

    use super::WsServer;
    use crate::websocket::message::{RTT_TOPIC_ID, ValueMessage, XtValue};

    /// The RFC 6455 example key.
    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";

    /// Sends an RFC 6455 GET and returns the server's raw response.
    fn client_handshake(stream: &mut TcpStream, path: &str) -> String {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Protocol: {NT4_SUBPROTOCOL}\r\n\r\n"
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
    fn connect(server: &WsServer) -> TcpStream {
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
    fn nt4_text_frame_publish_drives_announce() {
        let server = WsServer::bind_loopback().unwrap();
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
        let server = WsServer::bind_loopback().unwrap();
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
        let server = WsServer::bind_loopback().unwrap();
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
        let server = WsServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        let publish = r#"[{"method":"publish","params":{"name":"gyro","pubuid":7,"type":"double","properties":{}}}]"#;
        write_masked_text(&mut client, publish);
        let (_, announce) = read_server_frame(&mut client);
        let frame: serde_json::Value = serde_json::from_slice(&announce).unwrap();
        let topic_id = frame[0]["params"]["id"].as_u64().unwrap() as u32;

        let subscribe =
            r#"[{"method":"subscribe","params":{"topics":["gyro"],"subuid":1,"options":{}}}]"#;
        write_masked_text(&mut client, subscribe);

        let mut batch = Vec::new();
        for v in [1.5_f64, 2.5] {
            ValueMessage {
                topic_id,
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
        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn publish_round_trip_drives_announce() {
        let server = WsServer::bind_loopback().unwrap();
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
        let server = WsServer::bind_loopback().unwrap();
        let handle = server.start();
        let mut client = connect(&server);

        // Garbage: not valid msgpack, not valid JSON.
        write_masked_binary(&mut client, b"\xff\xfe\xfd\xfc not json or msgpack");

        // The server must close the connection: a WS close frame or EOF.
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
    fn consecutive_values_batch_into_one_frame_without_ping() {
        let server = WsServer::bind_loopback().unwrap();
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

        // Three values enqueued back-to-back coalesce into exactly one frame.
        for ts in 100..103 {
            server.fan_out("child", &XtValue::Double(1.0), ts);
        }
        let _ = b.set_read_timeout(Some(Duration::from_secs(2)));
        let (opcode, payload) = read_server_frame(&mut b);
        assert_eq!(opcode, 0x2, "values must arrive as one binary frame");
        let mut rest = payload.as_slice();
        let mut values = 0;
        while !rest.is_empty() {
            let (items, consumed) = crate::websocket::msgpack::decode_array(rest).unwrap();
            assert_eq!(items.len(), 4, "each value is a 4-tuple");
            rest = &rest[consumed..];
            values += 1;
        }
        assert_eq!(values, 3, "one frame must carry all three values");

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
        let server = WsServer::bind_loopback().unwrap();
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
        let server = WsServer::bind_loopback().unwrap();
        let handle = server.start();
        let client = connect(&server);

        server.stop_flag().store(true, Ordering::Relaxed);
        handle.join().unwrap();

        // The connection is still open but the accept loop has exited.
        let _ = client;
    }
}
