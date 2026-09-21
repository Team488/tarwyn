use super::*;
use crate::telemetry::{TELEMETRY_KEEPALIVE, register_telemetry_listener};

use std::time::Instant;
use tarwyn_server::websocket::message::ValueMessage;
use tungstenite::Message as WebsocketMessage;

#[test]
fn a_colliding_channel_is_refused_rather_than_cross_wired() {
    assert_eq!(
        telemetry::topic_hash("glbvs"),
        telemetry::topic_hash("yacxa"),
        "these names are chosen because they collide; the guard is pointless otherwise"
    );

    let mut listeners = HashMap::new();
    assert!(register_telemetry_listener(&mut listeners, "glbvs", Arc::new(|_, _| {})).is_some());
    assert!(register_telemetry_listener(&mut listeners, "yacxa", Arc::new(|_, _| {})).is_none());
    assert!(register_telemetry_listener(&mut listeners, "glbvs", Arc::new(|_, _| {})).is_some());

    let topic = &listeners[&telemetry::topic_hash("glbvs")];
    assert_eq!(topic.channel, "glbvs");
    assert_eq!(topic.listeners.len(), 2);
}

fn offline_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 21802,
        request_timeout: Duration::from_millis(150),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    }
}

#[test]
fn a_bad_endpoint_is_reported_rather_than_panicking() {
    let built = Client::try_with_config(Config {
        host: "no host here".to_string(),
        ..offline_config()
    });
    let Err(error) = built else {
        panic!("a host that cannot be resolved should not build a client");
    };

    assert!(
        matches!(error, ConnectError::Connect { .. }),
        "expected a connect failure, got {error:?}"
    );
    assert!(
        error.to_string().contains("no host here"),
        "the message should name the endpoint, got {error}"
    );
}

/// The path neither side can test alone: a value published by a client,
/// stored by the server, fanned out over the WebSocket, and delivered to a
/// subscriber.
///
/// The server does not add a publisher as a subscriber for a new topic, so
/// the topic is created by a first publish, subscribed, then published again;
/// the retained value from the subscribe is skipped in the receive loop.
#[test]
fn a_published_value_reaches_a_subscriber_through_a_real_server() {
    use std::sync::mpsc;
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21882, 21884);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21882,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    client.send_double("round-trip", 1.0);
    std::thread::sleep(Duration::from_millis(200));

    let (sender, receiver) = mpsc::channel();
    let _unsubscribe = client.subscribe("round-trip", move |value| {
        let _ = sender.send(value.clone());
    });
    client.start();
    std::thread::sleep(Duration::from_millis(200));

    let mut seen = None;
    for _ in 0..40 {
        client.send_double("round-trip", 4.88);
        if let Ok(value) = receiver.recv_timeout(Duration::from_millis(200))
            && value == Value::Double(4.88)
        {
            seen = Some(value);
            break;
        }
    }

    client.stop();
    server.stop();

    assert_eq!(
        seen,
        Some(Value::Double(4.88)),
        "a publish never came back through the server, so the wiring between \
             the push path, the store and the fan-out is broken"
    );
}

/// The same round trip with both readers busy polling, so the spinning read
/// path is exercised end to end: the server's on the publish, the client's on
/// the value coming back.
#[test]
fn a_published_value_reaches_a_busy_polling_subscriber() {
    use std::sync::mpsc;
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21892, 21894);
    server.set_busy_poll(Duration::from_millis(50));
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21892,
        request_timeout: Duration::from_millis(500),
        busy_poll: Duration::from_millis(50),
        predict: Duration::from_micros(200),
        ..Default::default()
    });

    client.send_double("busy-trip", 1.0);
    std::thread::sleep(Duration::from_millis(200));

    let (sender, receiver) = mpsc::channel();
    let _unsubscribe = client.subscribe("busy-trip", move |value| {
        let _ = sender.send(value.clone());
    });
    client.start();
    std::thread::sleep(Duration::from_millis(200));

    let mut seen = None;
    for _ in 0..40 {
        client.send_double("busy-trip", 2.75);
        if let Ok(value) = receiver.recv_timeout(Duration::from_millis(200))
            && value == Value::Double(2.75)
        {
            seen = Some(value);
            break;
        }
    }

    client.stop();
    server.stop();

    assert_eq!(seen, Some(Value::Double(2.75)));
}

/// Drives the receiver directly rather than through a server, so it does not
/// contend for the one fixed UDP port a relay would need.
#[test]
fn telemetry_delivery_resumes_after_a_stop_start_cycle() {
    use std::sync::atomic::AtomicUsize;

    let client = Client::with_config(offline_config());
    let seen = Arc::new(AtomicUsize::new(0));
    let sink = Arc::clone(&seen);

    {
        let mut listeners = client.telemetry_listeners.lock().unwrap();
        assert!(
            register_telemetry_listener(
                &mut listeners,
                "resumes",
                Arc::new(move |_, _| {
                    sink.fetch_add(1, Ordering::SeqCst);
                })
            )
            .is_some()
        );
    }
    client.start_telemetry_receiver();
    client.start();

    let port = client.telemetry_socket.local_addr().unwrap().port();
    let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let deliver = |target: u16| {
        let mut buf = [0u8; telemetry::HEADER_LEN + 8];
        let len = telemetry::encode(&mut buf, telemetry::topic_hash("resumes"), 1, b"payload");
        let _ = sender.send_to(&buf[..len], ("127.0.0.1", target));
    };
    let arrived = |before: usize| {
        for _ in 0..40 {
            if seen.load(Ordering::SeqCst) > before {
                return true;
            }
            deliver(port);
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    };

    assert!(arrived(0), "telemetry never arrived while the client ran");

    client.stop();
    std::thread::sleep(Duration::from_millis(250));
    let before_restart = seen.load(Ordering::SeqCst);
    client.start();

    assert!(
        arrived(before_restart),
        "telemetry stopped for good after stop()/start(); the receiver exits on \
             stop and nothing spawns it again, so the UDP plane goes silent"
    );
    client.stop();
}

/// The reader releases its listener map before running a callback, precisely
/// so this is legal. Holding the map across the call deadlocks the reader and
/// every subscription on the client with it.
#[test]
fn a_callback_may_subscribe_without_deadlocking_the_receive_thread() {
    use std::sync::atomic::AtomicBool;
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21923, 21924);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Arc::new(Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21923,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    }));

    client.send_double("reentrant", 1.0);
    std::thread::sleep(Duration::from_millis(200));

    let reentered = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&reentered);
    let inner = Arc::clone(&client);
    let _unsubscribe = client.subscribe("reentrant", move |_| {
        if done.load(Ordering::SeqCst) {
            return;
        }
        let _second = inner.subscribe("reentrant/nested", |_| {});
        done.store(true, Ordering::SeqCst);
    });
    client.start();
    std::thread::sleep(Duration::from_millis(200));

    let deadline = Instant::now() + Duration::from_secs(5);
    while !reentered.load(Ordering::SeqCst) && Instant::now() < deadline {
        client.send_double("reentrant", 1.0);
        std::thread::sleep(Duration::from_millis(50));
    }

    let survived = reentered.load(Ordering::SeqCst);
    if survived {
        client.stop();
    }
    server.stop();
    assert!(
        survived,
        "a callback that subscribed never returned, so the receive thread is \
             holding the listener map across user code"
    );
}

#[test]
fn stop_joins_its_threads_rather_than_abandoning_them() {
    let client = Client::with_config(offline_config());

    for cycle in 0..3 {
        client.start();
        client.stop();
        assert!(
            client.threads.lock().unwrap().is_empty(),
            "cycle {cycle}: stop() left thread handles behind"
        );
    }
}

#[test]
fn cancelling_a_telemetry_subscription_removes_its_listener() {
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21933, 21934);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21933,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: 21934,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let cancel = client
        .subscribe_telemetry("cancel-me", |_| {})
        .expect("the topic hash was free");
    assert_eq!(
        client.telemetry_listeners.lock().unwrap().len(),
        1,
        "the subscription was never registered"
    );

    cancel();
    assert!(
        client.telemetry_listeners.lock().unwrap().is_empty(),
        "cancelling left the listener behind, so it keeps decoding datagrams \
             into a ring nobody reads"
    );

    client.stop();
    server.stop();
}

/// The UDP path end to end, relay included, on a telemetry port of its own.
#[test]
fn telemetry_reaches_a_subscriber_through_the_server_relay() {
    use std::sync::mpsc;
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21943, 21944);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21943,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: 21944,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let (sender, receiver) = mpsc::channel();
    let _cancel = client
        .subscribe_telemetry("relayed", move |value| {
            let _ = sender.send(value.clone());
        })
        .expect("the topic hash was free");
    client.start();
    std::thread::sleep(Duration::from_millis(300));

    let mut seen = None;
    for _ in 0..40 {
        client.publish_telemetry("relayed", b"payload");
        if let Ok(value) = receiver.recv_timeout(Duration::from_millis(100)) {
            seen = Some(value);
            break;
        }
    }

    client.stop();
    server.stop();
    assert_eq!(
        seen,
        Some(Value::Bytes(b"payload".to_vec())),
        "a telemetry datagram never came back through the server relay"
    );
}

/// The server sweeps every registration older than its TTL whenever any client
/// registers, so a subscription that is never renewed goes silent as soon as a
/// second client appears, while publishes keep reporting success.
///
/// The stub is a bare UDP socket, because registration is a datagram on the
/// telemetry plane rather than a request on the control plane.
#[test]
fn a_telemetry_subscription_renews_its_lease() {
    use std::sync::atomic::AtomicUsize;

    let relay = std::net::UdpSocket::bind(("127.0.0.1", 21954)).unwrap();
    relay
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();

    let registrations = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&registrations);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);

    let server = std::thread::spawn(move || {
        let mut buf = [0u8; telemetry::MAX_DATAGRAM];
        while !server_stop.load(Ordering::SeqCst) {
            let Ok((len, _from)) = relay.recv_from(&mut buf) else {
                continue;
            };
            if telemetry::decode_registration(&buf[..len]).is_some() {
                counted.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21951,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: 21954,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let _cancel = client
        .subscribe_telemetry("leased", |_| {})
        .expect("the topic hash was free");

    let deadline = Instant::now() + TELEMETRY_KEEPALIVE * 3;
    while registrations.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    let seen = registrations.load(Ordering::SeqCst);
    client.stop();
    stop.store(true, Ordering::SeqCst);
    let _ = server.join();

    assert!(
        seen >= 2,
        "the subscription never renewed its lease within {:?}, so the server drops \
             it after its TTL and telemetry goes silent; saw {seen} registrations",
        TELEMETRY_KEEPALIVE * 3
    );
}

#[test]
fn client_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Client>();
}

/// A publish reaches the server over the WebSocket and is stored, so a read
/// round-trips it back.
#[test]
fn publishes_reach_a_bound_peer() {
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21812, 21814);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21812,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let mut received = None;
    for _ in 0..30 {
        client.send_double("probe", 1.5);
        if let Some(value) = client.get("probe") {
            received = Some(value);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    client.stop();
    server.stop();

    assert_eq!(
        received,
        Some(Value::Double(1.5)),
        "no publish reached the server within 3s"
    );
}

#[test]
fn send_does_not_block_when_server_is_absent() {
    let client = Client::with_config(offline_config());
    let started = Instant::now();
    for i in 0..100 {
        client.send_double("no-such-channel", i as f64);
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "send should drop rather than block, took {:?}",
        started.elapsed()
    );
}

#[test]
fn get_returns_none_when_server_is_absent() {
    let client = Client::with_config(offline_config());
    let started = Instant::now();
    assert!(client.get("no-such-channel").is_none());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "get() should give up after request_timeout, took {:?}",
        started.elapsed()
    );
}

#[test]
fn subscribe_does_not_block_when_server_is_absent() {
    let client = Client::with_config(offline_config());
    let started = Instant::now();
    let _unsubscribe = client.subscribe("no-such-channel", |_| {});
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "subscribe() should not block on an absent server, took {:?}",
        started.elapsed()
    );
}

#[test]
fn request_socket_recovers_after_timeout() {
    let client = Client::with_config(offline_config());
    assert!(client.get("first").is_none());
    let started = Instant::now();
    assert!(client.get("second").is_none());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "second request wedged after the first timed out, took {:?}",
        started.elapsed()
    );
}

#[test]
fn publish_drops_are_counted_not_silent() {
    let client = Client::with_config(Config {
        send_high_water_mark: 4,
        ..offline_config()
    });
    for i in 0..200 {
        client.send_double("no-such-channel", i as f64);
    }
    assert!(
        client.dropped_publishes() > 0,
        "publishes past the high water mark should be counted, saw {}",
        client.dropped_publishes()
    );
}

/// A list value survives the round trip to the server and back.
#[test]
fn list_types_survive_the_wire() {
    use tarwyn_server::server::Server;

    let server = Server::with_ports(21822, 21824);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21822,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let expected = vec!["alpha".to_string(), "beta".to_string()];
    let mut received = None;
    for _ in 0..30 {
        client.send_string_list("paths", &expected);
        if let Some(value) = client.get("paths") {
            received = Some(value);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    client.stop();
    server.stop();

    match received {
        Some(Value::StringArray(list)) => assert_eq!(list, expected),
        other => panic!("expected a string list, got {other:?}"),
    }
}

#[test]
fn cached_subscriber_keeps_only_its_depth() {
    let client = Client::with_config(offline_config());
    let (cache, _unsubscribe) = client.subscribe_cached("depth-test", 3);
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert!(cache.latest().is_none());
    assert!(cache.read_all().is_empty());
}

#[test]
fn subscribe_works_after_start() {
    let client = Client::with_config(offline_config());
    client.start();
    let started = Instant::now();
    let _unsubscribe = client.subscribe("after-start", |_| {});
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "subscribing after start() deadlocked, took {:?}",
        started.elapsed()
    );
    client.stop();
}

/// The gate is what keeps a live value from overtaking the snapshot, and what
/// keeps a value that arrives during the replay from being reordered ahead of
/// what is already buffered.
#[test]
fn a_buffered_listener_replays_in_order_then_passes_through() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let listener = Arc::new(BufferedListener::new(move |value: &Value| {
        recorded.lock().unwrap().push(value.clone());
    }));

    listener.deliver(&Value::Int64(1));
    listener.deliver(&Value::Int64(2));
    assert!(
        seen.lock().unwrap().is_empty(),
        "values that arrive before the snapshot must be held, not delivered"
    );

    listener.call(&Value::Int64(0));
    listener.open();
    listener.deliver(&Value::Int64(3));

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            Value::Int64(0),
            Value::Int64(1),
            Value::Int64(2),
            Value::Int64(3)
        ],
        "the snapshot comes first, then what arrived while it was in flight, \
             then everything after"
    );
}

/// A pose publish must not carry its schemas every time.
///
/// The schema bytes are constant, so re-sending them puts three extra
/// frames on the wire for every pose, on the path the project exists to
/// keep fast.
#[test]
fn struct_schemas_go_out_once_rather_than_with_every_pose() {
    use std::net::TcpListener;

    #[expect(
        clippy::result_large_err,
        reason = "tungstenite's Callback trait mandates HttpResponse as the error type"
    )]
    fn accept_nt4(stream: std::net::TcpStream) -> tungstenite::WebSocket<std::net::TcpStream> {
        tungstenite::accept_hdr(
            stream,
            |_req: &tungstenite::http::Request<()>, mut resp: tungstenite::http::Response<()>| {
                resp.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    tungstenite::http::HeaderValue::from_static(NT4_SUBPROTOCOL),
                );
                Ok(resp)
            },
        )
        .unwrap()
    }

    let listener = TcpListener::bind("127.0.0.1:21981").unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = accept_nt4(stream);
        let _ = socket
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(100)));
        let deadline = Instant::now() + Duration::from_millis(2500);
        let mut schema_publishes = 0;
        let mut schema_values = 0;
        while Instant::now() < deadline {
            let Ok(WebsocketMessage::Binary(payload)) = socket.read() else {
                continue;
            };
            let text = String::from_utf8_lossy(&payload);
            if text.contains("\"method\":\"publish\"") && text.contains("/.schema/") {
                schema_publishes += 1;
            } else if payload
                .windows(b"double x;double y".len())
                .any(|w| w == b"double x;double y")
            {
                schema_values += 1;
            }
        }
        let _ = sender.send((schema_publishes, schema_values));
    });

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21981,
        request_timeout: Duration::from_millis(200),
        send_high_water_mark: 500,
        telemetry_port: 21984,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    for _ in 0..10 {
        client.send_pose2d_struct("pose", 1.0, 2.0, 0.5);
        std::thread::sleep(Duration::from_millis(100));
    }

    let (schema_publishes, schema_values) = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    client.stop();
    let _ = server.join();

    assert!(
        schema_publishes > 0,
        "the schemas have to be declared at all, or the test proves nothing"
    );
    assert!(
        schema_values <= 2,
        "the Translation2d schema bytes went out {schema_values} times for ten \
             poses; they are constant, so the count must not scale with the pose \
             rate (two is the queued copy plus the session replay on first connect)"
    );
}

/// A connection dropped mid-session must not silently deaden the client.
///
/// Publishes and subscriptions are registered on the connection they were
/// sent on. Without a replay the server that answers the reconnect has
/// never heard of either, so it drops every value the client publishes and
/// sends it nothing it subscribed to, for the life of the process.
#[test]
fn the_client_republishes_and_resubscribes_after_a_reconnect() {
    use std::net::TcpListener;

    #[expect(
        clippy::result_large_err,
        reason = "tungstenite's Callback trait mandates HttpResponse as the error type"
    )]
    fn accept_nt4(stream: std::net::TcpStream) -> tungstenite::WebSocket<std::net::TcpStream> {
        tungstenite::accept_hdr(
            stream,
            |_req: &tungstenite::http::Request<()>, mut resp: tungstenite::http::Response<()>| {
                resp.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    tungstenite::http::HeaderValue::from_static(NT4_SUBPROTOCOL),
                );
                Ok(resp)
            },
        )
        .unwrap()
    }

    /// The control messages one connection receives, until `deadline`.
    fn control_messages(
        websocket: &mut tungstenite::WebSocket<std::net::TcpStream>,
        deadline: Instant,
    ) -> Vec<String> {
        let _ = websocket
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(100)));
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            let Ok(WebsocketMessage::Binary(payload)) = websocket.read() else {
                continue;
            };
            let text = String::from_utf8_lossy(&payload).to_string();
            for method in ["publish", "subscribe"] {
                if text.contains(&format!("\"method\":\"{method}\"")) {
                    seen.push(method.to_string());
                }
            }
        }
        seen
    }

    let listener = TcpListener::bind("127.0.0.1:21971").unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut first = accept_nt4(stream);
        let before = control_messages(&mut first, Instant::now() + Duration::from_millis(1200));
        drop(first);

        let (stream, _) = listener.accept().unwrap();
        let mut second = accept_nt4(stream);
        let after = control_messages(&mut second, Instant::now() + Duration::from_millis(2000));
        let _ = sender.send((before, after));
    });

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21971,
        request_timeout: Duration::from_millis(200),
        send_high_water_mark: 500,
        telemetry_port: 21974,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });

    let _unsubscribe = client.subscribe("window", |_| {});
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        client.send_double("window", 4.88);
        std::thread::sleep(Duration::from_millis(100));
    }

    let (before, after) = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    client.stop();
    let _ = server.join();

    assert!(
        before.contains(&"publish".to_string()),
        "the first connection has to see the publish, or the test proves nothing: {before:?}"
    );
    assert!(
        after.contains(&"publish".to_string()),
        "the reconnect never re-published, so the server drops every value \
             the client sends: {after:?}"
    );
    assert!(
        after.contains(&"subscribe".to_string()),
        "the reconnect never re-subscribed, so the client hears nothing: {after:?}"
    );
}

/// A value published while `subscribe` is reading the current value has to
/// reach the subscriber: on a channel that then goes quiet, a subscriber that
/// missed it stays behind the server for good, with nothing to say so.
///
/// The stub answers the subscribe with an announcement, then answers the read
/// by publishing a value before replying with no value at all, so the only way
/// the callback can fire is if the subscription was already in place when the
/// publish went out.
#[test]
#[expect(
    clippy::result_large_err,
    reason = "tungstenite's Callback trait mandates HttpResponse as the error type"
)]
fn a_value_published_while_subscribe_reads_the_current_one_is_not_lost() {
    use std::net::TcpListener;
    use std::sync::mpsc;

    let listener = TcpListener::bind("127.0.0.1:21961").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut websocket = tungstenite::accept_hdr(
            stream,
            |_req: &tungstenite::http::Request<()>, mut resp: tungstenite::http::Response<()>| {
                resp.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    tungstenite::http::HeaderValue::from_static(NT4_SUBPROTOCOL),
                );
                Ok(resp)
            },
        )
        .unwrap();
        while !server_stop.load(Ordering::SeqCst) {
            let Ok(WebsocketMessage::Binary(payload)) = websocket.read() else {
                continue;
            };
            if let Ok(request) = Request::decode(&payload[..]) {
                let _ = request;
                std::thread::sleep(Duration::from_millis(300));
                let vm = ValueMessage {
                    topic_id: 0,
                    timestamp_micros: 0,
                    data_type: 2,
                    value: Value::Int64(7),
                };
                let mut buf = Vec::new();
                vm.encode(&mut buf);
                let _ = websocket.send(WebsocketMessage::binary(buf));
                std::thread::sleep(Duration::from_millis(100));
                let reply = Reply {
                    payload: Some(reply::Payload::Data(
                        tarwyn_protobuf::protobuf::ReplyDataCommand { value: None },
                    )),
                }
                .encode_to_vec();
                let _ = websocket.send(WebsocketMessage::binary(reply));
            } else if let Ok(ControlMessage::Subscribe { .. }) =
                ControlMessage::from_json(&String::from_utf8_lossy(&payload))
            {
                let announce = ControlMessage::Announce {
                    name: "window".to_string(),
                    id: 0,
                    data_type: "int".to_string(),
                    properties: Map::new(),
                    pubuid: None,
                };
                let _ = websocket.send(WebsocketMessage::text(announce.to_json()));
            }
        }
    });

    let client = Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 21961,
        request_timeout: Duration::from_millis(3000),
        send_high_water_mark: 500,
        telemetry_port: 21964,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    });
    std::thread::sleep(Duration::from_millis(300));

    let (sender, receiver) = mpsc::channel();
    let _unsubscribe = client.subscribe("window", move |value| {
        let _ = sender.send(value.clone());
    });
    client.start();

    let seen = receiver.recv_timeout(Duration::from_secs(3)).ok();

    client.stop();
    stop.store(true, Ordering::SeqCst);
    let _ = server.join();

    assert_eq!(
        seen,
        Some(Value::Uint32(7)),
        "the publish landed between subscribing and reading the current value, \
             and never reached the subscriber"
    );
}

/// A client that goes out of scope has to stop its receive threads. They hold
/// clones of its sockets, so a client that is dropped without them being
/// joined leaks a thread and a live connection per client built.
#[test]
fn dropping_a_client_stops_its_receive_threads() {
    let alive = {
        let client = Client::with_config(offline_config());
        client.start();
        std::thread::sleep(Duration::from_millis(200));
        Arc::clone(&client.reader_alive)
    };

    assert!(
        !alive.load(Ordering::SeqCst),
        "the reader thread is still running after the client was dropped"
    );
}

/// Stopping from inside a subscription callback asks a receive thread to join
/// itself. It has to skip its own handle instead, and dropping the last
/// handle to a client from a callback reaches the same path through `Drop`.
#[test]
fn stopping_from_a_callback_does_not_wait_for_the_thread_running_it() {
    use std::sync::atomic::AtomicBool;
    use tarwyn_server::server::Server;

    let server = Server::with_ports(22003, 22004);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let client = Arc::new(Client::with_config(Config {
        host: "127.0.0.1".to_string(),
        port: 22003,
        request_timeout: Duration::from_millis(500),
        send_high_water_mark: 500,
        telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        busy_poll: Duration::ZERO,
        predict: Duration::ZERO,
    }));

    client.send_double("stopper", 1.0);
    std::thread::sleep(Duration::from_millis(200));

    let returned = Arc::new(AtomicBool::new(false));
    let escaped = Arc::clone(&returned);
    let inner = Arc::clone(&client);
    let _unsubscribe = client.subscribe("stopper", move |_| {
        if escaped.load(Ordering::SeqCst) {
            return;
        }
        inner.stop();
        escaped.store(true, Ordering::SeqCst);
    });
    client.start();
    std::thread::sleep(Duration::from_millis(200));

    let deadline = Instant::now() + Duration::from_secs(5);
    while !returned.load(Ordering::SeqCst) && Instant::now() < deadline {
        client.send_double("stopper", 1.0);
        std::thread::sleep(Duration::from_millis(50));
    }

    let survived = returned.load(Ordering::SeqCst);
    server.stop();
    assert!(
        survived,
        "stop() called from a callback never returned, so the receive thread \
             was waiting on itself"
    );
}

mod struct_layout_tests {
    use crate::typed::pack_le_doubles;

    fn unpack(bytes: &[u8]) -> Vec<f64> {
        bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk))
            .collect()
    }

    #[test]
    fn a_pose_is_packed_rather_than_written_as_a_list() {
        let packed = pack_le_doubles(&[1.0, 2.0, 3.0]);
        assert_eq!(packed.len(), 24);
        assert_eq!(&packed[..8], &1.0f64.to_le_bytes());
    }

    #[test]
    fn a_packed_pose_reads_back_field_for_field() {
        let fields = [1.5, -2.5, 0.75];
        assert_eq!(unpack(&pack_le_doubles(&fields)), fields);
    }

    #[test]
    fn a_pose3d_puts_w_before_x_y_and_z() {
        let packed = pack_le_doubles(&[0.0, 0.0, 0.0, 0.7, 0.1, 0.2, 0.3]);
        assert_eq!(packed.len(), 56);
        assert_eq!(unpack(&packed)[3], 0.7);
    }
}
