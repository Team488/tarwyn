//! NT4 connection fan-out and per-client writer.
//!
//! [`ConnectionMap`] routes [`Outbound`] frames from the registry to bounded
//! per-client channels; [`drain_channel`] empties one channel onto a
//! [`WsConnection`]. Value frames are shared across subscribers via [`Arc`]
//! (one allocation, N channel sends); draining coalesces consecutive values
//! into a single binary frame and sends control text as its own frame.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::Instant;

use crate::websocket::frame::WsConnection;
use crate::websocket::protocol::{ClientId, Outbound};

// Rust guideline compliant 2026-02-21

/// Per-client channel capacity, mirroring the ZMQ `PUB_HIGH_WATER_MARK`.
///
/// A subscriber that falls this far behind is a slow consumer: further frames
/// are dropped and counted rather than blocking the publisher.
pub const PUB_HIGH_WATER_MARK: usize = 10_000;

/// How long a connection may go without a write before a keepalive ping.
///
/// NT4 4.1 mandates periodic WebSocket pings.
pub const KEEPALIVE_INTERVAL_MS: u64 = 5_000;

/// A frame routed to one client's channel.
#[derive(Debug)]
pub enum RouteMsg {
    /// A JSON control message, sent as a WS text frame.
    Text(String),
    /// A pre-encoded value message, shared across subscribers.
    Value(Arc<[u8]>),
}

/// Writes every queued outbound frame to `conn`, batching consecutive values.
///
/// Consecutive [`RouteMsg::Value`] frames are concatenated into one binary
/// WebSocket frame; a [`RouteMsg::Text`] control message flushes that batch
/// first so ordering is preserved, then goes out as its own text frame.
/// `last_write` is stamped on every write so the caller can time keepalives.
pub fn drain_channel(conn: &mut WsConnection, rx: &Receiver<RouteMsg>, last_write: &mut Instant) {
    loop {
        match rx.try_recv() {
            Ok(RouteMsg::Text(s)) => {
                if conn.flush().is_err() {
                    return;
                }
                if conn.send_text(&s).is_err() {
                    return;
                }
                *last_write = Instant::now();
            }
            Ok(RouteMsg::Value(arc)) => {
                conn.write_batched(&arc);
                *last_write = Instant::now();
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                let _ = conn.flush();
                return;
            }
        }
    }
}

/// Routes outbound frames to per-client channels.
#[derive(Debug)]
pub struct ConnectionMap {
    senders: HashMap<ClientId, SyncSender<RouteMsg>>,
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

    /// Registers `tx` as the channel for `id`.
    pub fn add_client(&mut self, id: ClientId, tx: SyncSender<RouteMsg>) {
        self.senders.insert(id, tx);
    }

    /// Removes `id`'s channel.
    pub fn remove_client(&mut self, id: ClientId) {
        self.senders.remove(&id);
    }

    /// Routes each outbound frame to its target client's channel.
    ///
    /// A value frame arrives already wrapped in one [`Arc`], so each target
    /// costs a refcount bump rather than a copy; a full channel drops the
    /// frame and increments the dropped counter.
    pub fn dispatch(&self, routes: Vec<(ClientId, Outbound)>) -> u64 {
        let mut dropped = 0;
        for (id, outbound) in routes {
            let Some(tx) = self.senders.get(&id) else {
                continue;
            };
            let msg = match outbound {
                Outbound::Text(s) => RouteMsg::Text(s),
                Outbound::Value(frame) => RouteMsg::Value(frame),
            };
            if tx.try_send(msg).is_err() {
                dropped += 1;
            }
        }
        self.dropped.fetch_add(dropped, Ordering::Relaxed);
        dropped
    }
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
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    use super::{ConnectionMap, PUB_HIGH_WATER_MARK, RouteMsg, drain_channel};
    use crate::websocket::frame::WsConnection;
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
    fn establish_connection(path: &str) -> (WsConnection, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let path = path.to_string();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WsConnection::accept(tcp).unwrap()
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, &format!("/nt/{path}"), Some(NT4_SUBPROTOCOL));
        assert!(resp.starts_with("HTTP/1.1 101"), "handshake failed: {resp}");
        (server.join().unwrap(), client)
    }

    #[test]
    fn fan_out_shares_one_buffer_across_two_subscribers() {
        let mut map = ConnectionMap::new();
        let (tx1, rx1) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        let (tx2, rx2) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);
        map.add_client(1, tx1);
        map.add_client(2, tx2);

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
        let (tx, rx) = mpsc::sync_channel(1);
        map.add_client(1, tx);
        map.dispatch(vec![(1, Outbound::Value(Arc::from(vec![1])))]);
        let dropped = map.dispatch(vec![(1, Outbound::Value(Arc::from(vec![2])))]);
        assert_eq!(dropped, 1);
        assert_eq!(map.dropped().load(Ordering::Relaxed), 1);
        drop(rx);
    }

    #[test]
    fn draining_writes_every_enqueued_message_exactly_once_batched() {
        let (mut conn, mut client) = establish_connection("test");
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);

        let f1 = vec![0x94, 0x01];
        let f2 = vec![0x94, 0x02];
        let f3 = vec![0x94, 0x03];
        tx.send(RouteMsg::Value(Arc::from(f1.clone()))).unwrap();
        tx.send(RouteMsg::Value(Arc::from(f2.clone()))).unwrap();
        tx.send(RouteMsg::Value(Arc::from(f3.clone()))).unwrap();
        tx.send(RouteMsg::Text("{\"method\":\"announce\"}".into()))
            .unwrap();

        let mut last_write = Instant::now();
        drain_channel(&mut conn, &rx, &mut last_write);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected a binary frame");
        let mut expected = f1;
        expected.extend_from_slice(&f2);
        expected.extend_from_slice(&f3);
        assert_eq!(payload, expected);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "expected a text frame");
        assert_eq!(payload, b"{\"method\":\"announce\"}");
    }

    #[test]
    fn send_ping_emits_a_ping_frame() {
        let (mut conn, mut client) = establish_connection("test");

        conn.send_ping().unwrap();
        conn.flush().unwrap();

        let (opcode, _) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x9, "expected a ping frame");
    }

    #[test]
    fn control_text_not_batched_with_preceding_value_batch() {
        let (mut conn, mut client) = establish_connection("test");
        let (tx, rx) = mpsc::sync_channel(PUB_HIGH_WATER_MARK);

        tx.send(RouteMsg::Value(Arc::from(vec![0x94, 0x01])))
            .unwrap();
        tx.send(RouteMsg::Value(Arc::from(vec![0x94, 0x02])))
            .unwrap();
        tx.send(RouteMsg::Text("{\"method\":\"announce\"}".into()))
            .unwrap();

        let mut last_write = Instant::now();
        drain_channel(&mut conn, &rx, &mut last_write);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected one binary frame for the values");
        assert_eq!(payload, vec![0x94, 0x01, 0x94, 0x02]);

        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x1, "expected a separate text frame");
        assert_eq!(payload, b"{\"method\":\"announce\"}");
    }
}
