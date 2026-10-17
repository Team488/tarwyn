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
    connect_as(host, "test")
}

/// Open an NT4 connection to `host` as the client `name`.
pub(crate) fn connect_as(
    host: &str,
    name: &str,
) -> anyhow::Result<tungstenite::WebSocket<std::net::TcpStream>> {
    let path = format!("/nt/{name}");
    let url = websocket_url(host).replace(WS_PATH, &path);
    let uri: tungstenite::http::Uri = url.parse()?;
    let port = uri.port_u16().unwrap_or(WS_PORT);
    let stream = std::net::TcpStream::connect((uri.host().unwrap_or_default(), port))
        .context(uri.clone())?;
    stream.set_nodelay(true)?;
    let request = ClientRequestBuilder::new(uri).with_sub_protocol(SUBPROTOCOL);
    let (socket, _) = tungstenite::client::client(request, stream)?;
    Ok(socket)
}

/// Whether a control frame holds an `Announce`, alone or among other messages.
fn announces(frame: &str) -> bool {
    ControlMessage::from_json_batch(frame).is_ok_and(|messages| {
        messages
            .iter()
            .any(|m| matches!(m, ControlMessage::Announce { .. }))
    })
}

/// Wait for the server's `Announce` text frame, skipping anything before it.
fn wait_for_announce(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
) -> anyhow::Result<()> {
    loop {
        if let Message::Text(text) = socket.read()?
            && announces(&text)
        {
            return Ok(());
        }
    }
}

/// The probe's `publish`, and a `topicsonly` subscription to the same topic.
/// ntcore 2026 sends nothing to a client that has no subscription, so without
/// it the announce answering the publish never comes.
fn publish_frame() -> String {
    format!(
        r#"[{{"method":"publish","params":{{"name":"{CHANNEL}","pubuid":0,"type":"bin","properties":{{}}}}}},{{"method":"subscribe","params":{{"topics":["{CHANNEL}"],"subuid":0,"options":{{"topicsonly":true}}}}}}]"#
    )
}

/// The probe's subscription to its topic, asking for every value at the 1 ms
/// period the pyntcore probes use.
fn subscribe_frame() -> String {
    format!(
        r#"[{{"method":"subscribe","params":{{"topics":["{CHANNEL}"],"subuid":0,"options":{{"all":true,"periodic":0.001}}}}}}]"#
    )
}

/// Publish `count` paced samples of `payload` bytes over a raw NT4 connection.
/// The `publish` goes out as a one-element array, as NT4 specifies.
pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> anyhow::Result<()> {
    let mut socket = connect(host)?;

    socket.send(Message::text(publish_frame()))?;
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

    socket.send(Message::text(subscribe_frame()))?;
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

#[cfg(test)]
mod tests {
    use super::announces;
    use tarwyn_server::websocket::message::ControlMessage;

    /// An NT4 text frame is an array, so the announce may share it with other
    /// control messages.
    #[test]
    fn an_announce_is_found_in_a_frame_of_several_messages() {
        let frame = r#"[{"method":"announce","params":{"name":"bench","id":3,"type":"raw","properties":{},"pubuid":0}},{"method":"properties","params":{"name":"bench","update":{}}}]"#;
        assert!(announces(frame));
        assert!(!announces(
            r#"[{"method":"properties","params":{"name":"bench","update":{}}}]"#
        ));
    }

    /// ntcore 2026 sends nothing to a client with no subscription, the
    /// announce answering its publish included.
    #[test]
    fn the_publisher_subscribes_to_its_own_announcement() {
        let messages = ControlMessage::from_json_batch(&super::publish_frame()).unwrap();
        assert!(matches!(messages[0], ControlMessage::Publish { .. }));
        assert!(matches!(
            &messages[1],
            ControlMessage::Subscribe { topics, options, .. }
                if topics == &[super::CHANNEL.to_string()] && options["topicsonly"] == true
        ));
    }

    /// Without `all`, an NT4 server may send only the latest value per
    /// period, as ntcore 2026 does, and every other sample reads as lost. A
    /// 1 ms period keeps it from holding values for its default 100 ms.
    #[test]
    fn the_subscriber_asks_for_every_value() {
        let messages = ControlMessage::from_json_batch(&super::subscribe_frame()).unwrap();
        assert!(matches!(
            &messages[0],
            ControlMessage::Subscribe { topics, options, .. }
                if topics == &[super::CHANNEL.to_string()]
                    && options["all"] == true
                    && options["periodic"] == 0.001
        ));
    }
}
