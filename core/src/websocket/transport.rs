//! NT4 fan-out: [`ConnectionMap`] routes frames to bounded per-client
//! channels, and [`writer_loop`] drains one onto the socket.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::websocket::frame::{self, WebsocketWriter};
use crate::websocket::protocol::{ClientId, Outbound};

/// Per-client channel capacity. Frames past it are dropped and counted, so a
/// slow subscriber never blocks a publisher.
pub const PUB_HIGH_WATER_MARK: usize = 10_000;

/// Bytes of batched values that force a flush, per NT4 4.1's advice to keep
/// frames within the MTU.
pub const MAX_BATCH_BYTES: usize = 1400;

/// How long a connection may go without a write before a keepalive ping,
/// which NT4 4.1 requires.
pub const KEEPALIVE_INTERVAL_MS: u64 = 5_000;

/// A frame routed to one client's channel.
#[derive(Debug)]
pub enum RouteMsg {
    /// A JSON control message, sent as a WebSocket text frame.
    Text(String),
    /// A pre-encoded value message, shared across subscribers.
    Value(Arc<[u8]>),
    /// Bytes the reader produced, such as pongs, written through unchanged.
    Raw(Vec<u8>),
    /// A close frame ending the connection, with its NT4 status code.
    Close(u16, String),
}

/// Writes one connection's outbound frames until the channel closes,
/// batching consecutive values and pinging on idle.
///
/// `queued` counts what waits in `rx`, and is decremented only after a write.
pub fn writer_loop(
    writer: &Mutex<WebsocketWriter>,
    rx: &Receiver<RouteMsg>,
    queued: &AtomicUsize,
    keepalive: Duration,
) {
    loop {
        let first = match rx.recv_timeout(keepalive) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => {
                let mut writer = writer.lock().unwrap_or_else(|p| p.into_inner());
                if writer.send_ping().is_err() {
                    return;
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let mut writer = writer.lock().unwrap_or_else(|p| p.into_inner());
        let failed = write_one(&mut writer, first).is_err();
        queued.fetch_sub(1, Ordering::AcqRel);
        if failed {
            return;
        }
        while let Ok(msg) = rx.try_recv() {
            let failed = write_one(&mut writer, msg).is_err();
            queued.fetch_sub(1, Ordering::AcqRel);
            if failed {
                return;
            }
            if writer.batch_len() >= MAX_BATCH_BYTES && writer.flush().is_err() {
                return;
            }
        }
        if writer.flush().is_err() {
            return;
        }
    }
}

fn write_one(writer: &mut WebsocketWriter, msg: RouteMsg) -> Result<(), frame::Error> {
    match msg {
        RouteMsg::Value(bytes) => {
            writer.write_batched(&bytes);
            Ok(())
        }
        RouteMsg::Text(text) => {
            writer.flush()?;
            writer.send_text(&text)
        }
        RouteMsg::Raw(bytes) => writer.write_raw(&bytes),
        RouteMsg::Close(code, reason) => {
            writer.flush()?;
            writer.close(code, &reason)?;
            Err(frame::Error::Closed)
        }
    }
}

/// One client's outbound path: the writer itself, and the queue behind it.
#[derive(Clone, Debug)]
pub struct Client {
    tx: SyncSender<RouteMsg>,
    writer: Arc<Mutex<WebsocketWriter>>,
    queued: Arc<AtomicUsize>,
}

impl Client {
    /// A client whose frames are written by `writer`, queued through `tx`.
    /// `queued` must count every message put on `tx`.
    pub fn new(
        tx: SyncSender<RouteMsg>,
        writer: Arc<Mutex<WebsocketWriter>>,
        queued: Arc<AtomicUsize>,
    ) -> Self {
        Client { tx, writer, queued }
    }

    /// The queue depth this client's writer thread has yet to drain.
    pub fn queued(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.queued)
    }

    /// Writes everything routed to this client in one dispatch, in order.
    ///
    /// Values are written on the calling thread while the queue is empty, and
    /// queued from the first control message or backlog on.
    ///
    /// Returns how many frames were dropped for a full queue.
    fn deliver_all(&self, outbounds: Vec<Outbound>) -> u64 {
        let mut writer = match self.writer.try_lock() {
            Ok(writer) if self.queued.load(Ordering::Acquire) == 0 => Some(writer),
            _ => None,
        };
        let mut dropped = 0;
        let mut batched = false;
        for outbound in outbounds {
            match outbound {
                Outbound::Value(frame) => {
                    if let Some(writer) = writer.as_mut() {
                        writer.write_batched(&frame);
                        batched = true;
                        if writer.batch_len() >= MAX_BATCH_BYTES {
                            if writer.flush().is_err() {
                                return dropped;
                            }
                            batched = false;
                        }
                        continue;
                    }
                    if !self.enqueue(RouteMsg::Value(frame)) {
                        dropped += 1;
                    }
                }
                Outbound::Text(text) => {
                    if let Some(writer) = writer.as_mut()
                        && batched
                        && writer.flush().is_err()
                    {
                        return dropped;
                    }
                    batched = false;
                    writer = None;
                    if !self.enqueue(RouteMsg::Text(text)) {
                        dropped += 1;
                    }
                }
            }
        }
        if batched
            && let Some(writer) = writer.as_mut()
            && writer.flush().is_err()
        {
            return dropped;
        }
        dropped
    }

    /// Queues `msg` for the writer thread. `false` if the queue is full.
    fn enqueue(&self, msg: RouteMsg) -> bool {
        self.queued.fetch_add(1, Ordering::AcqRel);
        if self.tx.try_send(msg).is_err() {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            return false;
        }
        true
    }
}

/// Routes outbound frames to per-client writers.
#[derive(Debug)]
pub struct ConnectionMap {
    senders: HashMap<ClientId, Arc<Client>>,
    dropped: Arc<AtomicU64>,
}

impl ConnectionMap {
    /// Creates an empty map with a shared dropped-publish counter.
    pub fn new() -> Self {
        Self {
            senders: HashMap::new(),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the shared dropped-publish counter.
    pub fn dropped(&self) -> &Arc<AtomicU64> {
        &self.dropped
    }

    /// Registers `client` as the outbound path for `id`.
    pub fn add_client(&mut self, id: ClientId, client: Client) {
        self.senders.insert(id, Arc::new(client));
    }

    /// Removes `id`'s channel.
    pub fn remove_client(&mut self, id: ClientId) {
        self.senders.remove(&id);
    }

    /// Sends a close frame to one client, ending its connection.
    pub fn send_close(&self, id: ClientId, code: u16, reason: &str) {
        if let Some(client) = self.senders.get(&id) {
            client.enqueue(RouteMsg::Close(code, reason.to_owned()));
        }
    }

    /// Routes each outbound frame to its client's channel, dropping and
    /// counting frames for a full channel. Returns how many were dropped.
    pub fn dispatch(&self, routes: Vec<(ClientId, Outbound)>) -> u64 {
        let plan = self.plan(routes);
        deliver(plan, &self.dropped)
    }

    /// Resolves routes to the clients they belong to, for [`deliver`] to write
    /// after this map's lock is released.
    pub fn plan(
        &self,
        routes: Vec<(ClientId, Outbound)>,
    ) -> Vec<(ClientId, Arc<Client>, Outbound)> {
        routes
            .into_iter()
            .filter_map(|(id, outbound)| {
                self.senders
                    .get(&id)
                    .map(|client| (id, Arc::clone(client), outbound))
            })
            .collect()
    }

    /// The shared counter [`deliver`] adds dropped frames to.
    pub fn drop_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.dropped)
    }
}

/// Writes every planned frame, grouped so each client is written once, in
/// order. Returns how many frames were dropped.
pub fn deliver(plan: Vec<(ClientId, Arc<Client>, Outbound)>, dropped_total: &AtomicU64) -> u64 {
    let dropped = match plan.len() {
        0 => 0,
        1 => {
            let (_, client, outbound) = plan.into_iter().next().expect("length checked");
            client.deliver_all(vec![outbound])
        }
        _ => {
            let mut grouped: Vec<(ClientId, Arc<Client>, Vec<Outbound>)> = Vec::new();
            for (id, client, outbound) in plan {
                match grouped.iter_mut().find(|(seen, _, _)| *seen == id) {
                    Some((_, _, outbounds)) => outbounds.push(outbound),
                    None => grouped.push((id, client, vec![outbound])),
                }
            }
            grouped
                .into_iter()
                .map(|(_, client, outbounds)| client.deliver_all(outbounds))
                .sum()
        }
    };
    dropped_total.fetch_add(dropped, Ordering::Relaxed);
    dropped
}

impl Default for ConnectionMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, SyncSender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::{Client, ConnectionMap, PUB_HIGH_WATER_MARK, RouteMsg, writer_loop};
    use crate::websocket::frame::{WebsocketConnection, WebsocketWriter};
    use crate::websocket::protocol::Outbound;

    /// The RFC 6455 example key and its expected accept value.
    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";

    /// Sends an RFC 6455 GET and returns the server's raw response.
    fn client_handshake(stream: &mut TcpStream, path: &str, subprotocol: Option<&str>) -> String {
        let mut req = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {KEY}\r\n"
        );
        if let Some(sp) = subprotocol {
            req.push_str(&format!("Sec-WebSocket-Protocol: {sp}\r\n"));
        }
        req.push_str("\r\n");
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

    /// Spawns a server accepting on an ephemeral port and returns the
    /// connected pair after a successful handshake.
    fn establish_writer(path: &str) -> (WebsocketWriter, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let path = path.to_string();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
                .unwrap()
                .split(Box::new(|_| {}), Duration::ZERO, Duration::ZERO)
                .unwrap()
                .1
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, &format!("/nt/{path}"), Some(NT4_SUBPROTOCOL));
        assert!(resp.starts_with("HTTP/1.1 101"), "handshake failed: {resp}");
        (server.join().unwrap(), client)
    }

    /// A client sharing `writer`, with nothing queued behind it.
    fn client_for(tx: SyncSender<RouteMsg>, writer: &Arc<Mutex<WebsocketWriter>>) -> Client {
        Client::new(tx, Arc::clone(writer), Arc::new(AtomicUsize::new(0)))
    }

    /// Both writers are held locked, so both frames take the queue and can
    /// be inspected there.
    #[test]
    fn fan_out_shares_one_buffer_across_two_subscribers() {
        let mut map = ConnectionMap::new();
        let (w1, _s1) = establish_writer("test");
        let (w2, _s2) = establish_writer("test");
        let (w1, w2) = (Arc::new(Mutex::new(w1)), Arc::new(Mutex::new(w2)));
        let _held1 = w1.lock().unwrap();
        let _held2 = w2.lock().unwrap();
        let (tx1, rx1) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        let (tx2, rx2) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        map.add_client(1, client_for(tx1, &w1));
        map.add_client(2, client_for(tx2, &w2));

        let frame: Arc<[u8]> = Arc::from(vec![0x94, 0x01, 0x02, 0x03]);
        let routes = vec![
            (1, Outbound::Value(Arc::clone(&frame))),
            (2, Outbound::Value(frame)),
        ];
        map.dispatch(routes);

        let m1 = rx1.recv().unwrap();
        let m2 = rx2.recv().unwrap();
        match (m1, m2) {
            (RouteMsg::Value(a), RouteMsg::Value(b)) => {
                assert!(Arc::ptr_eq(&a, &b), "subscribers must share one Arc");
            }
            _ => panic!("expected Value routes"),
        }
    }

    #[test]
    fn full_channel_drops_and_counts_without_panic() {
        let mut map = ConnectionMap::new();
        let (writer, _sock) = establish_writer("test");
        let writer = Arc::new(Mutex::new(writer));
        let _held = writer.lock().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        map.add_client(1, client_for(tx, &writer));
        map.dispatch(vec![(1, Outbound::Value(Arc::from(vec![1])))]);
        let dropped = map.dispatch(vec![(1, Outbound::Value(Arc::from(vec![2])))]);
        assert_eq!(dropped, 1);
        assert_eq!(map.dropped().load(Ordering::Relaxed), 1);
        drop(rx);
    }

    #[test]
    fn the_writer_sends_every_enqueued_message_exactly_once_batched() {
        let (writer, mut client) = establish_writer("test");
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);

        let f1 = vec![0x94, 0x01];
        let f2 = vec![0x94, 0x02];
        let f3 = vec![0x94, 0x03];
        tx.send(RouteMsg::Value(Arc::from(f1.clone()))).unwrap();
        tx.send(RouteMsg::Value(Arc::from(f2.clone()))).unwrap();
        tx.send(RouteMsg::Value(Arc::from(f3.clone()))).unwrap();
        tx.send(RouteMsg::Text("{\"method\":\"announce\"}".into()))
            .unwrap();
        drop(tx);

        let writer = Arc::new(Mutex::new(writer));
        let queued = Arc::new(AtomicUsize::new(4));
        let handle =
            thread::spawn(move || writer_loop(&writer, &rx, &queued, Duration::from_secs(30)));

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected a binary frame");
        let mut expected = f1;
        expected.extend_from_slice(&f2);
        expected.extend_from_slice(&f3);
        assert_eq!(payload, expected);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "expected a text frame");
        assert_eq!(payload, b"{\"method\":\"announce\"}");
        handle.join().unwrap();
    }

    #[test]
    fn a_batch_past_the_mtu_bound_is_split_across_frames() {
        let (writer, mut client) = establish_writer("test");
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);

        let value: Arc<[u8]> = Arc::from(vec![0x94; 500]);
        for _ in 0..4 {
            tx.send(RouteMsg::Value(Arc::clone(&value))).unwrap();
        }
        drop(tx);

        let writer = Arc::new(Mutex::new(writer));
        let queued = Arc::new(AtomicUsize::new(4));
        let handle =
            thread::spawn(move || writer_loop(&writer, &rx, &queued, Duration::from_secs(30)));

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected a binary frame");
        assert!(
            payload.len() < 4 * 500,
            "2000 B of values must not ride in one frame, got {} B",
            payload.len()
        );
        let mut total = payload.len();
        while total < 4 * 500 {
            let (_, payload) = read_server_frame(&mut client);
            total += payload.len();
        }
        assert_eq!(
            total,
            4 * 500,
            "every value must still be sent exactly once"
        );
        handle.join().unwrap();
    }

    #[test]
    fn the_writer_sends_a_keepalive_ping_when_idle() {
        let (writer, mut client) = establish_writer("test");
        let (tx, rx) = mpsc::sync_channel::<RouteMsg>(PUB_HIGH_WATER_MARK);

        let writer = Arc::new(Mutex::new(writer));
        let queued = Arc::new(AtomicUsize::new(0));
        let handle =
            thread::spawn(move || writer_loop(&writer, &rx, &queued, Duration::from_millis(50)));
        let (opcode, _) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x9, "an idle writer must ping");
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn control_text_not_batched_with_preceding_value_batch() {
        let (writer, mut client) = establish_writer("test");
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);

        tx.send(RouteMsg::Value(Arc::from(vec![0x94, 0x01])))
            .unwrap();
        tx.send(RouteMsg::Value(Arc::from(vec![0x94, 0x02])))
            .unwrap();
        tx.send(RouteMsg::Text("{\"method\":\"announce\"}".into()))
            .unwrap();
        drop(tx);

        let writer = Arc::new(Mutex::new(writer));
        let queued = Arc::new(AtomicUsize::new(3));
        let handle =
            thread::spawn(move || writer_loop(&writer, &rx, &queued, Duration::from_secs(30)));

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected one binary frame for the values");
        assert_eq!(payload, vec![0x94, 0x01, 0x94, 0x02]);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "expected a separate text frame");
        assert_eq!(payload, b"{\"method\":\"announce\"}");
        handle.join().unwrap();
    }

    /// No writer thread exists here, so the frame can only reach the socket
    /// inline.
    #[test]
    fn a_value_is_written_inline_when_the_writer_is_idle() {
        let (writer, mut sock) = establish_writer("test");
        let writer = Arc::new(Mutex::new(writer));
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        let mut map = ConnectionMap::new();
        map.add_client(1, client_for(tx, &writer));

        map.dispatch(vec![(1, Outbound::Value(Arc::from(vec![0x94, 0x01])))]);

        let (opcode, payload) = read_server_frame(&mut sock);
        assert_eq!(opcode, 0x2, "expected a binary frame");
        assert_eq!(payload, vec![0x94, 0x01]);
        assert!(
            rx.try_recv().is_err(),
            "an inline write must not also queue"
        );
    }

    /// Text always queues, so the value behind it must queue too, in order.
    #[test]
    fn a_value_never_overtakes_what_is_already_queued() {
        let (writer, _sock) = establish_writer("test");
        let writer = Arc::new(Mutex::new(writer));
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        let mut map = ConnectionMap::new();
        map.add_client(1, client_for(tx, &writer));

        map.dispatch(vec![
            (1, Outbound::Text("{\"method\":\"announce\"}".into())),
            (1, Outbound::Value(Arc::from(vec![0x94, 0x01]))),
        ]);

        assert!(
            matches!(rx.try_recv(), Ok(RouteMsg::Text(_))),
            "the queued control frame must come first"
        );
        assert!(
            matches!(rx.try_recv(), Ok(RouteMsg::Value(_))),
            "the value must follow it on the queue, not jump to the socket"
        );
    }
}
