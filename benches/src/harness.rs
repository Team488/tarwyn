use hdrhistogram::Histogram;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HEADER_LEN: usize = 16;

pub fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_nanos() as u64
}

pub fn encode(buf: &mut [u8], seq: u64) {
    buf[0..8].copy_from_slice(&seq.to_le_bytes());
    buf[8..16].copy_from_slice(&now_nanos().to_le_bytes());
}

pub fn decode(buf: &[u8]) -> Option<(u64, u64)> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    let seq = u64::from_le_bytes(buf[0..8].try_into().ok()?);
    let sent = u64::from_le_bytes(buf[8..16].try_into().ok()?);
    Some((seq, sent))
}

pub struct Recorder {
    hist: Histogram<u64>,
    received: u64,
    highest_seq: Option<u64>,
    gaps: u64,
    reordered: u64,
}

impl Recorder {
    pub fn new() -> Self {
        Recorder {
            hist: Histogram::new_with_bounds(1, 60_000_000_000, 3)
                .expect("histogram bounds are valid"),
            received: 0,
            highest_seq: None,
            gaps: 0,
            reordered: 0,
        }
    }

    pub fn record(&mut self, seq: u64, sent_nanos: u64) {
        let latency = now_nanos().saturating_sub(sent_nanos);
        self.hist.saturating_record(latency);
        self.received += 1;

        match self.highest_seq {
            None => {}
            Some(highest) if seq > highest + 1 => self.gaps += seq - highest - 1,
            Some(highest) if seq <= highest => self.reordered += 1,
            Some(_) => {}
        }
        if self.highest_seq.is_none_or(|h| seq > h) {
            self.highest_seq = Some(seq);
        }
    }

    pub fn len(&self) -> u64 {
        self.received
    }

    pub fn is_empty(&self) -> bool {
        self.received == 0
    }

    pub fn report(&self, subject: &str, payload: usize) {
        if self.is_empty() {
            println!("{subject} @ {payload}B: no samples received");
            return;
        }
        let us = |v: u64| v as f64 / 1000.0;
        println!("subject      {subject}");
        println!("payload      {payload} B");
        println!("received     {}", self.received);
        println!("dropped      {} (gaps in sequence)", self.gaps);
        println!("reordered    {}", self.reordered);
        println!("p50          {:>9.2} us", us(self.hist.value_at_quantile(0.50)));
        println!("p99          {:>9.2} us", us(self.hist.value_at_quantile(0.99)));
        println!("p999         {:>9.2} us", us(self.hist.value_at_quantile(0.999)));
        println!("max          {:>9.2} us", us(self.hist.max()));
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Pacer {
    interval: Duration,
    next: SystemTime,
}

impl Pacer {
    pub fn new(rate_hz: u64) -> Self {
        Pacer {
            interval: Duration::from_nanos(1_000_000_000 / rate_hz.max(1)),
            next: SystemTime::now(),
        }
    }

    pub fn wait(&mut self) {
        self.next += self.interval;
        loop {
            let Ok(remaining) = self.next.duration_since(SystemTime::now()) else {
                return; // deadline already passed
            };
            if remaining > Duration::from_millis(1) {
                std::thread::sleep(remaining - Duration::from_millis(1));
            } else {
                std::hint::spin_loop();
            }
        }
    }
}
