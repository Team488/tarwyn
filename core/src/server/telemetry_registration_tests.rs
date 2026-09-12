use super::*;

fn register(server: &Server, socket: &std::net::UdpSocket, hash: u32, to: SocketAddr) {
    let mut buf = [0u8; telemetry::HEADER_LEN];
    let n = telemetry::encode_registration(&mut buf, hash);
    socket.send_to(&buf[..n], to).unwrap();
    let _ = server;
}

#[test]
fn one_datagram_is_not_amplified_past_the_subscriber_cap() {
    let fanout = MAX_TELEMETRY_SUBSCRIBERS * 4;
    let server = Server::with_ports_and_telemetry(22201, 22202, 22203, 22204);
    server.start();
    std::thread::sleep(Duration::from_millis(400));

    let relay: SocketAddr = "127.0.0.1:22204".parse().unwrap();
    let hash = telemetry::topic_hash("amplify");

    // One host, many source ports: without a cap each is its own subscriber
    // and the relay copies every datagram to all of them.
    let sockets: Vec<std::net::UdpSocket> = (0..fanout)
        .map(|_| {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.set_nonblocking(true).unwrap();
            register(&server, &socket, hash, relay);
            socket
        })
        .collect();
    std::thread::sleep(Duration::from_millis(300));

    let mut datagram = vec![0u8; telemetry::HEADER_LEN + 64];
    let n = telemetry::encode(&mut datagram, hash, 0, &[7u8; 64]);
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .send_to(&datagram[..n], relay)
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    let mut copies = 0;
    let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
    for socket in &sockets {
        if socket.recv_from(&mut buf).is_ok() {
            copies += 1;
        }
    }
    server.stop();

    assert!(
        copies <= MAX_TELEMETRY_SUBSCRIBERS,
        "one datagram reached {copies} addresses, past the cap of \
             {MAX_TELEMETRY_SUBSCRIBERS}"
    );
    assert!(
        copies > 0,
        "the relay has to still deliver to real subscribers"
    );
}

#[test]
fn a_subscriber_keeps_its_slot_when_the_channel_is_full() {
    let registry = Mutex::new(HashMap::new());
    let published = ArcSwap::from_pointee(HashMap::new());
    let address = |port: u16| SocketAddr::from(([127, 0, 0, 1], port));

    for port in 0..MAX_TELEMETRY_SUBSCRIBERS as u16 {
        assert!(Server::register_telemetry(
            &registry,
            &published,
            7,
            address(9000 + port)
        ));
    }
    assert!(
        !Server::register_telemetry(&registry, &published, 7, address(9999)),
        "a full channel has to turn a new address away"
    );
    assert!(
        Server::register_telemetry(&registry, &published, 7, address(9000)),
        "an address already holding a slot has to be able to renew its lease"
    );
}
