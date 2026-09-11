use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{
    LazyLock, Mutex, Once,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::utils::ring_buffer::RingBuffer;

const UNREAD_LOG_LIMIT: usize = 500;

/// The server's logger: a bounded history, plus the lines no client has read yet.
///
/// Both are capped, so a server nobody is reading logs from does not grow.
#[derive(Debug)]
pub struct Logger {
    enabled: AtomicBool,
    logs: Mutex<RingBuffer<String>>,
    unread_logs: Mutex<Vec<String>>,
    dropped: AtomicU64,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        // Enable all logs at or below max level
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
        }
    }

    fn flush(&self) {}
}

impl Logger {
    /// The full retained history, oldest first. `None` if the lock is poisoned.
    pub fn get_logs(&self) -> Option<Vec<String>> {
        if let Ok(buffer) = self.logs.lock() {
            Some(buffer.items.iter().cloned().collect())
        } else {
            None
        }
    }

    /// How many log lines were discarded because nothing read them in time.
    ///
    /// The unread queue is capped, and the oldest lines are dropped to keep the
    /// newest. A subscriber that connects after this has moved is missing lines
    /// that are not in [`read_unread_logs`](Self::read_unread_logs) and never
    /// will be; the retained history from [`get_logs`](Self::get_logs) may still
    /// hold them.
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
}

/// The process-wide logger, installed by [`init_logger`].
pub static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger {
    enabled: AtomicBool::new(false),
    logs: Mutex::new(RingBuffer::new(500)),
    unread_logs: Mutex::new(Vec::new()),
    dropped: AtomicU64::new(0),
});

static INIT: Once = Once::new();

/// Install [`LOGGER`] as the `log` implementation. Does nothing after the first
/// call.
///
/// Records are only kept when `enabled` is true, which the binary sets from
/// `--log`; otherwise nothing is retained.
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
