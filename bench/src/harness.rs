use hdrhistogram::Histogram;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bytes of sequence number and timestamp ahead of every sample's padding.
pub const HEADER_LEN: usize = 16;

/// Nanoseconds since the Unix epoch.
pub fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_nanos() as u64
}

/// Stamp a sample with its sequence number and the current time.
pub fn encode(buf: &mut [u8], seq: u64) {
    buf[0..8].copy_from_slice(&seq.to_le_bytes());
    buf[8..16].copy_from_slice(&now_nanos().to_le_bytes());
}

/// Read a sample's sequence number and send time. `None` if the buffer is too short.
pub fn decode(buf: &[u8]) -> Option<(u64, u64)> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    let seq = u64::from_le_bytes(buf[0..8].try_into().ok()?);
    let sent = u64::from_le_bytes(buf[8..16].try_into().ok()?);
    Some((seq, sent))
}

/// Records one-way latencies into an HDR histogram, tracking loss by sequence gap.
///
/// The first `WARMUP` samples are discarded, so a JIT-compiled or cold subject is
/// not measured while it is still warming up.
pub struct Recorder {
    hist: Histogram<u64>,
    warmup: u64,
    discarded: u64,
    received: u64,
    highest_seq: Option<u64>,
    first_seq: Option<u64>,
    gaps: u64,
    reordered: u64,
}

impl Recorder {
    /// A recorder with no windowing.
    pub fn new() -> Self {
        Recorder {
            hist: Histogram::new_with_bounds(1, 60_000_000_000, 3)
                .expect("histogram bounds are valid"),
            warmup: std::env::var("BENCH_WARMUP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500),
            discarded: 0,
            received: 0,
            highest_seq: None,
            first_seq: None,
            gaps: 0,
            reordered: 0,
        }
    }

    /// Record a sample, deriving its latency from the send time it carries.
    pub fn record(&mut self, seq: u64, sent_nanos: u64) {
        let latency = now_nanos().saturating_sub(sent_nanos);
        self.record_measured(seq, latency);
    }

    fn record_measured(&mut self, seq: u64, latency: u64) {
        if self.discarded < self.warmup {
            self.discarded += 1;
            self.highest_seq = Some(seq);
            return;
        }
        self.hist.saturating_record(latency);
        self.received += 1;
        if self.first_seq.is_none() {
            self.first_seq = Some(seq);
        }

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

    /// How many samples were recorded after warmup.
    pub fn len(&self) -> u64 {
        self.received
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.received == 0
    }

    /// Print the percentile row for this subject.
    pub fn report(&self, subject: &str, payload: usize) {
        if self.is_empty() {
            println!("{subject} @ {payload}B: no samples received");
            return;
        }
        let us = |v: u64| v as f64 / 1000.0;
        let sent = self.received + self.gaps;
        let loss = if sent == 0 {
            0.0
        } else {
            100.0 * self.gaps as f64 / sent as f64
        };
        println!(
            "ROW\t{subject}\t{payload}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
            us(self.hist.value_at_quantile(0.50)),
            us(self.hist.min()),
            us(self.hist.value_at_quantile(0.80)),
            us(self.hist.value_at_quantile(0.90)),
            us(self.hist.value_at_quantile(0.95)),
            us(self.hist.max()),
            loss
        );
        println!("subject      {subject}");
        println!("payload      {payload} B");
        println!("received     {}", self.received);
        println!("dropped      {} (gaps in sequence)", self.gaps);
        println!(
            "first seq    {} (loss before this point is startup, not congestion)",
            self.first_seq.unwrap_or(0)
        );
        println!("reordered    {}", self.reordered);
        println!(
            "median       {:>9.2} us",
            us(self.hist.value_at_quantile(0.50))
        );
        println!("p0           {:>9.2} us", us(self.hist.min()));
        println!(
            "p80          {:>9.2} us",
            us(self.hist.value_at_quantile(0.80))
        );
        println!(
            "p90          {:>9.2} us",
            us(self.hist.value_at_quantile(0.90))
        );
        println!(
            "p95          {:>9.2} us",
            us(self.hist.value_at_quantile(0.95))
        );
        println!("p100         {:>9.2} us", us(self.hist.max()));
        println!("loss         {:>9.2} %", loss);
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Paces a send loop at a fixed rate.
///
/// Sleeps the bulk of the interval and spins the sub-millisecond remainder, since
/// a bare sleep overshoots by enough to distort the measurement.
#[derive(Debug)]
pub struct Pacer {
    interval: Duration,
    next: Instant,
}

impl Pacer {
    /// A pacer running at `rate_hz`.
    ///
    /// Paced against [`Instant`], so a clock step cannot stretch or collapse the
    /// send rate the way it would on [`SystemTime`].
    pub fn new(rate_hz: u64) -> Self {
        Pacer {
            interval: Duration::from_nanos(1_000_000_000 / rate_hz.max(1)),
            next: Instant::now(),
        }
    }

    /// Block until the next send is due.
    pub fn wait(&mut self) {
        self.next += self.interval;
        loop {
            let Some(remaining) = self.next.checked_duration_since(Instant::now()) else {
                return; // deadline already passed
            };
            if remaining.is_zero() {
                return;
            }
            if remaining > Duration::from_millis(1) {
                std::thread::sleep(remaining - Duration::from_millis(1));
            } else {
                std::hint::spin_loop();
            }
        }
    }
}
