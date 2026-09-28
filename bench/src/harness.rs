use hdrhistogram::Histogram;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bytes of sequence number and timestamp ahead of every sample's padding.
pub const HEADER_LEN: usize = 16;

/// What a `ROW` line identifies itself as: case, implementation, version and
/// rate.
#[derive(Debug, Clone)]
pub struct RowId {
    pub case: String,
    pub implementation: String,
    pub version: String,
    pub rate_hz: u64,
}

impl RowId {
    pub fn new(case: &str, implementation: &str, version: &str, rate_hz: u64) -> Self {
        RowId {
            case: case.to_string(),
            implementation: implementation.to_string(),
            version: version.to_string(),
            rate_hz,
        }
    }
}

/// Nanoseconds since the Unix epoch.
pub fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_nanos() as u64
}

/// Stamp a sample with its sequence number and the time it was *due* to be
/// sent.
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

/// One reporting window of a soak run, reported as soon as it closes.
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

/// Records one-way latencies into an HDR histogram, tracking loss by sequence
/// gap.
///
/// The first `WARMUP` samples are discarded. Setting `BENCH_WINDOW_SECS`
/// also prints a `WINDOW` row every that many seconds.
pub struct Recorder {
    hist: Histogram<u64>,
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
    /// A recorder windowed by `BENCH_WINDOW_SECS`, unwindowed when unset.
    pub fn new() -> Self {
        Recorder {
            hist: new_histogram(),
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

    /// Report and roll every window whose deadline has passed, including one
    /// that received nothing. Call it from idle loops too.
    pub fn close_elapsed_windows(&mut self) {
        let Some(window) = self.window.as_mut() else {
            return;
        };
        while Instant::now() >= window.ends {
            window.report();
            window.roll();
        }
    }

    /// Samples received per second, derived from record times unless set with
    /// [`Recorder::override_achieved_hz`].
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

    /// Report `hz` as the achieved rate, for a caller that records measured
    /// latencies in a tight loop.
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

    /// The `ROW` line for this recorder, or `None` if it recorded nothing.
    pub fn row(&self, id: &RowId, payload: usize) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let us = |v: u64| v as f64 / 1000.0;
        let sent = self.received + self.gaps;
        let loss = if sent == 0 {
            0.0
        } else {
            100.0 * self.gaps as f64 / sent as f64
        };
        Some(format!(
            "ROW\t{}\t{}\t{}\t{payload}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{:.1}",
            id.case,
            id.implementation,
            id.version,
            id.rate_hz,
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
            self.achieved_hz()
        ))
    }

    /// Print the percentile row for this row id, and the human summary under it.
    pub fn report(&self, id: &RowId, payload: usize) {
        match self.row(id, payload) {
            Some(row) => println!("{row}"),
            None => {
                println!(
                    "{} {} @ {payload}B: no samples received",
                    id.case, id.implementation
                );
                return;
            }
        }
        let us = |v: u64| v as f64 / 1000.0;
        let sent = self.received + self.gaps;
        let loss = if sent == 0 {
            0.0
        } else {
            100.0 * self.gaps as f64 / sent as f64
        };
        println!("case         {} ({})", id.case, id.implementation);
        println!("version      {}", id.version);
        println!("rate         {} Hz asked", id.rate_hz);
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
            "p99          {:>9.2} us",
            us(self.hist.value_at_quantile(0.99))
        );
        println!(
            "p99.9        {:>9.2} us",
            us(self.hist.value_at_quantile(0.999))
        );
        println!("p100         {:>9.2} us", us(self.hist.max()));
        println!("loss         {:>9.2} %", loss);
        println!("rate         {:>9.1} Hz received", self.achieved_hz());
    }
}

impl Recorder {
    /// How many samples this recorder discards before it records anything.
    pub fn warmup(&self) -> u64 {
        self.warmup
    }

    /// A recorder that discards `warmup` samples, whatever the environment says.
    pub fn with_warmup(warmup: u64) -> Self {
        Recorder {
            warmup,
            ..Recorder::new()
        }
    }
}

/// Read a foreign harness's `S<TAB>seq<TAB>due<TAB>received` lines and print
/// their `ROW`, so every language's row is computed here alike.
///
/// # Errors
///
/// Returns an error when the file has no sample lines, or when a sample was
/// received before it was due, which means the two clocks disagree.
pub fn row_from_samples(
    path: &std::path::Path,
    id: &RowId,
    payload: usize,
    warmup: Option<u64>,
) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path)?;
    let samples: Vec<(u64, u64, u64)> = text
        .lines()
        .filter_map(|line| line.strip_prefix("S\t"))
        .filter_map(|rest| {
            let mut fields = rest.split('\t');
            let seq = fields.next()?.trim().parse().ok()?;
            let due = fields.next()?.trim().parse().ok()?;
            let received = fields.next()?.trim().parse().ok()?;
            Some((seq, due, received))
        })
        .collect();
    if samples.is_empty() {
        return Err(anyhow::anyhow!(
            "{}: no sample lines to report",
            path.display()
        ));
    }

    if let Some((seq, due, received)) = samples.iter().find(|(_, due, got)| got < due) {
        return Err(anyhow::anyhow!(
            "{}: sample {seq} was received {} ns before it was due; \
                 the publisher and subscriber clocks disagree",
            path.display(),
            due - received
        ));
    }

    let mut recorder = match warmup {
        Some(warmup) => Recorder::with_warmup(warmup),
        None => Recorder::new(),
    };
    let warmup = recorder.warmup() as usize;
    for (seq, due, received) in &samples {
        recorder.record_latency(*seq, received - due);
    }
    if let (Some(first), Some(last)) = (samples.get(warmup), samples.last())
        && last.2 > first.2
    {
        let seconds = (last.2 - first.2) as f64 / 1e9;
        recorder.override_achieved_hz((samples.len() - warmup - 1) as f64 / seconds);
    }
    recorder
        .row(id, payload)
        .ok_or_else(|| anyhow::anyhow!("no samples recorded"))
}

/// Read a foreign harness's sample lines and print the `ROW` line for them.
///
/// # Errors
///
/// Returns any error from [`row_from_samples`].
pub fn report_samples(path: &std::path::Path, id: &RowId, payload: usize) -> anyhow::Result<()> {
    println!("{}", row_from_samples(path, id, payload, None)?);
    Ok(())
}

impl Recorder {
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

/// Paces a send loop on a fixed schedule: sleeps most of each interval and
/// spins the rest. A late send never moves later deadlines.
#[derive(Debug)]
pub struct Pacer {
    interval: Duration,
    next: Instant,
    interval_nanos: u64,
}

impl Pacer {
    /// A pacer running at `rate_hz`. The schedule runs on [`Instant`], and the
    /// due times it hands out are wall-clock.
    pub fn new(rate_hz: u64) -> Self {
        let interval_nanos = 1_000_000_000 / rate_hz.max(1);
        Pacer {
            interval: Duration::from_nanos(interval_nanos),
            next: Instant::now(),
            interval_nanos,
        }
    }

    /// The gap between scheduled sends, in nanoseconds.
    pub fn interval_nanos(&self) -> u64 {
        self.interval_nanos
    }

    /// Block until the next send is due, and return the wall-clock time it was
    /// due. Stamp that into the sample, not the actual send time.
    pub fn wait(&mut self) -> u64 {
        self.next += self.interval;
        while let Some(remaining) = self.next.checked_duration_since(Instant::now()) {
            if remaining.is_zero() {
                break;
            }
            if remaining > Duration::from_millis(1) {
                std::thread::sleep(remaining - Duration::from_millis(1));
            } else {
                std::hint::spin_loop();
            }
        }
        self.due_wall_clock()
    }

    /// The wall-clock time the deadline just waited for fell at.
    fn due_wall_clock(&self) -> u64 {
        // Advancing a wall-clock counter instead would drift with NTP and turn samples negative.
        let overshoot = Instant::now()
            .saturating_duration_since(self.next)
            .as_nanos() as u64;
        now_nanos().saturating_sub(overshoot)
    }
}

/// Counts a publisher's missed slots, split into lateness inside the send call
/// and lateness before it.
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
    /// Stats for a loop sending every `interval_nanos`. Only a send later than a
    /// whole interval counts as missed.
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
