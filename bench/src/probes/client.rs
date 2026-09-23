//! Client-library probes: the Rust client on one side, a raw NT4 probe on the
//! other, so each row differs from the raw one by one thing.

use crate::harness::{HEADER_LEN, Pacer, Recorder, RowId, SendStats, decode, encode, now_nanos};
use std::time::{Duration, Instant};
use tarwyn_client::{Client, Value};

/// The channel the value is published on, matching the NetworkTables probe's topic.
const CHANNEL: &str = "bench";

/// Publish `count` paced samples of `payload` bytes through the client, after
/// the first publish has announced the topic.
pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> anyhow::Result<()> {
    let payload = payload.max(HEADER_LEN);
    let client = Client::connect(host);

    let mut buf = vec![0u8; payload];
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

/// Receive `samples` through `subscribers` clients on one topic, stamped in
/// the callback.
///
/// With several subscribers a value counts once, at the slowest. A value one
/// never gets is forgotten after [`STRAGGLER_WINDOW`] newer ones.
pub fn subscribe(
    host: &str,
    payload: usize,
    samples: u64,
    id: &RowId,
    subscribers: usize,
) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let clients: Vec<Client> = (0..subscribers.max(1))
        .map(|_| {
            let client = Client::connect(host);
            let tx = tx.clone();
            let _cancel = client.subscribe(CHANNEL, move |value| {
                if let Value::Bytes(data) = value
                    && let Some((seq, sent)) = decode(data)
                {
                    let _ = tx.send((seq, now_nanos().saturating_sub(sent)));
                }
            });
            client.start();
            client
        })
        .collect();
    drop(tx);

    let mut recorder = Recorder::new();
    let mut pending: std::collections::BTreeMap<u64, (usize, u64)> = Default::default();
    println!(
        "subscribed to '{CHANNEL}' on {host} with {} subscriber(s), waiting for {samples} samples...",
        clients.len()
    );
    let deadline = Instant::now() + Duration::from_secs(super::networktables::deadline_secs());
    while recorder.len() < samples {
        recorder.close_elapsed_windows();
        if Instant::now() > deadline {
            println!("timed out with {}/{} samples", recorder.len(), samples);
            break;
        }
        let Ok((seq, latency)) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let entry = pending.entry(seq).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.max(latency);
        if entry.0 >= clients.len() {
            let (_, slowest) = pending.remove(&seq).unwrap_or((0, latency));
            recorder.record_latency(seq, slowest);
        }
        let stale = seq.saturating_sub(STRAGGLER_WINDOW);
        pending.retain(|&waiting, _| waiting >= stale);
    }
    recorder.report(id, payload.max(HEADER_LEN));
    Ok(())
}

/// How far behind the newest sequence number a value may fall before the
/// subscribers still missing it are given up on.
const STRAGGLER_WINDOW: u64 = 1000;
