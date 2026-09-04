//! The WebSocket frame I/O wrapper.
//!
//! [`WebsocketConnection`] owns a tungstenite server [`WebSocket`] over a blocking
//! [`TcpStream`] and exposes the frame-level interface NT4 needs: a
//! subprotocol-checked handshake, one complete binary payload per read,
//! batched writes that become a single WebSocket frame, and ping/close
//! plumbing for the keepalive loop.

// Rust guideline compliant 2026-02-21

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::HeaderValue;
use tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tungstenite::protocol::Role;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::frame::{CloseFrame, Utf8Bytes};
use tungstenite::{Message, WebSocket};

/// The NT4 4.1 WebSocket subprotocol.
const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The NT4 4.0 WebSocket subprotocol, accepted as a fallback.
const NT4_SUBPROTOCOL_V40: &str = "networktables.first.wpi.edu";

/// An error from the WebSocket frame layer.
#[derive(Debug)]
pub enum FrameError {
    /// The WebSocket handshake was rejected.
    Handshake(String),
    /// The peer closed the connection cleanly.
    Closed,
    /// The underlying TCP stream failed.
    Io(io::Error),
    /// The WebSocket protocol layer failed.
    Protocol(tungstenite::Error),
    /// A raw frame arrived where a complete message was expected.
    UnexpectedFrame,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Handshake(msg) => write!(f, "websocket handshake rejected: {msg}"),
            FrameError::Closed => f.write_str("websocket connection closed"),
            FrameError::Io(e) => write!(f, "websocket io error: {e}"),
            FrameError::Protocol(e) => write!(f, "websocket protocol error: {e}"),
            FrameError::UnexpectedFrame => f.write_str("unexpected raw websocket frame"),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FrameError::Io(e) => Some(e),
            FrameError::Protocol(e) => Some(e),
            _ => None,
        }
    }
}

/// A write half that hands whole frames to the connection's writer thread.
///
/// tungstenite answers pings and closes from inside `read`, so a connection
/// cannot be split into an independent reader and writer. Giving the reader a
/// sink that forwards its bytes to the writer thread keeps a single owner of
/// the socket while letting both halves block on their own events.
pub struct FrameSink {
    emit: Box<dyn Fn(Vec<u8>) + Send>,
    pending: Vec<u8>,
}

impl fmt::Debug for FrameSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameSink")
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl FrameSink {
    /// Creates a sink that hands each flushed run of bytes to `emit`.
    pub fn new(emit: Box<dyn Fn(Vec<u8>) + Send>) -> Self {
        Self {
            emit,
            pending: Vec::new(),
        }
    }
}

impl Write for FrameSink {
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

/// A reader's stream: reads from the socket, writes through the sink.
#[derive(Debug)]
pub struct ReadHalf {
    socket: TcpStream,
    sink: FrameSink,
}

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buf)
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

/// One complete NT4 message read from the socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// A binary frame: one or more MessagePack value messages.
    Binary(Vec<u8>),
    /// A text frame: a JSON array of control messages.
    Text(String),
}

/// A server WebSocket connection with NT4 frame semantics.
#[derive(Debug)]
pub struct WebsocketConnection {
    socket: WebSocket<TcpStream>,
    batch: Vec<u8>,
    client_name: String,
    peer: String,
}

impl WebsocketConnection {
    /// Accepts an NT4 client's WebSocket handshake on `tcp`.
    ///
    /// Sets TCP_NODELAY, then runs the RFC 6455 server handshake. Per NT4
    /// §"WebSocket Interface" the resource name is `/nt/<name>`, where the
    /// client picks `<name>`; any name is accepted and kept as
    /// [`WebsocketConnection::client_name`]. The request must offer the 4.1 or the
    /// 4.0 subprotocol, and the matched one is echoed back. Anything else is
    /// rejected with HTTP 400.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Handshake`] when the request is rejected or the
    /// handshake fails, and [`FrameError::Io`] when the socket cannot be
    /// configured.
    pub fn accept(tcp: TcpStream) -> Result<Self, FrameError> {
        tcp.set_nodelay(true).map_err(FrameError::Io)?;
        let peer = tcp
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default();
        let mut client_name = String::new();
        let websocket = tungstenite::accept_hdr(
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
        )
        .map_err(|e| FrameError::Handshake(e.to_string()))?;
        Ok(Self {
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

    /// Splits the connection into a reader and a writer over one socket.
    ///
    /// The reader keeps the handshaked [`WebSocket`], with its writes routed
    /// through `tx` so the pongs and closes tungstenite emits from inside
    /// `read` reach the socket in frame order. The returned [`WebsocketWriter`] owns
    /// the only real handle to the socket. Both halves then block on their own
    /// event, inbound bytes or an outbound frame, with no polling.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Io`] if the socket cannot be duplicated.
    pub fn split(
        self,
        emit: Box<dyn Fn(Vec<u8>) + Send>,
    ) -> Result<(WebsocketReader, WebsocketWriter), FrameError> {
        let socket = self.socket.get_ref().try_clone().map_err(FrameError::Io)?;
        let reader = WebsocketReader {
            socket: WebSocket::from_raw_socket(
                ReadHalf {
                    socket: self.socket.into_inner(),
                    sink: FrameSink::new(emit),
                },
                Role::Server,
                None,
            ),
        };
        let writer = WebsocketWriter {
            socket: WebSocket::from_raw_socket(socket, Role::Server, None),
            batch: Vec::new(),
        };
        Ok((reader, writer))
    }

    /// The client name from the `/nt/<name>` resource this connection opened.
    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    /// Reads one complete message from the peer.
    ///
    /// Loops over frames: pings are answered with a pong, pongs are ignored,
    /// and a close frame closes the connection and returns
    /// [`FrameError::Closed`]. NT4 carries control messages as text (JSON) and
    /// value messages as binary (MessagePack), and forbids a message from
    /// spanning frames, so one frame is one complete payload.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Closed`] on a clean close, [`FrameError::Io`] on
    /// a TCP failure, [`FrameError::Protocol`] on a protocol failure, and
    /// [`FrameError::UnexpectedFrame`] on a raw frame.
    pub fn recv(&mut self) -> Result<Payload, FrameError> {
        loop {
            match self.socket.read().map_err(FrameError::Protocol)? {
                Message::Binary(payload) => return Ok(Payload::Binary(payload.to_vec())),
                Message::Text(text) => return Ok(Payload::Text(text.to_string())),
                Message::Ping(_) => self.send_pong()?,
                Message::Pong(_) => {}
                Message::Close(_) => {
                    let _ = self.socket.close(None);
                    return Err(FrameError::Closed);
                }
                Message::Frame(_) => return Err(FrameError::UnexpectedFrame),
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
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent and
    /// [`FrameError::Io`] if the underlying socket write fails.
    pub fn flush(&mut self) -> Result<(), FrameError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        self.socket
            .send(Message::Binary(std::mem::take(&mut self.batch).into()))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)?;
        Ok(())
    }

    /// Sends `text` as one text frame and flushes.
    ///
    /// NT4 control messages go out as WebSocket text frames, distinct from the
    /// batched binary value frames. This is the only text-send path; the
    /// transport layer must not call tungstenite directly.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent and
    /// [`FrameError::Io`] if the underlying socket write fails.
    pub fn send_text(&mut self, text: &str) -> Result<(), FrameError> {
        self.socket
            .send(Message::Text(text.into()))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)?;
        Ok(())
    }

    /// Sends an empty ping frame.
    ///
    /// NT4 4.1 mandates periodic pings to keep the connection alive; the
    /// transport layer emits one when its channel is idle.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent.
    pub fn send_ping(&mut self) -> Result<(), FrameError> {
        self.socket
            .send(Message::Ping(Vec::new().into()))
            .map_err(FrameError::Protocol)
    }

    /// Sends an empty pong frame.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent.
    pub fn send_pong(&mut self) -> Result<(), FrameError> {
        self.socket
            .send(Message::Pong(Vec::new().into()))
            .map_err(FrameError::Protocol)
    }

    /// Sends a close frame with the given code and reason.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent and
    /// [`FrameError::Io`] if the underlying socket write fails.
    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), FrameError> {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: Utf8Bytes::from(reason),
        };
        self.socket
            .send(Message::Close(Some(frame)))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)?;
        Ok(())
    }

    /// Sets the read timeout on the underlying TCP stream.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Io`] if the socket cannot be configured.
    pub fn set_read_timeout(&mut self, d: Duration) -> Result<(), FrameError> {
        self.socket
            .get_ref()
            .set_read_timeout(Some(d))
            .map_err(FrameError::Io)
    }
}

/// Picks the preferred subprotocol the client offered, if any.
///
/// NT4 negotiates 4.1 first with 4.0 as the fallback.
fn negotiate_subprotocol(offered: &str) -> Option<&'static str> {
    let offers: Vec<&str> = offered.split(',').map(str::trim).collect();
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
    /// Reads one complete message, blocking until the peer sends one.
    ///
    /// Pings are answered and closes acknowledged through the writer, so this
    /// never writes to the socket itself.
    ///
    /// # Errors
    ///
    /// The same errors as [`WebsocketConnection::recv`].
    pub fn recv(&mut self) -> Result<Payload, FrameError> {
        loop {
            match self.socket.read().map_err(FrameError::Protocol)? {
                Message::Binary(payload) => return Ok(Payload::Binary(payload.to_vec())),
                Message::Text(text) => return Ok(Payload::Text(text.to_string())),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Close(_) => return Err(FrameError::Closed),
                Message::Frame(_) => return Err(FrameError::UnexpectedFrame),
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

    /// Sends the batch as one binary frame and clears the buffer.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] or [`FrameError::Io`] on write failure.
    pub fn flush(&mut self) -> Result<(), FrameError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        self.socket
            .send(Message::Binary(std::mem::take(&mut self.batch).into()))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)
    }

    /// Sends `text` as one text frame and flushes.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] or [`FrameError::Io`] on write failure.
    pub fn send_text(&mut self, text: &str) -> Result<(), FrameError> {
        self.socket
            .send(Message::Text(text.into()))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)
    }

    /// Sends an empty ping frame.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] if the frame cannot be sent.
    pub fn send_ping(&mut self) -> Result<(), FrameError> {
        self.socket
            .send(Message::Ping(Vec::new().into()))
            .map_err(FrameError::Protocol)
    }

    /// Writes bytes the reader produced, keeping them whole and in order.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Io`] if the socket write fails.
    pub fn write_raw(&mut self, bytes: &[u8]) -> Result<(), FrameError> {
        self.flush()?;
        self.socket
            .get_mut()
            .write_all(bytes)
            .map_err(FrameError::Io)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)
    }

    /// Sends a close frame with the given code and reason.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Protocol`] or [`FrameError::Io`] on write failure.
    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), FrameError> {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: Utf8Bytes::from(reason),
        };
        self.socket
            .send(Message::Close(Some(frame)))
            .map_err(FrameError::Protocol)?;
        self.socket.get_mut().flush().map_err(FrameError::Io)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::{FrameError, NT4_SUBPROTOCOL, NT4_SUBPROTOCOL_V40, Payload, WebsocketConnection};

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
        assert!(matches!(
            server.join().unwrap(),
            Err(FrameError::Handshake(_))
        ));
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
        assert!(matches!(
            server.join().unwrap(),
            Err(FrameError::Handshake(_))
        ));
    }

    #[test]
    fn recv_binary_round_trips_masked_frame() {
        let (mut conn, mut client) = establish_connection("test");
        let payload = vec![0x94, 0x01, 0x02, 0x03, 0x04, 0x05];
        write_masked_binary(&mut client, &payload);
        assert_eq!(conn.recv().unwrap(), Payload::Binary(payload));
    }

    #[test]
    fn recv_binary_answers_ping_with_pong() {
        let (mut conn, mut client) = establish_connection("test");
        write_masked_frame(&mut client, 0x9, &[]);
        write_masked_binary(&mut client, &[7, 8]);
        assert_eq!(conn.recv().unwrap(), Payload::Binary(vec![7, 8]));
        let (opcode, _) = read_server_frame(&mut client);
        assert_eq!(opcode, 0xA, "expected a pong frame");
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
