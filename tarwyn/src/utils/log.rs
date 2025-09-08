use log::{LevelFilter, Log, Metadata, Record};
use once_cell::sync::Lazy;
use std::sync::{Mutex, Once};

use crate::utils::ring_buffer::RingBuffer;

// Our custom logger
pub struct TarwynLogger {
    logs: Mutex<RingBuffer<String>>,
    unread_logs: Mutex<Vec<String>>,
}

impl Log for TarwynLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
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

pub static LOGGER: Lazy<TarwynLogger> = Lazy::new(|| TarwynLogger {
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
