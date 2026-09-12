use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use tarwyn_protobuf::protobuf::GetDataCommand;
use tarwyn_protobuf::protobuf::{
    BoolList, DoubleList, FloatList, IntegerList, LongList, StringList,
};

/// The RFC 6455 example key.
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
const NT4_SUBPROTOCOL: &str = "v4.1.networktables.first.wpi.edu";

fn get_request(channel: &str) -> Vec<u8> {
    Request {
        payload: Some(request::Payload::Data(GetDataCommand {
            channel: channel.to_string(),
        })),
    }
    .encode_to_vec()
}

fn string(value: &str) -> supported_values::Kind {
    supported_values::Kind::String(value.to_string())
}

fn wrap(kind: supported_values::Kind) -> Option<Box<SupportedValues>> {
    Some(Box::new(SupportedValues { kind: Some(kind) }))
}

/// Connects a WebSocket client to the server and completes the handshake.
fn connect(server: &Server) -> TcpStream {
    let port = server.websocket.local_addr().unwrap().port();
    let mut client = TcpStream::connect(("127.0.0.1", port))
        .expect("a client reaches the server over loopback, not the wildcard it listens on");
    let req = format!(
        "GET /nt/test HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Protocol: {NT4_SUBPROTOCOL}\r\n\r\n"
    );
    client.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = client.read(&mut buf).unwrap();
        assert!(n > 0, "server closed during handshake");
        resp.extend_from_slice(&buf[..n]);
        if resp.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    assert!(
        String::from_utf8(resp).unwrap().starts_with("HTTP/1.1 101"),
        "handshake failed"
    );
    client
}

/// Writes a masked binary frame.
fn write_masked_binary(stream: &mut TcpStream, payload: &[u8]) {
    let mask = [0x12, 0x34, 0x56, 0x78];
    let mut header = vec![0x80 | 0x2];
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

/// Reads one server frame, returning `None` on a clean close or timeout.
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

/// Sends a control request and returns the decoded reply payload.
fn control_round_trip(server: &Server, request: &[u8]) -> reply::Payload {
    let mut client = connect(server);
    write_masked_binary(&mut client, request);
    let (opcode, payload) = read_server_frame(&mut client);
    assert_eq!(opcode, 0x2, "control reply must be a binary frame");
    let reply = Reply::decode(payload.as_slice()).expect("not a Reply");
    reply.payload.expect("reply carried no payload")
}

fn publish_frame(name: &str, pubuid: u32, data_type: &str) -> Vec<u8> {
    crate::websocket::message::ControlMessage::Publish {
        name: name.to_string(),
        pubuid,
        data_type: data_type.to_string(),
        properties: serde_json::Map::new(),
    }
    .to_json()
    .into_bytes()
}

fn value_frame(pubuid: u32, value: Value) -> Vec<u8> {
    let mut buf = Vec::new();
    crate::websocket::message::ValueMessage {
        topic_id: pubuid,
        timestamp_micros: Server::now_micros(),
        data_type: crate::websocket::protocol::xt_data_type(&value),
        value,
    }
    .encode(&mut buf);
    buf
}

/// A dashboard edit is an NT4 publish plus a value, and has to land.
#[test]
fn a_value_an_nt_client_writes_is_readable_over_the_control_plane() {
    let server = Server::with_ports_and_telemetry(22301, 22302, 22303, 22304);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let mut dashboard = connect(&server);
    write_masked_binary(&mut dashboard, &publish_frame("edited", 1, "double"));
    std::thread::sleep(Duration::from_millis(150));
    write_masked_binary(&mut dashboard, &value_frame(1, Value::Double(4.88)));
    std::thread::sleep(Duration::from_millis(200));

    let reply = control_round_trip(&server, &get_request("edited"));
    server.stop();

    match reply {
        reply::Payload::Data(cmd) => assert_eq!(
            cmd.value.and_then(|v| v.kind),
            Some(supported_values::Kind::Double(4.88)),
            "an edit from an NT client has to reach the server's read cache"
        ),
        other => panic!("expected data reply, got {other:?}"),
    }
}

/// The two planes must agree on what a topic holds.
#[test]
fn a_value_of_the_wrong_type_reaches_neither_plane() {
    let server = Server::with_ports_and_telemetry(22311, 22312, 22313, 22314);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let mut dashboard = connect(&server);
    write_masked_binary(&mut dashboard, &publish_frame("typed", 1, "double"));
    std::thread::sleep(Duration::from_millis(150));
    write_masked_binary(
        &mut dashboard,
        &value_frame(1, Value::String("not a double".into())),
    );
    std::thread::sleep(Duration::from_millis(200));

    let reply = control_round_trip(&server, &get_request("typed"));
    server.stop();

    match reply {
        reply::Payload::Data(cmd) => assert_eq!(
            cmd.value.and_then(|v| v.kind),
            Some(supported_values::Kind::String(NO_DATA_SENTINEL.to_string())),
            "the NT4 plane rejected this value for its type, so the read cache \
                 must not hold it either"
        ),
        other => panic!("expected data reply, got {other:?}"),
    }
}

/// An uncapped accept loop is two threads per connection with no ceiling.
#[test]
fn the_server_stops_accepting_past_the_connection_cap() {
    use crate::websocket::server::MAX_CONNECTIONS;

    let server = Server::with_ports_and_telemetry(22321, 22322, 22323, 22324);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let held: Vec<TcpStream> = (0..MAX_CONNECTIONS).map(|_| connect(&server)).collect();

    let port = server.websocket.local_addr().unwrap().port();
    let mut extra = TcpStream::connect(("127.0.0.1", port)).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let refused = extra.write_all(b"GET /nt/over HTTP/1.1\r\n\r\n").is_err() || {
        let mut buf = [0u8; 1];
        matches!(extra.read(&mut buf), Ok(0) | Err(_))
    };

    server.stop();
    drop(held);

    assert!(
        refused,
        "the {MAX_CONNECTIONS}th connection was already the cap, so this one \
             has to be dropped rather than given two more threads"
    );
}

#[test]
fn reading_an_absent_channel_does_not_invent_it() {
    let cached: HashMap<String, RingBuffer<supported_values::Kind>> = HashMap::new();

    let value = Server::read(&cached, "never-published");

    assert!(value.is_none());
    assert!(
        cached.is_empty(),
        "a read created the channel, so getTables reports one that was never published \
             and the map grows for every name anyone asks about"
    );
}

#[test]
fn a_refused_compare_and_set_does_not_invent_the_channel() {
    let mut cached = HashMap::new();

    let (swapped, current) = Server::compare_and_set(
        &mut cached,
        CompareAndSetCommand {
            channel: "never-published".into(),
            expected: wrap(string("something")),
            value: wrap(string("agent-a")),
            expect_absent: false,
        },
    );

    assert!(!swapped);
    assert_eq!(current, None);
    assert!(cached.is_empty(), "a refused swap created the channel");
}

#[test]
fn compare_and_set_claims_an_empty_channel_once() {
    let mut cached = HashMap::new();

    let (claimed, _) = Server::compare_and_set(
        &mut cached,
        CompareAndSetCommand {
            channel: "lock".into(),
            expected: None,
            value: wrap(string("agent-a")),
            expect_absent: true,
        },
    );
    assert!(claimed);

    let (stolen, current) = Server::compare_and_set(
        &mut cached,
        CompareAndSetCommand {
            channel: "lock".into(),
            expected: None,
            value: wrap(string("agent-b")),
            expect_absent: true,
        },
    );
    assert!(
        !stolen,
        "a second claimant took a lock that was already held"
    );
    assert_eq!(current, Some(string("agent-a")));
}

#[test]
fn compare_and_set_refuses_a_stale_expectation() {
    let mut cached = HashMap::new();
    cached
        .entry(String::from("counter"))
        .or_insert_with(|| RingBuffer::new(100))
        .push(supported_values::Kind::Double(1.0));

    let (moved, _) = Server::compare_and_set(
        &mut cached,
        CompareAndSetCommand {
            channel: "counter".into(),
            expected: wrap(supported_values::Kind::Double(1.0)),
            value: wrap(supported_values::Kind::Double(2.0)),
            expect_absent: false,
        },
    );
    assert!(moved);

    let (again, current) = Server::compare_and_set(
        &mut cached,
        CompareAndSetCommand {
            channel: "counter".into(),
            expected: wrap(supported_values::Kind::Double(1.0)),
            value: wrap(supported_values::Kind::Double(3.0)),
            expect_absent: false,
        },
    );
    assert!(!again, "a read-modify-write raced and both writers won");
    assert_eq!(current, Some(supported_values::Kind::Double(2.0)));
}

#[test]
fn json_escapes_what_would_otherwise_break_the_document() {
    let mut cached = HashMap::new();
    cached
        .entry(String::from("quote\"and\\slash"))
        .or_insert_with(|| RingBuffer::new(100))
        .push(string("line\nbreak\ttab"));

    let json = Server::to_json(&cached, "");
    assert_eq!(
        json, r#"{"quote\"and\\slash":"line\nbreak\ttab"}"#,
        "the document would not parse"
    );
}

#[test]
fn json_leaves_out_channels_outside_the_prefix() {
    let mut cached = HashMap::new();
    for name in ["robot/a", "robot/b", "vision/c"] {
        cached
            .entry(String::from(name))
            .or_insert_with(|| RingBuffer::new(100))
            .push(supported_values::Kind::Bool(true));
    }

    assert_eq!(
        Server::to_json(&cached, "robot/"),
        r#"{"robot/a":true,"robot/b":true}"#
    );
}

#[test]
fn kind_xtvalue_round_trips_scalars_and_lists() {
    use supported_values::Kind;
    let cases: Vec<Kind> = vec![
        Kind::String("hi".into()),
        Kind::Int32(-5),
        Kind::Int64(-9_000_000_000),
        Kind::Uint32(7),
        Kind::Uint64(9_000_000_000),
        Kind::Bool(true),
        Kind::Double(1.5),
        Kind::Float(2.5),
        Kind::Bytes(vec![1, 2, 3]),
        Kind::StringList(StringList {
            values: vec!["a".into(), "b".into()],
        }),
        Kind::FloatList(FloatList {
            values: vec![1.0, 2.0],
        }),
        Kind::BoolList(BoolList {
            values: vec![true, false],
        }),
        Kind::DoubleList(DoubleList {
            values: vec![1.5, 2.5],
        }),
        Kind::IntegerList(IntegerList {
            values: vec![1, 2, 3],
        }),
        Kind::LongList(LongList {
            values: vec![1, 2, 3],
        }),
    ];
    for kind in cases {
        let value = Value::from(kind.clone());
        let back = supported_values::Kind::from(value);
        assert_eq!(back, kind, "round trip changed the value");
    }
}

#[test]
fn control_plane_get_returns_no_data_for_absent_channel() {
    let server = Server::with_ports_and_telemetry(21841, 21842, 21843, 21844);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    match control_round_trip(&server, &get_request("absent")) {
        reply::Payload::Data(cmd) => {
            let value = cmd.value.and_then(|v| v.kind);
            assert_eq!(
                value,
                Some(supported_values::Kind::String(NO_DATA_SENTINEL.to_string()))
            );
        }
        other => panic!("expected data reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_cas_then_get() {
    let server = Server::with_ports_and_telemetry(21851, 21852, 21853, 21854);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let cas_request = Request {
        payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: "lock".into(),
            expected: None,
            value: wrap(string("agent-a")),
            expect_absent: true,
        })),
    }
    .encode_to_vec();
    match control_round_trip(&server, &cas_request) {
        reply::Payload::CompareAndSet(cmd) => {
            assert!(cmd.swapped, "CAS should claim an empty channel");
        }
        other => panic!("expected CAS reply, got {other:?}"),
    }

    match control_round_trip(&server, &get_request("lock")) {
        reply::Payload::Data(cmd) => {
            assert_eq!(cmd.value.and_then(|v| v.kind), Some(string("agent-a")));
        }
        other => panic!("expected data reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_tables_lists_channels() {
    let server = Server::with_ports_and_telemetry(21861, 21862, 21863, 21864);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let cas_a = Request {
        payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: "robot/a".into(),
            expected: None,
            value: wrap(string("va")),
            expect_absent: true,
        })),
    }
    .encode_to_vec();
    let _ = control_round_trip(&server, &cas_a);

    let cas_b = Request {
        payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: "robot/b".into(),
            expected: None,
            value: wrap(string("vb")),
            expect_absent: true,
        })),
    }
    .encode_to_vec();
    let _ = control_round_trip(&server, &cas_b);

    let tables_request = Request {
        payload: Some(request::Payload::Tables(
            tarwyn_protobuf::protobuf::ListTablesCommand {
                prefix: "robot/".into(),
            },
        )),
    }
    .encode_to_vec();
    match control_round_trip(&server, &tables_request) {
        reply::Payload::Tables(cmd) => {
            assert_eq!(cmd.channels, vec!["robot/a", "robot/b"]);
        }
        other => panic!("expected tables reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_ping_returns_server_nanos() {
    let server = Server::with_ports_and_telemetry(21871, 21872, 21873, 21874);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let ping_request = Request {
        payload: Some(request::Payload::Ping(
            tarwyn_protobuf::protobuf::PingCommand { sent_nanos: 42 },
        )),
    }
    .encode_to_vec();
    match control_round_trip(&server, &ping_request) {
        reply::Payload::Ping(cmd) => {
            assert_eq!(cmd.sent_nanos, 42);
            assert!(cmd.server_nanos > 0);
        }
        other => panic!("expected ping reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_statistics() {
    let server = Server::with_ports_and_telemetry(21881, 21882, 21883, 21884);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let stats_request = Request {
        payload: Some(request::Payload::Statistics(
            tarwyn_protobuf::protobuf::StatisticsCommand {},
        )),
    }
    .encode_to_vec();
    match control_round_trip(&server, &stats_request) {
        reply::Payload::Statistics(cmd) => {
            assert_eq!(cmd.version, env!("CARGO_PKG_VERSION"));
        }
        other => panic!("expected statistics reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_json() {
    let server = Server::with_ports_and_telemetry(21891, 21892, 21893, 21894);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let cas_request = Request {
        payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: "test".into(),
            expected: None,
            value: wrap(string("hello")),
            expect_absent: true,
        })),
    }
    .encode_to_vec();
    let _ = control_round_trip(&server, &cas_request);

    let json_request = Request {
        payload: Some(request::Payload::Json(
            tarwyn_protobuf::protobuf::JsonCommand {
                prefix: "test".into(),
            },
        )),
    }
    .encode_to_vec();
    match control_round_trip(&server, &json_request) {
        reply::Payload::Json(cmd) => {
            assert!(cmd.json.contains("hello"));
        }
        other => panic!("expected json reply, got {other:?}"),
    }
    server.stop();
}

#[test]
fn control_plane_delete() {
    let server = Server::with_ports_and_telemetry(21901, 21902, 21903, 21904);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let cas_request = Request {
        payload: Some(request::Payload::CompareAndSet(CompareAndSetCommand {
            channel: "del".into(),
            expected: None,
            value: wrap(string("val")),
            expect_absent: true,
        })),
    }
    .encode_to_vec();
    let _ = control_round_trip(&server, &cas_request);

    let delete_request = Request {
        payload: Some(request::Payload::Delete(
            tarwyn_protobuf::protobuf::DeleteCommand {
                channel: "del".into(),
            },
        )),
    }
    .encode_to_vec();
    match control_round_trip(&server, &delete_request) {
        reply::Payload::Delete(cmd) => {
            assert_eq!(cmd.deleted, 1);
        }
        other => panic!("expected delete reply, got {other:?}"),
    }

    match control_round_trip(&server, &get_request("del")) {
        reply::Payload::Data(cmd) => {
            let value = cmd.value.and_then(|v| v.kind);
            assert_eq!(value, Some(string(NO_DATA_SENTINEL)));
        }
        other => panic!("expected data reply after delete, got {other:?}"),
    }
    server.stop();
}

#[test]
fn stop_joins_its_loops_so_the_sockets_can_be_picked_up_again() {
    let server = Server::with_ports_and_telemetry(21911, 21912, 21913, 21914);
    server.start();
    std::thread::sleep(Duration::from_millis(200));
    server.stop();

    assert!(
        server.threads.lock().unwrap().is_empty(),
        "stop() left thread handles behind"
    );
}

#[test]
fn malformed_ws_payload_closes_connection() {
    let server = Server::with_ports_and_telemetry(21921, 21922, 21923, 21924);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let mut client = connect(&server);
    write_masked_binary(&mut client, b"\xff\xfe\xfd\xfc");
    let _ = client.set_read_timeout(Some(Duration::from_secs(2)));

    let closed = match try_read_server_frame(&mut client) {
        Ok(Some((opcode, _))) => opcode == 0x8,
        Ok(None) => true,
        Err(_) => false,
    };
    assert!(
        closed,
        "server must close the connection on malformed input"
    );

    let mut client2 = connect(&server);
    let ping_request = Request {
        payload: Some(request::Payload::Ping(
            tarwyn_protobuf::protobuf::PingCommand { sent_nanos: 1 },
        )),
    }
    .encode_to_vec();
    write_masked_binary(&mut client2, &ping_request);
    let (opcode, _) = read_server_frame(&mut client2);
    assert_eq!(
        opcode, 0x2,
        "server must still accept requests after a malformed one"
    );

    server.stop();
}

#[test]
fn dropping_a_server_releases_its_ws_port() {
    let port;
    {
        let server = Server::with_ports_and_telemetry(21931, 21932, 21933, 21934);
        server.start();
        port = server.websocket.local_addr().unwrap().port();
        std::thread::sleep(Duration::from_millis(200));
    }

    assert!(
        std::net::TcpListener::bind((DEFAULT_BIND_HOST, port)).is_ok(),
        "WebSocket port {port} was still bound after the server was dropped"
    );
    assert!(
        std::net::UdpSocket::bind(("127.0.0.1", 21934)).is_ok(),
        "the telemetry port was still bound after the server was dropped"
    );
}

#[test]
fn a_port_that_stays_taken_is_reported_rather_than_panicking() {
    let squatter = std::net::TcpListener::bind((DEFAULT_BIND_HOST, 22023))
        .expect("the squatter has to hold the address the server will ask for");

    let error = Server::try_with_ports_and_telemetry(22021, 22022, 22023, 22024).expect_err(
        "the WebSocket port was already bound on the address the server binds, \
                 so this cannot succeed",
    );

    assert!(
        matches!(error, BindError::WebsocketBind { port: 22023, .. }),
        "the error has to name the port, got {error:?}"
    );

    drop(squatter);
}

/// The relay routes a channel to the address its registration arrived from.
///
/// A subscriber cannot learn its own address - its socket is bound to
/// `0.0.0.0` - so any address it could name would be a guess, and the guess
/// that was made was the server's own. That reached a subscriber only on
/// loopback, where the guess happens to be right, which is where every test
/// ran. Registering by datagram takes the address out of the caller's hands:
/// the server reads it off the packet.
#[test]
fn a_subscriber_is_routed_to_wherever_its_registration_came_from() {
    let server = Server::with_ports_and_telemetry(22041, 22042, 22043, 22044);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let relay: SocketAddr = ([127, 0, 0, 1], 22044).into();
    let subscriber = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    subscriber
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let mut buf = [0u8; telemetry::MAX_DATAGRAM];
    let len = telemetry::encode_registration(&mut buf, telemetry::topic_hash("routed"));
    subscriber.send_to(&buf[..len], relay).unwrap();
    std::thread::sleep(Duration::from_millis(200));

    let publisher = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let len = telemetry::encode(
        &mut buf,
        telemetry::topic_hash("routed"),
        telemetry::now_micros(),
        b"payload",
    );
    publisher.send_to(&buf[..len], relay).unwrap();

    let mut received = [0u8; telemetry::MAX_DATAGRAM];
    let (len, _) = subscriber
        .recv_from(&mut received)
        .expect("the relay never reached the address the registration came from");
    assert_eq!(
        telemetry::decode(&received[..len]).map(|(_, _, payload)| payload),
        Some(&b"payload"[..])
    );
}

/// Registration carries no address, so a caller cannot name one.
///
/// It used to name one over REQ/REP, which let anyone aim a channel's whole
/// fan-out at a machine that never asked for it - the server would send
/// traffic on their behalf, to a target of their choosing, at a rate they did
/// not have to generate.
#[test]
fn a_publisher_is_not_registered_by_publishing() {
    let server = Server::with_ports_and_telemetry(22051, 22052, 22053, 22054);
    server.start();
    std::thread::sleep(Duration::from_millis(200));

    let relay: SocketAddr = ([127, 0, 0, 1], 22054).into();
    let publisher = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    publisher
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();

    let mut buf = [0u8; telemetry::MAX_DATAGRAM];
    for _ in 0..3 {
        let len = telemetry::encode(
            &mut buf,
            telemetry::topic_hash("loud"),
            telemetry::now_micros(),
            b"payload",
        );
        publisher.send_to(&buf[..len], relay).unwrap();
    }

    let mut received = [0u8; telemetry::MAX_DATAGRAM];
    assert!(
        publisher.recv_from(&mut received).is_err(),
        "publishing subscribed the publisher, so the relay echoes traffic back \
             to whoever sends it"
    );
    assert!(
        server.telemetry_registry.lock().unwrap().is_empty(),
        "a datagram that was not a registration created one"
    );
}
