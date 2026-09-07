//! Telemetry-plane subject: measures the UDP fan-out path the server serves on
//! 5809, publisher through relay to subscriber.
//!
//! This plane is best effort. Nothing is retransmitted, nothing is ordered, and
//! a datagram lost on the way is simply gone, so its row belongs beside
//! `udp-floor` rather than beside the NT4 subjects, which are reliable streams.
//!
//! A subscriber registers by sending a registration datagram; the relay routes
//! the channel to the source address of that datagram and holds the lease for
//! `TELEMETRY_TTL`, so a run longer than the lease has to re-register.

use crate::harness::{HEADER_LEN, Pacer, Recorder, SendStats, decode, encode, now_nanos};
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use tarwyn_protobuf::telemetry;

/// The channel both sides publish and subscribe to.
const CHANNEL: &str = "bench";

/// How often the subscriber refreshes its lease, well inside the server's TTL.
const REREGISTER: Duration = Duration::from_secs(3);

fn relay_addr(host: &str) -> String {
    if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:{}", telemetry::DEFAULT_TELEMETRY_PORT)
    }
}

pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let payload = payload.max(HEADER_LEN);
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.connect(relay_addr(host))?;

    let mut sample = vec![0u8; payload];
    let mut datagram = vec![0u8; telemetry::HEADER_LEN + payload];
    let channel = telemetry::topic_hash(CHANNEL);
    let mut pacer = Pacer::new(rate_hz);
    let mut stats = SendStats::new(pacer.interval_nanos());

    for seq in 0..count {
        let due = pacer.wait();
        encode(&mut sample, seq, due);
        let len = telemetry::encode(&mut datagram, channel, due / 1000, &sample);
        let entered = now_nanos();
        let started = Instant::now();
        let result = socket.send(&datagram[..len]);
        stats.record(due, entered, started.elapsed());
        match result {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                println!("relay closed after {seq} messages");
                stats.report(seq);
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }
    println!("sent {count} messages of {payload} B");
    stats.report(count);
    Ok(())
}

pub fn subscribe(host: &str, payload: usize, samples: u64) -> std::io::Result<()> {
    let payload = payload.max(HEADER_LEN);
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let relay = relay_addr(host);
    let channel = telemetry::topic_hash(CHANNEL);

    let mut registration = vec![0u8; telemetry::HEADER_LEN];
    let len = telemetry::encode_registration(&mut registration, channel);
    socket.send_to(&registration[..len], &relay)?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
    let mut recorder = Recorder::new();
    println!("registered for '{CHANNEL}' on {relay}, waiting for {samples} samples...");

    let deadline = Instant::now() + Duration::from_secs(deadline_secs());
    let mut renew = Instant::now() + REREGISTER;
    while recorder.len() < samples {
        recorder.close_elapsed_windows();
        let now = Instant::now();
        if now > deadline {
            println!("timed out with {}/{} samples", recorder.len(), samples);
            break;
        }
        if now > renew {
            socket.send_to(&registration[..len], &relay)?;
            renew = now + REREGISTER;
        }
        let received = match socket.recv(&mut buf) {
            Ok(received) => received,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        if let Some((_channel, _timestamp, sample)) = telemetry::decode(&buf[..received])
            && let Some((seq, sent)) = decode(sample)
        {
            recorder.record(seq, sent);
        }
    }
    recorder.report(
        &format!("tarwyn-rust telemetry v{}", env!("CARGO_PKG_VERSION")),
        payload,
    );
    Ok(())
}

fn deadline_secs() -> u64 {
    std::env::var("BENCH_DEADLINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
}
