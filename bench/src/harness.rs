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

/// Stamp a sample with its sequence number and the time it was due to be sent.
///
/// `sent_nanos` is the pacer's intended send time, not the time the send actually
/// happened. Stamping the actual time is coordinated omission: a send delayed by
/// a stalled transport would record only its own short flight, and the delay it
/// waited out would never appear in any sample.
pub fn encode(buf: &mut [u8], seq: u64, sent_nanos: u64) {
    buf[0..8].copy_from_slice(&seq.to_le_bytes());
    buf[8..16].copy_from_slice(&sent_nanos.to_le_bytes());
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

fn new_histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(1, 60_000_000_000, 3).expect("histogram bounds are valid")
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

/// One reporting window of a soak run.
///
/// Windows are reported as they close, so a run that degrades halfway through
/// shows it in the rows rather than averaging it away over the whole run.
struct WindowState {
    hist: Histogram<u64>,
    secs: u64,
    ends: Instant,
    index: u64,
    received: u64,
    lost: u64,
}

impl WindowState {
    fn new(secs: u64) -> Self {
        WindowState {
            hist: new_histogram(),
            secs: secs.max(1),
            ends: Instant::now() + Duration::from_secs(secs.max(1)),
            index: 0,
            received: 0,
            lost: 0,
        }
    }

    fn report(&self) {
        let us = |v: u64| v as f64 / 1000.0;
        println!(
            "WINDOW\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{}",
            self.index,
            self.received,
            us(self.hist.value_at_quantile(0.50)),
            us(self.hist.value_at_quantile(0.95)),
            us(self.hist.max()),
            self.lost
        );
    }

    fn roll(&mut self) {
        self.index += 1;
        self.received = 0;
        self.lost = 0;
        self.hist.reset();
        self.ends += Duration::from_secs(self.secs);
    }
}

/// Records one-way latencies into an HDR histogram, tracking loss by sequence gap.
///
/// The first `WARMUP` samples are discarded, so a JIT-compiled or cold subject is
/// not measured while it is still warming up. Setting `BENCH_WINDOW_SECS` also
/// reports a `WINDOW` row every that many seconds, which is what `soak.sh` reads.
pub struct Recorder {
    hist: Histogram<u64>,
    corrected: Histogram<u64>,
    expected_interval: u64,
    first_at: Option<Instant>,
    last_at: Option<Instant>,
    warmup: u64,
    discarded: u64,
    received: u64,
    highest_seq: Option<u64>,
    first_seq: Option<u64>,
    gaps: u64,
    reordered: u64,
    window: Option<WindowState>,
    achieved_hz_override: Option<f64>,
}

impl Recorder {
    /// A recorder windowed by `BENCH_WINDOW_SECS`, unwindowed when it is unset.
    ///
    /// `BENCH_RATE_HZ` turns on the coordinated-omission correction: a latency
    /// longer than one send interval means samples that should have been recorded
    /// during it were not, and the correction fills them back in.
    pub fn new() -> Self {
        Recorder {
            hist: new_histogram(),
            corrected: new_histogram(),
            expected_interval: env_u64("BENCH_RATE_HZ")
                .filter(|hz| *hz > 0)
                .map_or(0, |hz| 1_000_000_000 / hz),
            first_at: None,
            last_at: None,
            warmup: env_u64("BENCH_WARMUP").unwrap_or(500),
            discarded: 0,
            received: 0,
            highest_seq: None,
            first_seq: None,
            gaps: 0,
            reordered: 0,
            window: env_u64("BENCH_WINDOW_SECS")
                .filter(|secs| *secs > 0)
                .map(WindowState::new),
            achieved_hz_override: None,
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
        if self.expected_interval > 0 {
            let _ = self
                .corrected
                .record_correct(latency, self.expected_interval);
        }
        self.received += 1;
        let now = Instant::now();
        self.first_at.get_or_insert(now);
        self.last_at = Some(now);
        if self.first_seq.is_none() {
            self.first_seq = Some(seq);
        }

        let mut lost = 0;
        match self.highest_seq {
            None => {}
            Some(highest) if seq > highest + 1 => lost = seq - highest - 1,
            Some(highest) if seq <= highest => self.reordered += 1,
            Some(_) => {}
        }
        self.gaps += lost;
        if self.highest_seq.is_none_or(|h| seq > h) {
            self.highest_seq = Some(seq);
        }

        if let Some(window) = self.window.as_mut() {
            window.hist.saturating_record(latency);
            window.received += 1;
            window.lost += lost;
        }
        self.close_elapsed_windows();
    }

    /// Report and roll every window whose deadline has passed.
    ///
    /// Call this from an idle read loop too: a window that received nothing still
    /// has to be reported, since a stream that stopped is the point of a soak.
    pub fn close_elapsed_windows(&mut self) {
        let Some(window) = self.window.as_mut() else {
            return;
        };
        while Instant::now() >= window.ends {
            window.report();
            window.roll();
        }
    }

    /// Samples received per second over the recorded window.
    ///
    /// Reported next to the percentiles because a rate well under the one asked
    /// for is how a swallowed stall shows itself. Derived from the `Instant`s
    /// each sample was recorded at, which only tracks the send rate for a
    /// recorder fed as samples arrive; a caller that replays already-measured
    /// latencies into the recorder in a tight loop must supply the real span
    /// with [`Recorder::override_achieved_hz`] instead.
    pub fn achieved_hz(&self) -> f64 {
        if let Some(hz) = self.achieved_hz_override {
            return hz;
        }
        match (self.first_at, self.last_at) {
            (Some(first), Some(last)) if last > first && self.received > 1 => {
                (self.received - 1) as f64 / last.duration_since(first).as_secs_f64()
            }
            _ => 0.0,
        }
    }

    /// Report `hz` as the achieved rate instead of deriving it from record
    /// timestamps.
    ///
    /// For a caller that records already-measured latencies rather than
    /// timing them as they arrive, those timestamps land microseconds apart
    /// regardless of how long the run actually took, so the derived rate is
    /// meaningless; this substitutes the rate the caller measured itself.
    pub fn override_achieved_hz(&mut self, hz: f64) {
        self.achieved_hz_override = Some(hz);
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
            "ROW\t{subject}\t{payload}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{:.2}\t{:.2}\t{:.1}",
            us(self.hist.value_at_quantile(0.50)),
            us(self.hist.min()),
            us(self.hist.value_at_quantile(0.80)),
            us(self.hist.value_at_quantile(0.90)),
            us(self.hist.value_at_quantile(0.95)),
            us(self.hist.value_at_quantile(0.99)),
            us(self.hist.value_at_quantile(0.999)),
            us(self.hist.max()),
            loss,
            self.received,
            us(self.corrected.value_at_quantile(0.50)),
            us(self.corrected.value_at_quantile(0.99)),
            self.achieved_hz()
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
        println!("rate         {:>9.1} Hz received", self.achieved_hz());
        if self.expected_interval > 0 {
            println!(
                "corrected    {:>9.2} us median, {:.2} us p99, {:.2} us p99.9",
                us(self.corrected.value_at_quantile(0.50)),
                us(self.corrected.value_at_quantile(0.99)),
                us(self.corrected.value_at_quantile(0.999))
            );
        }
    }
}

impl Recorder {
    /// A recorder that discards nothing, for callers that warmed up already.
    pub fn unwarmed() -> Self {
        let mut recorder = Recorder::new();
        recorder.warmup = 0;
        recorder
    }

    /// Record a latency that the caller measured itself.
    pub fn record_latency(&mut self, seq: u64, latency_nanos: u64) {
        self.record_measured(seq, latency_nanos);
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Paces a send loop at a fixed rate, on a fixed schedule.
///
/// Sleeps the bulk of the interval and spins the sub-millisecond remainder, since
/// a bare sleep overshoots by enough to distort the measurement.
///
/// The schedule never slips. A send that comes back late leaves the following
/// deadlines where they were, so the loop catches up rather than quietly dropping
/// the slots it missed. Skipping them would hide exactly the stall worth seeing.
#[derive(Debug)]
pub struct Pacer {
    interval: Duration,
    next: Instant,
    due_nanos: u64,
    interval_nanos: u64,
}

impl Pacer {
    /// A pacer running at `rate_hz`.
    ///
    /// The schedule is held against [`Instant`], so a clock step cannot stretch or
    /// collapse the send rate the way it would on [`SystemTime`]. The intended send
    /// times it hands out are wall-clock, to be compared against the subscriber's.
    pub fn new(rate_hz: u64) -> Self {
        let interval_nanos = 1_000_000_000 / rate_hz.max(1);
        Pacer {
            interval: Duration::from_nanos(interval_nanos),
            next: Instant::now(),
            due_nanos: now_nanos(),
            interval_nanos,
        }
    }

    /// The gap between scheduled sends, in nanoseconds.
    pub fn interval_nanos(&self) -> u64 {
        self.interval_nanos
    }

    /// Block until the next send is due, returning the time it was due.
    ///
    /// Stamp the returned time into the sample rather than the time the send
    /// actually happens: the gap between the two is the delay a real publisher
    /// would have suffered, and it belongs in the measurement.
    pub fn wait(&mut self) -> u64 {
        self.next += self.interval;
        self.due_nanos += self.interval_nanos;
        loop {
            let Some(remaining) = self.next.checked_duration_since(Instant::now()) else {
                return self.due_nanos; // deadline already passed
            };
            if remaining.is_zero() {
                return self.due_nanos;
            }
            if remaining > Duration::from_millis(1) {
                std::thread::sleep(remaining - Duration::from_millis(1));
            } else {
                std::hint::spin_loop();
            }
        }
    }
}

/// Tracks why a publisher missed its schedule.
///
/// Lateness that accrues inside the send call is the transport pushing back;
/// lateness that is already there before the call is this process being
/// descheduled. The measurement cannot tell them apart, so count them separately.
#[derive(Debug, Default)]
pub struct SendStats {
    blocked_total: u64,
    blocked_max: u64,
    late_total: u64,
    late_max: u64,
    late_sends: u64,
    interval_nanos: u64,
}

impl SendStats {
    /// Stats for a loop sending every `interval_nanos`.
    ///
    /// Every send is a little late, since the pacer's spin exits just past the
    /// deadline. Only a send later than a whole interval displaced a slot, so
    /// that is what gets counted.
    pub fn new(interval_nanos: u64) -> Self {
        SendStats {
            interval_nanos,
            ..SendStats::default()
        }
    }

    /// Record one send that was due at `due_nanos` and blocked for `blocked`.
    pub fn record(&mut self, due_nanos: u64, entered_nanos: u64, blocked: Duration) {
        let blocked = blocked.as_nanos() as u64;
        self.blocked_total += blocked;
        self.blocked_max = self.blocked_max.max(blocked);
        let late = entered_nanos.saturating_sub(due_nanos);
        self.late_total += late;
        self.late_max = self.late_max.max(late);
        if late > self.interval_nanos {
            self.late_sends += 1;
        }
    }

    /// Print what the publisher's own scheduling cost the run.
    pub fn report(&self, count: u64) {
        let ms = |v: u64| v as f64 / 1_000_000.0;
        println!(
            "blocked in send  {:.2} ms total, {:.2} ms worst",
            ms(self.blocked_total),
            ms(self.blocked_max)
        );
        println!(
            "late before send {:.2} ms total, {:.2} ms worst, {} of {} sends missed a slot",
            ms(self.late_total),
            ms(self.late_max),
            self.late_sends,
            count
        );
    }
}
