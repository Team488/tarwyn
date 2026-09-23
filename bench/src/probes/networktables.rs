//! Raw NT4 probe: publishes or subscribes over the WebSocket the way an NT4
//! client does, with no client library in the way.

use crate::harness::{HEADER_LEN, Pacer, Recorder, RowId, SendStats, decode, encode, now_nanos};
use anyhow::Context as _;
use std::time::Duration;
use tarwyn_server::value::Value;
use tarwyn_server::websocket::message::{ControlMessage, ValueMessage};
use tungstenite::{ClientRequestBuilder, Message};

/// The NT4 WebSocket subprotocol.
const SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";
/// The NT4 resource the probe connects to, matching the client's `TABLE_PATH`.
const WS_PATH: &str = "/nt/test";
/// The default WebSocket port.
const WS_PORT: u16 = 5810;
/// The topic name both sides publish/subscribe to.
const CHANNEL: &str = "bench";
/// The publisher UID this probe publishes under, which keys every value
/// message it sends.
const PUBUID: u32 = 0;
/// The NT4 numeric data type for raw bytes (`xt_data_type(&Value::Bytes(..))`).
const DATA_TYPE_BYTES: u32 = 5;

fn websocket_url(host: &str) -> String {
    if host.contains(':') {
        format!("ws://{host}{WS_PATH}")
    } else {
        format!("ws://{host}:{WS_PORT}{WS_PATH}")
    }
}

fn connect(host: &str) -> anyhow::Result<tungstenite::WebSocket<std::net::TcpStream>> {
    let uri: tungstenite::http::Uri = websocket_url(host).parse()?;
    let port = uri.port_u16().unwrap_or(WS_PORT);
    let stream = std::net::TcpStream::connect((uri.host().unwrap_or_default(), port))
        .context(uri.clone())?;
    stream.set_nodelay(true)?;
    let request = ClientRequestBuilder::new(uri).with_sub_protocol(SUBPROTOCOL);
    let (socket, _) = tungstenite::client::client(request, stream)?;
    Ok(socket)
}

/// Wait for the server's `Announce` text frame, skipping anything before it.
fn wait_for_announce(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
) -> anyhow::Result<()> {
    loop {
        if let Message::Text(text) = socket.read()?
            && let Ok(ControlMessage::Announce { .. }) = ControlMessage::from_json(&text)
        {
            return Ok(());
        }
    }
}

/// Publish `count` paced samples of `payload` bytes over a raw NT4 connection.
/// The `publish` goes out as a one-element array, as NT4 specifies.
pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> anyhow::Result<()> {
    let mut socket = connect(host)?;

    let publish = format!(
        r#"[{{"method":"publish","params":{{"name":"{CHANNEL}","pubuid":0,"type":"bin","properties":{{}}}}}}]"#
    );
    socket.send(Message::text(publish))?;
    wait_for_announce(&mut socket)?;

    let mut buf = vec![0u8; payload.max(HEADER_LEN)];
    let mut wire = Vec::new();

    std::thread::sleep(Duration::from_millis(500));
    let mut pacer = Pacer::new(rate_hz);
    let mut stats = SendStats::new(pacer.interval_nanos());

    for seq in 0..count {
        let due = pacer.wait();
        encode(&mut buf, seq, due);
        let vm = ValueMessage {
            topic_id: PUBUID,
            timestamp_micros: due / 1000,
            data_type: DATA_TYPE_BYTES,
            value: Value::Bytes(buf.clone()),
        };
        wire.clear();
        vm.encode(&mut wire);
        let entered = now_nanos();
        let started = std::time::Instant::now();
        let result = socket.send(Message::binary(wire.clone()));
        stats.record(due, entered, started.elapsed());
        result?;
    }
    println!("sent {count} messages of {} B", buf.len());
    stats.report(count);
    Ok(())
}

/// How long a subscriber waits before reporting what it has. Must stay below
/// the harness's `--limit`, or the probe is killed first.
pub(crate) fn deadline_secs() -> u64 {
    std::env::var("BENCH_DEADLINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
}

/// Receive samples over a raw NT4 connection until `samples` are recorded or
/// the deadline passes.
pub fn subscribe(host: &str, payload: usize, samples: u64, id: &RowId) -> anyhow::Result<()> {
    let mut socket = connect(host)?;

    let subscribe = format!(
        r#"[{{"method":"subscribe","params":{{"topics":["{CHANNEL}"],"subuid":0,"options":{{}}}}}}]"#
    );
    socket.send(Message::text(subscribe))?;
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))?;

    let mut recorder = Recorder::new();
    println!("subscribed to '{CHANNEL}' on {host}, waiting for {samples} samples...");
    let deadline = std::time::Instant::now() + Duration::from_secs(deadline_secs());

    while recorder.len() < samples {
        recorder.close_elapsed_windows();
        if std::time::Instant::now() > deadline {
            println!("timed out with {}/{} samples", recorder.len(), samples);
            break;
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                for message in ValueMessage::decode_all(&bytes)? {
                    if let Value::Bytes(data) = message.value
                        && let Some((seq, sent)) = decode(&data)
                    {
                        recorder.record(seq, sent);
                    }
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }

    recorder.report(id, payload.max(HEADER_LEN));
    Ok(())
}
