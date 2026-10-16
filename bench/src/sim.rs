//! `bench sim`: replay a robot's NetworkTables load from a profile. See
//! `bench/BENCHMARK.md`.

pub mod capture;
pub mod dashboard;
pub mod measure;
pub mod profile;
pub mod roles;
pub mod subscriptions;
pub mod sweep;
pub mod values;

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// What every role process is told.
#[derive(Debug, Clone)]
pub struct RoleArgs {
    pub profile: PathBuf,
    /// The NT4 server as `host:port`.
    pub host: String,
    /// What every topic's rate is multiplied by.
    pub multiplier: u32,
    /// Seconds a role runs before it records anything.
    pub warmup: f64,
    /// Seconds a role records for, after the warmup.
    pub seconds: f64,
    /// Copies of the first spare dashboard to add.
    pub extra: usize,
    /// Fresh connections a dashboard role times to full state.
    pub probes: u32,
    /// When the warmup starts, in milliseconds since the Unix epoch, so every
    /// role of a step records over the same window. Now, when absent.
    pub opens_at_ms: Option<u64>,
    /// How long the server may hold a value before it sends it, by design,
    /// in milliseconds. A dashboard adds it to each class's allowance.
    pub batching_ms: f64,
}

/// When a role's recording window opens and closes.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub opens: Instant,
    pub closes: Instant,
}

impl Window {
    /// The window for `args`: the warmup from `opens_at_ms`, or from now,
    /// then the recording.
    pub fn from(args: &RoleArgs) -> Self {
        let now_ms = crate::harness::now_nanos() / 1_000_000;
        let wait = args.opens_at_ms.map_or(0, |at| at.saturating_sub(now_ms));
        let opens =
            Instant::now() + Duration::from_millis(wait) + Duration::from_secs_f64(args.warmup);
        Window {
            opens,
            closes: opens + Duration::from_secs_f64(args.seconds),
        }
    }

    /// Whether `at` falls inside the window.
    pub fn records(&self, at: Instant) -> bool {
        at >= self.opens && at < self.closes
    }
}
