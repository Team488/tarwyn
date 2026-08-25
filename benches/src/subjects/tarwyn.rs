use crate::harness::{HEADER_LEN, Pacer, Recorder, decode, encode};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tarwyn_client::tarwyn_client::{TarwynClient, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values;

pub const CHANNEL: &str = "bench";

fn client(host: &str) -> TarwynClient {
    TarwynClient::with_config(TarwynConfig {
        host: host.to_string(),
        request_timeout: Duration::from_millis(250),
        ..Default::default()
    })
}

pub fn publish(host: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let client = client(host);
    client.start();

    let mut buf = vec![0u8; payload.max(HEADER_LEN)];
    let mut pacer = Pacer::new(rate_hz);

    std::thread::sleep(Duration::from_millis(500));

    for seq in 0..count {
        pacer.wait();
        encode(&mut buf, seq);
        client.send_bytes(CHANNEL, &buf);
    }
    println!("sent {count} messages of {} B", buf.len());
    Ok(())
}

pub fn subscribe(host: &str, payload: usize, samples: u64) -> std::io::Result<()> {
    let recorder = Arc::new(Mutex::new(Recorder::new()));

    let client = client(host);
    let sink = Arc::clone(&recorder);
    let _unsubscribe = client.subscribe(CHANNEL, move |value| {
        if let supported_values::Kind::Bytes(bytes) = value {
            if let Some((seq, sent)) = decode(bytes) {
                if let Ok(mut recorder) = sink.lock() {
                    recorder.record(seq, sent);
                }
            }
        }
    });
    client.start();

    println!("subscribed to '{CHANNEL}' on {host}, waiting for {samples} samples...");
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let received = recorder.lock().map(|r| r.len()).unwrap_or(0);
        if received >= samples {
            break;
        }
        if std::time::Instant::now() > deadline {
            println!("timed out with {received}/{samples} samples");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    if let Ok(recorder) = recorder.lock() {
        recorder.report("tarwyn-rust", payload.max(HEADER_LEN));
    }
    client.stop();
    Ok(())
}
