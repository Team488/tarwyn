//! The WebSocket connection: opening it, splitting it, writing to it.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::Duration,
};

use tungstenite::{
    Message as WebsocketMessage, WebSocket, http::Request as HttpRequest, protocol::Role,
    stream::MaybeTlsStream,
};

pub(crate) const POLL_INTERVAL_MS: i32 = 100;
/// How long the reader loop blocks on the socket before it looks at the stop
/// flag and drains the outbound queue again.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(POLL_INTERVAL_MS as u64);

/// How long the reader blocks before draining frames that could not be written.
///
/// A publish is written by the calling thread, so this governs only what falls
/// back to the queue: a TLS connection, whose stream cannot be duplicated, and
/// anything published while the connection is down. It cannot usefully go lower
/// than a millisecond anyway, since a socket read timeout is rounded to the
/// kernel's timer granularity.
pub(crate) const OUTBOUND_POLL: Duration = Duration::from_millis(1);

pub(crate) fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// The writing half of a connection, shared by every thread that publishes.
///
/// `None` while disconnected, and for a TLS connection, whose stream cannot be
/// duplicated; publishes then fall back to the queue.
pub(crate) type SharedWriter = Arc<Mutex<Option<WebSocket<TcpStream>>>>;

/// The reading half of a split connection.
///
/// Reads come off the socket. Writes do not: tungstenite answers pings and
/// closes from inside `read`, and those bytes are already framed, so they are
/// buffered here and handed to the writing half under its lock. One writer on
/// the socket at a time is what stops a pong landing inside a value's frame.
#[derive(Debug)]
pub(crate) struct ReadHalf {
    stream: MaybeTlsStream<TcpStream>,
    writer: SharedWriter,
    pending: Vec<u8>,
}

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for ReadHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let mut guard = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_mut() {
            Some(writer) => {
                writer.get_mut().write_all(&self.pending)?;
                writer.get_mut().flush()?;
            }
            None => {
                self.stream.write_all(&self.pending)?;
                self.stream.flush()?;
            }
        }
        self.pending.clear();
        Ok(())
    }
}

/// Split a freshly connected socket into a reader and a shared writer.
///
/// The writing half is a second handle on the same socket, so a publish is
/// written by the thread that made it. Without one it waits for the reader to
/// return from a blocking read, and that wait is a whole kernel tick: the
/// timeout a socket read takes is rounded to the timer's granularity, so no
/// choice of poll interval gets it under a millisecond.
///
/// Split before anything is sent, so the codec has no buffered frames to lose.
pub(crate) fn split_connection(
    websocket: WebSocket<MaybeTlsStream<TcpStream>>,
    writer: &SharedWriter,
) -> (WebSocket<ReadHalf>, Option<WebSocket<TcpStream>>) {
    let stream = websocket.into_inner();
    let duplicate = match &stream {
        MaybeTlsStream::Plain(tcp) => tcp.try_clone().ok(),
        _ => None,
    };
    let reader = WebSocket::from_raw_socket(
        ReadHalf {
            stream,
            writer: Arc::clone(writer),
            pending: Vec::new(),
        },
        Role::Client,
        None,
    );
    (
        reader,
        duplicate.map(|tcp| WebSocket::from_raw_socket(tcp, Role::Client, None)),
    )
}

/// Write one frame on the calling thread, or hand it back to be queued.
pub(crate) fn write_frame(writer: &SharedWriter, frame: Vec<u8>) -> Option<Vec<u8>> {
    let mut guard = writer.lock().unwrap_or_else(|p| p.into_inner());
    let Some(websocket) = guard.as_mut() else {
        return Some(frame);
    };
    if websocket.send(WebsocketMessage::binary(frame)).is_err() {
        // The connection is gone; the reader will notice and rebuild it.
        *guard = None;
    }
    None
}

/// Establish the WebSocket connection, requesting the NT4 subprotocol.
pub(crate) fn connect_websocket(
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
    let (websocket, _response) = tungstenite::connect(request)?;
    // Without this the kernel holds a small write back until the previous one is
    // acknowledged, which puts hundreds of microseconds in front of every value
    // a publisher sends. The server sets it on its side of every connection.
    if let MaybeTlsStream::Plain(stream) = websocket.get_ref() {
        let _ = stream.set_nodelay(true);
    }
    Ok(websocket)
}

/// Give the reader loop a bounded read so it can drain outbound and check stop.
pub(crate) fn set_read_timeout(websocket: &WebSocket<ReadHalf>, timeout: Duration) {
    if let MaybeTlsStream::Plain(stream) = &websocket.get_ref().stream {
        let _ = stream.set_read_timeout(Some(timeout));
    }
}

pub(crate) fn is_timeout(e: &tungstenite::Error) -> bool {
    matches!(
        e,
        tungstenite::Error::Io(io)
            if io.kind() == std::io::ErrorKind::WouldBlock
                || io.kind() == std::io::ErrorKind::TimedOut
    )
}

/// Send every queued outbound frame. Returns `false` if the connection died.
pub(crate) fn drain_outbound(
    websocket: &mut WebSocket<ReadHalf>,
    outbound: &Receiver<Vec<u8>>,
) -> bool {
    while let Ok(frame) = outbound.try_recv() {
        if websocket.send(WebsocketMessage::binary(frame)).is_err() {
            return false;
        }
    }
    true
}

/// Drop every queued outbound frame while disconnected, counting them.
pub(crate) fn drain_outbound_dropped(outbound: &Receiver<Vec<u8>>, dropped: &AtomicU64) {
    while outbound.try_recv().is_ok() {
        dropped.fetch_add(1, Ordering::Relaxed);
    }
}
