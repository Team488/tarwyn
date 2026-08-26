use crate::harness::{HEADER_LEN, Pacer, Recorder, decode, encode};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tarwyn_client::tarwyn_client::{TarwynClient, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values;

pub const CHANNEL: &str = "bench-udp";

fn client(host: &str) -> TarwynClient {
    TarwynClient::with_config(TarwynConfig {
        host: host.to_string(),
        request_timeout: Duration::from_millis(250),
        send_high_water_mark: std::env::var("XT_HWM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500),
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
        client.publish_telemetry(CHANNEL, &buf);
    }
    println!("sent {count} messages of {} B", buf.len());
    Ok(())
}

pub fn subscribe(
    host: &str,
    payload: usize,
    samples: u64,
    window_secs: u64,
    duration_secs: u64,
) -> std::io::Result<()> {
    let mut base = Recorder::new();
    if window_secs > 0 {
        base = base.with_window(Duration::from_secs(window_secs));
    }
    let recorder = Arc::new(Mutex::new(base));

    let client = client(host);
    let sink = Arc::clone(&recorder);
    client.subscribe_telemetry(CHANNEL, move |value| {
        if let supported_values::Kind::Bytes(bytes) = value
            && let Some((seq, sent)) = decode(bytes)
            && let Ok(mut recorder) = sink.lock()
        {
            recorder.record(seq, sent);
        }
    });

    println!("subscribed to '{CHANNEL}' on {host}, waiting for {samples} samples...");
    let deadline = std::time::Instant::now() + Duration::from_secs(duration_secs);
    loop {
        if let Ok(mut recorder) = recorder.lock()
            && let Some(row) = recorder.take_window_row()
        {
            println!("{row}");
        }
        let received = recorder.lock().map(|r| r.len()).unwrap_or(0);
        if window_secs == 0 && received >= samples {
            break;
        }
        if std::time::Instant::now() > deadline {
            if window_secs == 0 {
                println!("timed out with {received}/{samples} samples");
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    if let Ok(recorder) = recorder.lock() {
        recorder.report(
            &format!("tarwyn-rust v{}", env!("CARGO_PKG_VERSION")),
            payload.max(HEADER_LEN),
        );
    }
    client.stop();
    Ok(())
}
