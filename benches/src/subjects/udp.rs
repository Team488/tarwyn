
use crate::harness::{HEADER_LEN, Pacer, Recorder, decode, encode};
use std::net::UdpSocket;

pub const DEFAULT_ADDR: &str = "127.0.0.1:48810";

pub const MAX_DATAGRAM: usize = 65_507;

fn check_payload(payload: usize) -> std::io::Result<usize> {
    let payload = payload.max(HEADER_LEN);
    if payload > MAX_DATAGRAM {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "payload {payload} B exceeds the {MAX_DATAGRAM} B UDP datagram limit; \
                 payloads this large belong on the bulk (TCP) plane"
            ),
        ));
    }
    Ok(payload)
}

pub fn publish(addr: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let payload = check_payload(payload)?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.connect(addr)?;

    let mut buf = vec![0u8; payload];
    let mut pacer = Pacer::new(rate_hz);

    for seq in 0..count {
        pacer.wait();
        encode(&mut buf, seq);
        match socket.send(&buf) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                println!("receiver closed after {seq} messages");
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }
    println!("sent {count} messages of {} B", buf.len());
    Ok(())
}

pub fn subscribe(addr: &str, payload: usize, samples: u64) -> std::io::Result<()> {
    let payload = check_payload(payload)?;
    let socket = UdpSocket::bind(addr)?;
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut recorder = Recorder::new();

    println!("listening on {addr}, waiting for {samples} samples...");
    while recorder.len() < samples {
        let received = socket.recv(&mut buf)?;
        if let Some((seq, sent)) = decode(&buf[..received]) {
            recorder.record(seq, sent);
        }
    }
    recorder.report("udp-floor", payload);
    Ok(())
}
