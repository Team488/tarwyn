//! `bench sim`: replay a robot's NetworkTables load from a profile. See
//! `bench/BENCHMARK.md`.

pub mod capture;
pub mod dashboard;
pub mod measure;
pub mod profile;
pub mod subscriptions;
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
}

/// When a role's recording window opens and closes.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub opens: Instant,
    pub closes: Instant,
}

impl Window {
    /// The window for `args`, counted from now.
    pub fn from(args: &RoleArgs) -> Self {
        let opens = Instant::now() + Duration::from_secs_f64(args.warmup);
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
