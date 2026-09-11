use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::Server;
use crate::value::Value;
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
fn connect(server: &Server) -> TcpStream {
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
    let handle = server.start();
    let mut client = connect(&server);

    let mut rtt = Vec::new();
    ValueMessage {
        topic_id: RTT_TOPIC_ID,
        timestamp_micros: 0,
        data_type: 2,
        value: Value::Int64(1234),
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
    let server = Server::bind_loopback().unwrap();
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
            value: Value::Double(v),
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
            Value::Double(d) => Some(d),
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
        value: Value::Double(9.5),
    }
    .encode(&mut unknown);
    write_masked_binary(&mut client, &unknown);
    write_masked_binary(&mut client, &batch);

    let (_, payload) = read_server_frame(&mut client);
    let values: Vec<f64> = ValueMessage::decode_all(&payload)
        .unwrap()
        .into_iter()
        .filter_map(|m| match m.value {
            Value::Double(d) => Some(d),
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
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
    let server = Server::bind_loopback().unwrap();
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
        server.fan_out("child", &Value::Double(1.0), ts);
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
    let server = Server::bind_loopback().unwrap();
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
    server.fan_out("child", &Value::Double(1.5), 100);

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
    let server = Server::bind_loopback().unwrap();
    let handle = server.start();
    let client = connect(&server);

    server.stop_flag().store(true, Ordering::Relaxed);
    handle.join().unwrap();

    // The connection is still open but the accept loop has exited.
    let _ = client;
}

#[test]
fn an_rtt_connection_answers_timestamps_and_joins_no_client_list() {
    let server = Server::bind_loopback().unwrap();
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
        value: Value::Double(1234.5),
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
        Value::Double(1234.5),
        "the client's own value must come back unchanged"
    );
    assert!(
        replies[0].timestamp_micros > 0,
        "the reply carries the server's time"
    );
}
