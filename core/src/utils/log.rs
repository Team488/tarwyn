use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{LazyLock, Mutex, Once};

use crate::utils::{args::CONFIG, ring_buffer::RingBuffer};

const UNREAD_LOG_LIMIT: usize = 500;

// Our custom logger
pub struct TarwynLogger {
    logs: Mutex<RingBuffer<String>>,
    unread_logs: Mutex<Vec<String>>,
}

impl Log for TarwynLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if !CONFIG.get().unwrap().log {
            return false;
        }
        // Enable all logs at or below max level
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[{}] {} - {}",
                record.level(),
                record.target(),
                record.args()
            );
            if let Ok(mut buffer) = self.logs.lock() {
                buffer.push(format!(
                    "[{}] {} - {}",
                    record.level(),
                    record.target(),
                    record.args()
                ));
            }
            if let Ok(mut unread) = self.unread_logs.lock() {
                unread.push(format!(
                    "[{}] {} - {}",
                    record.level(),
                    record.target(),
                    record.args()
                ));
                if unread.len() > UNREAD_LOG_LIMIT {
                    let excess = unread.len() - UNREAD_LOG_LIMIT;
                    unread.drain(..excess);
                }
            }
        }
    }

    fn flush(&self) {}
}

impl TarwynLogger {
    pub fn get_logs(&self) -> Option<Vec<String>> {
        if let Ok(buffer) = self.logs.lock() {
            Some(buffer.items.iter().cloned().collect())
        } else {
            None
        }
    }

    pub fn read_unread_logs(&self) -> Option<Vec<String>> {
        if let Ok(mut unread) = self.unread_logs.lock() {
            let logs: Vec<String> = unread.drain(..).collect();
            if logs.is_empty() { None } else { Some(logs) }
        } else {
            None
        }
    }
}

pub static LOGGER: LazyLock<TarwynLogger> = LazyLock::new(|| TarwynLogger {
    logs: Mutex::new(RingBuffer::new(500)),
    unread_logs: Mutex::new(Vec::new()),
});

static INIT: Once = Once::new();

pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&*LOGGER)
            .map(|()| log::set_max_level(LevelFilter::Debug))
            .expect("Failed to set logger");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::args::TarwynArgs;

    #[test]
    fn unread_logs_stop_growing_once_they_hit_the_limit() {
        let _ = CONFIG.set(TarwynArgs { log: true });
        log::set_max_level(LevelFilter::Debug);

        let logger = TarwynLogger {
            logs: Mutex::new(RingBuffer::new(500)),
            unread_logs: Mutex::new(Vec::new()),
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
    }
}
