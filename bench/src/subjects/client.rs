//! Client-library subject: measures what a user of the Rust client actually
//! gets, publishing through `TarwynClient` rather than straight onto a socket.
//!
//! Every other subject drives the wire directly, which measures the server but
//! says nothing about the path a robot's code takes to reach it. The subscriber
//! is the ordinary NT4 one, so the only difference from `tarwyn-rust` is who
//! writes the value.

use crate::harness::{HEADER_LEN, Pacer, SendStats, encode, now_nanos};
use std::time::Instant;
use tarwyn_client::TarwynClient;

/// The channel the value is published on, matching the NT4 subject's topic.
const CHANNEL: &str = "bench";

pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let payload = payload.max(HEADER_LEN);
    let client = TarwynClient::connect(host);

    let mut buf = vec![0u8; payload];
    // The first publish announces the topic; give the server time to answer
    // before any sample is timed, exactly as the NT4 subject does.
    client.send_bytes(CHANNEL, &buf);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let mut pacer = Pacer::new(rate_hz);
    let mut stats = SendStats::new(pacer.interval_nanos());
    for seq in 0..count {
        let due = pacer.wait();
        encode(&mut buf, seq, due);
        let entered = now_nanos();
        let started = Instant::now();
        client.send_bytes(CHANNEL, &buf);
        stats.record(due, entered, started.elapsed());
    }
    println!("sent {count} messages of {payload} B through the client");
    stats.report(count);
    Ok(())
}
