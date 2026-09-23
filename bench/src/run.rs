//! Running the benchmark: starting servers and probes, collecting rows, and
//! writing the report.

mod compare;
mod env;
mod plan;
mod process;
mod soak;
mod sweep;

pub use compare::compare;
pub use soak::soak;
pub use sweep::sweep;

use std::path::PathBuf;
use std::time::Duration;

/// What a run was asked for.
pub struct Settings {
    /// The rate `soak` and `compare` pace at.
    pub rate_hz: u64,
    /// The rates a sweep paces at, one after another.
    pub rates: Vec<u64>,
    pub samples: u64,
    pub warmup: u64,
    pub count: u64,
    pub payloads: Vec<usize>,
    pub reps: u32,
    pub limit: Duration,
    /// Seconds to wait after a subscriber says it is ready.
    pub sub_settle: Duration,
    /// Which cases to run, or all of them when this is empty.
    pub cases: Vec<String>,
    pub pin: bool,
    pub only_report: bool,
    pub rows_dir: PathBuf,
    pub json: PathBuf,
    pub markdown: PathBuf,
}

impl Settings {
    /// How many samples a foreign probe is asked for: the warmup plus the
    /// samples, since it discards nothing itself.
    fn total_samples(&self) -> u64 {
        self.samples + self.warmup
    }

    /// How long a probe waits before reporting what it has, set below the
    /// timeout that would kill it.
    fn probe_deadline(&self) -> Duration {
        self.limit
            .saturating_sub(Duration::from_secs(10))
            .max(self.limit / 2)
    }
}

/// Warn about the machine settings that account for most of the spread.
fn noise_check() {
    let read = |path: &str| {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    };
    if let Some(governor) = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        && governor != "performance"
    {
        eprintln!("note: cpu governor is '{governor}', not performance; expect run-to-run spread");
    }
    if read("/sys/devices/system/cpu/cpufreq/boost").as_deref() == Some("1") {
        eprintln!("note: turbo/boost is on, so clocks drift with temperature across a long run");
    }
    if let Some(load) =
        read("/proc/loadavg").and_then(|l| l.split_whitespace().next()?.parse::<f64>().ok())
        && load > 1.0
    {
        eprintln!("note: load average is {load:.2}, the machine is not quiet");
    }
}
