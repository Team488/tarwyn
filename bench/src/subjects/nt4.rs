//! NT4-over-WebSocket subject: measures the full publish -> server -> subscribe
//! path over the server's WS endpoint, exactly as a real NT4 client drives it.
//!
//! The publisher and subscriber are separate processes on one host. The
//! publisher performs the NT4 `publish` handshake, reads the server's `Announce`
//! to learn the topic id, then sends paced `ValueMessage` binary frames. The
//! subscriber performs the NT4 `subscribe` handshake and records the one-way
//! latency of every binary value frame it receives.

use crate::harness::{HEADER_LEN, Pacer, Recorder, decode, encode};
use std::time::Duration;
use tungstenite::{ClientRequestBuilder, Message};
use tarwyn_server::value::XtValue;
use tarwyn_server::websocket::message::ValueMessage;

/// The NT4 WebSocket subprotocol.
const SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The server's NT4 table path (`TABLE_PATH` in `core/src/websocket/server.rs`).
const WS_PATH: &str = "/nt/test";
/// The default WS port.
const WS_PORT: u16 = 4881;
/// The topic name both sides publish/subscribe to.
const CHANNEL: &str = "bench";
/// The NT4 numeric data type for raw bytes (`xt_data_type(&XtValue::Bytes(..))`).
const DATA_TYPE_BYTES: u32 = 5;

fn ws_url(host: &str) -> String {
    if host.contains(':') {
        format!("ws://{host}{WS_PATH}")
    } else {
        format!("ws://{host}:{WS_PORT}{WS_PATH}")
    }
}

fn connect(host: &str) -> std::io::Result<tungstenite::WebSocket<std::net::TcpStream>> {
    let uri: tungstenite::http::Uri = ws_url(host)
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid ws url: {e}")))?;
    let host_str = uri
        .host()
        .ok_or_else(|| std::io::Error::other("ws url has no host"))?;
    let port = uri.port_u16().unwrap_or(WS_PORT);
    let stream = std::net::TcpStream::connect((host_str, port))
        .map_err(|e| std::io::Error::other(format!("tcp connect: {e}")))?;
    stream
        .set_nodelay(true)
        .map_err(|e| std::io::Error::other(format!("set_nodelay: {e}")))?;
    let request = ClientRequestBuilder::new(uri).with_sub_protocol(SUBPROTOCOL);
    let (socket, _) = tungstenite::client::client(request, stream)
        .map_err(|e| std::io::Error::other(format!("ws handshake: {e}")))?;
    Ok(socket)
}

/// Extract the topic id from the server's `Announce` JSON (`params.id`).
///
/// The announce is `{"method":"announce","params":{"name":"bench","id":N,...}}`;
/// `"id"` appears exactly once, so a targeted scan is enough.
fn extract_topic_id(json: &str) -> Option<u32> {
    let marker = "\"id\":";
    let start = json.find(marker)? + marker.len();
    let rest = &json[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Read the server's `Announce` text frame and return the assigned topic id.
fn read_topic_id(socket: &mut tungstenite::WebSocket<std::net::TcpStream>) -> std::io::Result<u32> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                return extract_topic_id(&text)
                    .ok_or_else(|| std::io::Error::other("announce had no topic id"));
            }
            Ok(_) => {} // ignore ping/pong/binary until the announce arrives
            Err(e) => return Err(std::io::Error::other(format!("ws read: {e}"))),
        }
    }
}

/// Decode one msgpack integer, returning `(value, remaining)`.
///
/// The core's `encode_uint` emits signed int markers (`0xd0`-`0xd3`) for values
/// that fit in `i64`, so both unsigned and signed markers must be handled.
fn read_uint(buf: &[u8]) -> std::io::Result<(u64, &[u8])> {
    let (&marker, rest) = buf
        .split_first()
        .ok_or_else(|| std::io::Error::other("truncated uint"))?;
    let (value, n) = match marker {
        0x00..=0x7f => (marker as u64, 0),
        0xcc => (rest[0] as u64, 1),
        0xcd => (u16::from_be_bytes([rest[0], rest[1]]) as u64, 2),
        0xce => (
            u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as u64,
            4,
        ),
        0xcf => (u64::from_be_bytes(rest[..8].try_into().unwrap()), 8),
        0xd0 => (rest[0] as i8 as u64, 1),
        0xd1 => (i16::from_be_bytes([rest[0], rest[1]]) as u64, 2),
        0xd2 => (
            i32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as u64,
            4,
        ),
        0xd3 => (i64::from_be_bytes(rest[..8].try_into().unwrap()) as u64, 8),
        _ => return Err(std::io::Error::other("expected integer")),
    };
    Ok((value, &rest[n..]))
}

/// Decode a msgpack bin (raw bytes), returning `(bytes, remaining)`.
///
/// The benchmark publishes `XtValue::Bytes`, which the core encodes as a bin.
fn read_bin(buf: &[u8]) -> std::io::Result<(Vec<u8>, &[u8])> {
    let (&marker, rest) = buf
        .split_first()
        .ok_or_else(|| std::io::Error::other("truncated bin"))?;
    let (len, n) = match marker {
        0xc4 => (rest[0] as usize, 1),
        0xc5 => (u16::from_be_bytes([rest[0], rest[1]]) as usize, 2),
        0xc6 => (
            u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize,
            4,
        ),
        _ => return Err(std::io::Error::other("expected bin")),
    };
    let data = rest[n..n + len].to_vec();
    Ok((data, &rest[n + len..]))
}

/// Decode every `ValueMessage` in a batched binary frame.
///
/// The server coalesces consecutive value messages into one frame, so a frame
/// is a run of `fixarray(4)` tuples. Each tuple is `[topic_id, ts, data_type,
/// value]`; the value is a bin (the benchmark publishes `Bytes`).
fn decode_batch(buf: &[u8], mut f: impl FnMut(u32, u64, u32, &XtValue)) -> std::io::Result<()> {
    let mut rest = buf;
    while !rest.is_empty() {
        if rest[0] != 0x94 {
            return Err(std::io::Error::other("expected fixarray(4)"));
        }
        rest = &rest[1..];
        let (topic_id, r) = read_uint(rest)?;
        rest = r;
        let (ts, r) = read_uint(rest)?;
        rest = r;
        let (dt, r) = read_uint(rest)?;
        rest = r;
        let (data, r) = read_bin(rest)?;
        rest = r;
        f(topic_id as u32, ts, dt as u32, &XtValue::Bytes(data));
    }
    Ok(())
}

pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let mut socket = connect(host)?;

    // NT4 publish handshake: the server answers with an Announce carrying the
    // topic id we must reuse in every value message. Control messages ride
    // binary frames; the server accepts control JSON on either frame type.
    let publish = format!(
        r#"{{"method":"publish","params":{{"name":"{CHANNEL}","pubuid":0,"type":"bin","properties":{{}}}},"id":0}}"#
    );
    socket
        .send(Message::binary(publish.into_bytes()))
        .map_err(|e| std::io::Error::other(format!("ws send: {e}")))?;
    let topic_id = read_topic_id(&mut socket)?;

    let mut buf = vec![0u8; payload.max(HEADER_LEN)];
    let mut pacer = Pacer::new(rate_hz);
    let mut wire = Vec::new();

    std::thread::sleep(Duration::from_millis(500));

    for seq in 0..count {
        pacer.wait();
        encode(&mut buf, seq);
        let vm = ValueMessage {
            topic_id,
            timestamp_micros: crate::harness::now_nanos() / 1000,
            data_type: DATA_TYPE_BYTES,
            value: XtValue::Bytes(buf.clone()),
        };
        wire.clear();
        vm.encode(&mut wire);
        socket
            .send(Message::binary(wire.clone()))
            .map_err(|e| std::io::Error::other(format!("ws send: {e}")))?;
    }
    println!("sent {count} messages of {} B", buf.len());
    Ok(())
}

pub fn subscribe(host: &str, payload: usize, samples: u64) -> std::io::Result<()> {
    let mut socket = connect(host)?;

    // NT4 subscribe handshake. The topic may not exist yet; the server matches
    // the subscription when the publisher later announces it. Control messages
    // ride binary frames; the server accepts control JSON on either frame type.
    let subscribe = format!(
        r#"{{"method":"subscribe","params":{{"topics":["{CHANNEL}"],"subuid":0,"options":{{}}}},"id":0}}"#
    );
    socket
        .send(Message::binary(subscribe.into_bytes()))
        .map_err(|e| std::io::Error::other(format!("ws send: {e}")))?;
    // A short read timeout lets the loop check the deadline while idle.
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| std::io::Error::other(format!("ws read timeout: {e}")))?;

    let mut recorder = Recorder::new();
    println!("subscribed to '{CHANNEL}' on {host}, waiting for {samples} samples...");
    let deadline = std::time::Instant::now() + Duration::from_secs(120);

    while recorder.len() < samples {
        if std::time::Instant::now() > deadline {
            println!("timed out with {}/{} samples", recorder.len(), samples);
            break;
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                decode_batch(&bytes, |_topic_id, _ts, _dt, value| {
                    if let XtValue::Bytes(data) = value
                        && let Some((seq, sent)) = decode(data)
                    {
                        recorder.record(seq, sent);
                    }
                })?;
            }
            // Ignore text frames (Announce/control) and ping/pong/close.
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(std::io::Error::other(format!("ws read: {e}"))),
        }
    }

    recorder.report(
        &format!("nt4-ws v{}", env!("CARGO_PKG_VERSION")),
        payload.max(HEADER_LEN),
    );
    Ok(())
}
