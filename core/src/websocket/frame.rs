//! The WebSocket frame layer: NT4 handshake, one payload per read, batched
//! writes.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::HeaderValue;
use tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::frame::{CloseFrame, Utf8Bytes};
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{Bytes, Message, WebSocket};

use crate::websocket::pacing::{self, Predictor};

/// The NT4 4.1 WebSocket subprotocol.
const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The NT4 4.0 WebSocket subprotocol, accepted as a fallback.
const NT4_SUBPROTOCOL_V40: &str = "networktables.first.wpi.edu";

/// The NT4 subprotocol for a timestamp-only connection, so round-trip
/// measurements do not queue behind values.
pub const RTT_SUBPROTOCOL: &str = "rtt.networktables.first.wpi.edu";

/// The largest message a peer may send. Decoding can take about 32 times the
/// bytes, and real NT4 frames are tens of kilobytes.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// The WebSocket limits every server connection runs with.
fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

/// An error from the WebSocket frame layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The WebSocket handshake was rejected.
    #[error("websocket handshake rejected: {0}")]
    Handshake(String),
    /// The peer closed the connection cleanly.
    #[error("websocket connection closed")]
    Closed,
    /// The underlying TCP stream failed.
    #[error("websocket io error")]
    Io(#[source] io::Error),
    /// The WebSocket protocol layer failed.
    #[error("websocket protocol error")]
    Protocol(#[source] tungstenite::Error),
    /// A raw frame arrived where a complete message was expected.
    #[error("unexpected raw websocket frame")]
    UnexpectedFrame,
}

/// A write half that hands whole frames to the connection's writer thread,
/// so the reader's pongs reach the socket through its single owner.
pub struct Sink {
    emit: Box<dyn Fn(Vec<u8>) + Send>,
    pending: Vec<u8>,
}

impl fmt::Debug for Sink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sink")
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl Sink {
    /// Creates a sink that hands each flushed run of bytes to `emit`.
    pub fn new(emit: Box<dyn Fn(Vec<u8>) + Send>) -> Self {
        Self {
            emit,
            pending: Vec::new(),
        }
    }
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        (self.emit)(std::mem::take(&mut self.pending));
        Ok(())
    }
}

/// A reader's stream: reads from the socket with a [`Predictor`] and an
/// optional `busy_poll` spin, and writes through the sink.
#[derive(Debug)]
pub struct ReadHalf {
    socket: TcpStream,
    sink: Sink,
    busy_poll: Duration,
    predictor: Predictor,
}

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        pacing::read_paced(&self.socket, buf, self.busy_poll, &mut self.predictor)
    }
}

impl Write for ReadHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sink.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

/// One complete NT4 message read from the socket, in tungstenite's own buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// A binary frame: one or more MessagePack value messages.
    Binary(Bytes),
    /// A text frame: a JSON array of control messages.
    Text(Utf8Bytes),
}

/// A server WebSocket connection with NT4 frame semantics.
#[derive(Debug)]
pub struct WebsocketConnection {
    socket: WebSocket<TcpStream>,
    batch: Vec<u8>,
    client_name: String,
    peer: String,
    rtt_only: bool,
}

impl WebsocketConnection {
    /// Accepts an NT4 client's WebSocket handshake on `tcp`.
    ///
    /// The path must be `/nt/<name>` and the request must offer the 4.1 or
    /// 4.0 subprotocol. Anything else is refused with HTTP 400.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Handshake`] when the request is rejected or the
    /// handshake fails, and [`Error::Io`] when the socket cannot be configured.
    pub fn accept(tcp: TcpStream) -> Result<Self, Error> {
        tcp.set_nodelay(true).map_err(Error::Io)?;
        let peer = tcp
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default();
        let mut client_name = String::new();
        let mut negotiated: Option<String> = None;
        let websocket = tungstenite::accept_hdr_with_config(
            tcp,
            #[expect(
                clippy::result_large_err,
                reason = "tungstenite's Callback trait mandates HttpResponse<Option<String>> as the error type"
            )]
            |req: &Request, mut resp: Response| {
                let subprotocol = req
                    .headers()
                    .get(SEC_WEBSOCKET_PROTOCOL)
                    .and_then(|v| v.to_str().ok())
                    .and_then(negotiate_subprotocol);
                let name = req.uri().path().strip_prefix("/nt/").map(str::to_owned);
                match (subprotocol, name) {
                    (Some(subprotocol), Some(name)) if !name.is_empty() => {
                        client_name = name;
                        negotiated = Some(subprotocol.to_owned());
                        resp.headers_mut().insert(
                            SEC_WEBSOCKET_PROTOCOL,
                            HeaderValue::from_static(subprotocol),
                        );
                        Ok(resp)
                    }
                    _ => Err(tungstenite::http::Response::builder()
                        .status(400)
                        .body(None)
                        .expect("building a 400 response is infallible")),
                }
            },
            Some(config()),
        )
        .map_err(|e| Error::Handshake(e.to_string()))?;
        Ok(Self {
            rtt_only: matches!(negotiated.as_deref(), Some(RTT_SUBPROTOCOL)),
            socket: websocket,
            batch: Vec::new(),
            client_name,
            peer,
        })
    }

    /// The peer address this connection came from, as `host:port`.
    pub fn peer(&self) -> &str {
        &self.peer
    }

    /// Whether this connection was accepted for timestamps only. Such a
    /// connection carries no topics or subscriptions.
    pub fn is_rtt_only(&self) -> bool {
        self.rtt_only
    }

    /// Splits the connection into a reader and a writer over one socket.
    ///
    /// The writer owns the socket, and the reader's own writes go to `emit`.
    /// `busy_poll` and `predict` are ignored off Unix.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the socket cannot be duplicated.
    pub fn split(
        self,
        emit: Box<dyn Fn(Vec<u8>) + Send>,
        busy_poll: Duration,
        predict: Duration,
    ) -> Result<(WebsocketReader, WebsocketWriter), Error> {
        let socket = self.socket.get_ref().try_clone().map_err(Error::Io)?;
        let (busy_poll, predict) = if pacing::supported() {
            (busy_poll, predict)
        } else {
            (Duration::ZERO, Duration::ZERO)
        };
        let reader = WebsocketReader {
            socket: WebSocket::from_raw_socket(
                ReadHalf {
                    socket: self.socket.into_inner(),
                    sink: Sink::new(emit),
                    busy_poll,
                    predictor: Predictor::new(predict),
                },
                Role::Server,
                Some(config()),
            ),
        };
        let writer = WebsocketWriter {
            socket: WebSocket::from_raw_socket(socket, Role::Server, Some(config())),
            batch: Vec::new(),
        };
        Ok((reader, writer))
    }

    /// A second handle on the socket, so a stop can shut down a blocked read.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the socket cannot be duplicated.
    pub fn try_clone_socket(&self) -> Result<TcpStream, Error> {
        self.socket.get_ref().try_clone().map_err(Error::Io)
    }

    /// The client name from the `/nt/<name>` resource this connection opened.
    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    /// Reads one complete message, answering pings on the way.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Closed`] when the peer closes cleanly, [`Error::Io`] or
    /// [`Error::Protocol`] when the read fails, and [`Error::UnexpectedFrame`]
    /// for a raw frame.
    pub fn recv(&mut self) -> Result<Payload, Error> {
        loop {
            match self.socket.read().map_err(Error::Protocol)? {
                Message::Binary(payload) => return Ok(Payload::Binary(payload)),
                Message::Text(text) => return Ok(Payload::Text(text)),
                Message::Ping(_) => self.send_pong()?,
                Message::Pong(_) => {}
                Message::Close(_) => {
                    let _ = self.socket.close(None);
                    return Err(Error::Closed);
                }
                Message::Frame(_) => return Err(Error::UnexpectedFrame),
            }
        }
    }

    /// Appends `frame` to the outgoing batch buffer.
    pub fn write_batched(&mut self, frame: &[u8]) {
        self.batch.extend_from_slice(frame);
    }

    /// Sends the batch as one binary frame and clears the buffer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] if the frame cannot be sent and
    /// [`Error::Io`] if the socket write fails.
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let frame = Bytes::copy_from_slice(&self.batch);
        self.batch.clear();
        self.socket
            .send(Message::Binary(frame))
            .map_err(Error::Protocol)?;
        self.socket.get_mut().flush().map_err(Error::Io)
    }

    fn send_pong(&mut self) -> Result<(), Error> {
        self.socket
            .send(Message::Pong(Bytes::new()))
            .map_err(Error::Protocol)
    }
}

/// Picks the preferred subprotocol the client offered: RTT, then 4.1, then
/// 4.0.
fn negotiate_subprotocol(offered: &str) -> Option<&'static str> {
    let offers: Vec<&str> = offered.split(',').map(str::trim).collect();
    if offers.contains(&RTT_SUBPROTOCOL) {
        return Some(RTT_SUBPROTOCOL);
    }
    if offers.contains(&NT4_SUBPROTOCOL) {
        return Some(NT4_SUBPROTOCOL);
    }
    if offers.contains(&NT4_SUBPROTOCOL_V40) {
        return Some(NT4_SUBPROTOCOL_V40);
    }
    None
}

/// The reading half of a split connection.
#[derive(Debug)]
pub struct WebsocketReader {
    socket: WebSocket<ReadHalf>,
}

impl WebsocketReader {
    /// How this reader's reads ended so far. See [`pacing::Tally`].
    pub fn pacing(&self) -> pacing::Tally {
        self.socket.get_ref().predictor.tally()
    }

    /// Reads one complete message. Pings are answered through the writer.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`WebsocketConnection::recv`].
    pub fn recv(&mut self) -> Result<Payload, Error> {
        loop {
            match self.socket.read().map_err(Error::Protocol)? {
                Message::Binary(payload) => return Ok(Payload::Binary(payload)),
                Message::Text(text) => return Ok(Payload::Text(text)),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Close(_) => return Err(Error::Closed),
                Message::Frame(_) => return Err(Error::UnexpectedFrame),
            }
        }
    }
}

/// The writing half of a split connection: the only owner of the socket.
#[derive(Debug)]
pub struct WebsocketWriter {
    socket: WebSocket<TcpStream>,
    batch: Vec<u8>,
}

impl WebsocketWriter {
    /// Appends `frame` to the outgoing batch buffer.
    pub fn write_batched(&mut self, frame: &[u8]) {
        self.batch.extend_from_slice(frame);
    }

    /// Bytes waiting in the batch buffer.
    pub fn batch_len(&self) -> usize {
        self.batch.len()
    }

    /// Sends the batch as one binary frame and clears the buffer, keeping its
    /// capacity.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] or [`Error::Io`] when the write fails.
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let frame = Bytes::copy_from_slice(&self.batch);
        self.batch.clear();
        self.socket
            .send(Message::Binary(frame))
            .map_err(Error::Protocol)?;
        self.socket.get_mut().flush().map_err(Error::Io)
    }

    /// Sends `text` as one text frame and flushes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] or [`Error::Io`] on write failure.
    pub fn send_text(&mut self, text: &str) -> Result<(), Error> {
        self.socket
            .send(Message::Text(text.into()))
            .map_err(Error::Protocol)?;
        self.socket.get_mut().flush().map_err(Error::Io)
    }

    /// Sends an empty ping frame.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] if the frame cannot be sent.
    pub fn send_ping(&mut self) -> Result<(), Error> {
        self.socket
            .send(Message::Ping(Bytes::new()))
            .map_err(Error::Protocol)
    }

    /// Writes bytes the reader produced, keeping them whole and in order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the socket write fails.
    pub fn write_raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.flush()?;
        self.socket.get_mut().write_all(bytes).map_err(Error::Io)?;
        self.socket.get_mut().flush().map_err(Error::Io)
    }

    /// Sends a close frame with the given code and reason.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Protocol`] or [`Error::Io`] on write failure.
    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), Error> {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: Utf8Bytes::from(reason),
        };
        self.socket
            .send(Message::Close(Some(frame)))
            .map_err(Error::Protocol)?;
        self.socket.get_mut().flush().map_err(Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    use super::{Error, NT4_SUBPROTOCOL, NT4_SUBPROTOCOL_V40, Payload, WebsocketConnection};

    /// The RFC 6455 example key and its expected accept value.
    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

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

    /// Spawns a server accepting on an ephemeral port and returns the
    /// connected pair after a successful handshake.
    fn establish_connection(path: &str) -> (WebsocketConnection, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let path = path.to_string();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp).unwrap()
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, &format!("/nt/{path}"), Some(NT4_SUBPROTOCOL));
        assert!(resp.starts_with("HTTP/1.1 101"), "handshake failed: {resp}");
        (server.join().unwrap(), client)
    }

    #[test]
    fn accept_ok_on_valid_nt4_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/nt/test", Some(NT4_SUBPROTOCOL));
        assert!(
            resp.starts_with("HTTP/1.1 101"),
            "expected 101, got: {resp}"
        );
        let lower = resp.to_ascii_lowercase();
        assert!(
            lower.contains(&format!(
                "sec-websocket-accept: {}",
                ACCEPT.to_ascii_lowercase()
            )),
            "wrong accept key in: {resp}"
        );
        assert!(
            lower.contains(&format!("sec-websocket-protocol: {NT4_SUBPROTOCOL}")),
            "subprotocol not echoed in: {resp}"
        );
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn accept_keeps_the_client_name_from_the_resource() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/nt/AdvantageScope", Some(NT4_SUBPROTOCOL));
        assert!(
            resp.starts_with("HTTP/1.1 101"),
            "the client picks its own name, so any /nt/<name> must be accepted: {resp}"
        );
        assert_eq!(
            server.join().unwrap().unwrap().client_name(),
            "AdvantageScope"
        );
    }

    #[test]
    fn accept_rejects_a_resource_outside_nt() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/other", Some(NT4_SUBPROTOCOL));
        assert!(
            resp.starts_with("HTTP/1.1 400"),
            "expected 400, got: {resp}"
        );
        assert!(matches!(server.join().unwrap(), Err(Error::Handshake(_))));
    }

    #[test]
    fn accept_falls_back_to_the_v40_subprotocol() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/nt/x", Some(NT4_SUBPROTOCOL_V40));
        assert!(
            resp.starts_with("HTTP/1.1 101"),
            "expected 101, got: {resp}"
        );
        assert!(
            resp.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {NT4_SUBPROTOCOL_V40}")),
            "the matched subprotocol must be echoed: {resp}"
        );
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn accept_rejects_missing_subprotocol() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            WebsocketConnection::accept(tcp)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let resp = client_handshake(&mut client, "/nt/test", None);
        assert!(
            resp.starts_with("HTTP/1.1 400"),
            "expected 400, got: {resp}"
        );
        assert!(matches!(server.join().unwrap(), Err(Error::Handshake(_))));
    }

    #[test]
    fn recv_binary_round_trips_masked_frame() {
        let (mut conn, mut client) = establish_connection("test");
        let payload = vec![0x94, 0x01, 0x02, 0x03, 0x04, 0x05];
        write_masked_binary(&mut client, &payload);
        assert_eq!(conn.recv().unwrap(), Payload::Binary(payload.into()));
    }

    #[test]
    fn recv_binary_answers_ping_with_pong() {
        let (mut conn, mut client) = establish_connection("test");
        write_masked_frame(&mut client, 0x9, &[]);
        write_masked_binary(&mut client, &[7, 8]);
        assert_eq!(conn.recv().unwrap(), Payload::Binary(vec![7, 8].into()));
        let (opcode, _) = read_server_frame(&mut client);
        assert_eq!(opcode, 0xA, "expected a pong frame");
    }

    #[test]
    fn a_busy_polling_reader_receives_a_frame_that_lands_inside_the_window() {
        let (conn, mut client) = establish_connection("test");
        let (mut reader, _writer) = conn
            .split(Box::new(|_| {}), Duration::from_millis(200), Duration::ZERO)
            .unwrap();
        let payload = vec![0x94, 0x01, 0x02];
        let sender = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            write_masked_binary(&mut client, &payload);
            client
        });
        assert_eq!(
            reader.recv().unwrap(),
            Payload::Binary(vec![0x94, 0x01, 0x02].into())
        );
        drop(sender.join().unwrap());
    }

    #[test]
    fn a_busy_polling_reader_still_blocks_once_the_window_lapses() {
        let (conn, mut client) = establish_connection("test");
        let (mut reader, _writer) = conn
            .split(Box::new(|_| {}), Duration::from_millis(5), Duration::ZERO)
            .unwrap();
        let sender = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            write_masked_binary(&mut client, &[7, 8]);
            client
        });
        let started = std::time::Instant::now();
        assert_eq!(reader.recv().unwrap(), Payload::Binary(vec![7, 8].into()));
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "the read returned before the frame could have been written"
        );
        drop(sender.join().unwrap());
    }

    #[test]
    fn write_batched_flush_sends_one_binary_message() {
        let (mut conn, mut client) = establish_connection("test");
        conn.write_batched(&[1, 2, 3]);
        conn.write_batched(&[4, 5]);
        conn.flush().unwrap();
        let (opcode, payload) = read_server_frame(&mut client);
        assert_eq!(opcode, 0x2, "expected a binary frame");
        assert_eq!(payload, vec![1, 2, 3, 4, 5]);
    }
}
