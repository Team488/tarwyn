use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{
    Condvar, LazyLock, Mutex, Once,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use crate::utils::ring_buffer::RingBuffer;

const UNREAD_LOG_LIMIT: usize = 500;

/// The server's logger: a bounded history, plus the lines no client has read
/// yet, also bounded.
#[derive(Debug)]
pub struct Logger {
    enabled: AtomicBool,
    logs: Mutex<RingBuffer<String>>,
    unread_logs: Mutex<Vec<String>>,
    unread_ready: Condvar,
    dropped: AtomicU64,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format!(
            "[{}] {} - {}",
            record.level(),
            record.target(),
            record.args()
        );
        println!("{line}");

        if let Ok(mut buffer) = self.logs.lock() {
            buffer.push(line.clone());
        }
        if let Ok(mut unread) = self.unread_logs.lock() {
            unread.push(line);
            if unread.len() > UNREAD_LOG_LIMIT {
                let excess = unread.len() - UNREAD_LOG_LIMIT;
                unread.drain(..excess);
                self.dropped.fetch_add(excess as u64, Ordering::Relaxed);
            }
            self.unread_ready.notify_all();
        }
    }

    fn flush(&self) {}
}

impl Logger {
    /// The full retained history, oldest first. `None` if the lock is poisoned.
    pub fn get_logs(&self) -> Option<Vec<String>> {
        if let Ok(buffer) = self.logs.lock() {
            Some(buffer.iter().cloned().collect())
        } else {
            None
        }
    }

    /// How many unread log lines were dropped to make room for newer ones.
    ///
    /// [`get_logs`](Self::get_logs) may still hold them.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Take the lines not yet handed to a client, leaving none behind. `None` if
    /// there are none, or the lock is poisoned.
    pub fn read_unread_logs(&self) -> Option<Vec<String>> {
        if let Ok(mut unread) = self.unread_logs.lock() {
            let logs: Vec<String> = unread.drain(..).collect();
            if logs.is_empty() { None } else { Some(logs) }
        } else {
            None
        }
    }

    /// As [`read_unread_logs`](Self::read_unread_logs), but blocks until there
    /// is something to take, `stop` is set, or `timeout` passes.
    pub fn wait_unread_logs(&self, stop: &AtomicBool, timeout: Duration) -> Option<Vec<String>> {
        let unread = self.unread_logs.lock().unwrap_or_else(|p| p.into_inner());
        let (mut unread, _) = self
            .unread_ready
            .wait_timeout_while(unread, timeout, |unread| {
                unread.is_empty() && !stop.load(Ordering::SeqCst)
            })
            .unwrap_or_else(|p| p.into_inner());
        let logs: Vec<String> = unread.drain(..).collect();
        if logs.is_empty() { None } else { Some(logs) }
    }

    /// Wakes every [`wait_unread_logs`](Self::wait_unread_logs) after the
    /// caller sets its stop flag.
    pub fn wake(&self) {
        // Under the lock, so a waiter between its stop check and its wait still hears this.
        let _held = self.unread_logs.lock().unwrap_or_else(|p| p.into_inner());
        self.unread_ready.notify_all();
    }
}

/// The process-wide logger, installed by [`init_logger`].
pub static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger {
    enabled: AtomicBool::new(false),
    logs: Mutex::new(RingBuffer::new(500)),
    unread_logs: Mutex::new(Vec::new()),
    unread_ready: Condvar::new(),
    dropped: AtomicU64::new(0),
});

static INIT: Once = Once::new();

/// Install [`LOGGER`] as the `log` implementation. Does nothing after the first
/// call. Records are kept only when `enabled`, which `--log` sets.
pub fn init_logger(enabled: bool) {
    LOGGER.enabled.store(enabled, Ordering::Relaxed);
    INIT.call_once(|| {
        log::set_logger(&*LOGGER)
            .map(|()| log::set_max_level(LevelFilter::Debug))
            .expect("Failed to set logger");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_logs_stop_growing_once_they_hit_the_limit() {
        log::set_max_level(LevelFilter::Debug);

        let logger = Logger {
            enabled: AtomicBool::new(true),
            logs: Mutex::new(RingBuffer::new(500)),
            unread_logs: Mutex::new(Vec::new()),
            unread_ready: Condvar::new(),
            dropped: AtomicU64::new(0),
        };

        for i in 0..UNREAD_LOG_LIMIT * 3 {
            logger.log(&Record::builder().args(format_args!("{i}")).build());
        }

        let unread = logger.unread_logs.lock().unwrap();
        assert_eq!(unread.len(), UNREAD_LOG_LIMIT);
        assert!(
            unread
                .last()
                .unwrap()
                .ends_with(&format!("{}", UNREAD_LOG_LIMIT * 3 - 1))
        );
        drop(unread);
        assert_eq!(
            logger.dropped(),
            (UNREAD_LOG_LIMIT * 2) as u64,
            "lines pushed out of the unread queue have to be counted, or a log \
             subscriber cannot tell a quiet server from one it fell behind"
        );
    }
}
