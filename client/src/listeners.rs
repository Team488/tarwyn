//! The listener tables a client fans values, logs and topic names out to.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use slotmap::{DefaultKey, SlotMap};

use tarwyn_server::Value;

/// A subscription callback that buffers values until its snapshot is
/// delivered, then passes them straight through.
pub(crate) struct BufferedListener<F> {
    callback: F,
    pending: Mutex<Option<Vec<Value>>>,
    open: AtomicBool,
}

impl<F: Fn(&Value)> BufferedListener<F> {
    pub(crate) fn new(callback: F) -> Self {
        BufferedListener {
            callback,
            pending: Mutex::new(Some(Vec::new())),
            open: AtomicBool::new(false),
        }
    }

    /// Call the callback, bypassing the buffer. Used for the snapshot itself.
    pub(crate) fn call(&self, value: &Value) {
        (self.callback)(value);
    }

    /// Buffer a value while the gate is closed, deliver it once it is open.
    pub(crate) fn deliver(&self, value: &Value) {
        if !self.open.load(Ordering::Acquire)
            && let Ok(mut pending) = self.pending.lock()
            && let Some(buffered) = pending.as_mut()
        {
            buffered.push(value.clone());
            return;
        }
        (self.callback)(value);
    }

    /// Replay what arrived while the gate was closed, then open it. The
    /// callback never runs under the lock.
    pub(crate) fn open(&self) {
        loop {
            let batch = {
                let Ok(mut pending) = self.pending.lock() else {
                    return;
                };
                match pending.as_mut() {
                    None => return,
                    Some(buffered) if buffered.is_empty() => {
                        *pending = None;
                        self.open.store(true, Ordering::Release);
                        return;
                    }
                    Some(buffered) => std::mem::take(buffered),
                }
            };
            for value in &batch {
                (self.callback)(value);
            }
        }
    }
}

pub(crate) type SubscribeListener = Arc<dyn Fn(&Value) + Send + Sync + 'static>;
pub(crate) type SubscribeListenerMap =
    Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

pub(crate) type LogListener = Arc<dyn Fn(&String) + Send + Sync + 'static>;
pub(crate) type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

/// A topic the server announced: its name and numeric NT4 data type.
#[derive(Debug, Clone)]
pub(crate) struct Topic {
    pub(crate) name: String,
    pub(crate) data_type: u32,
}

/// `server topic id -> topic`, filled in from announcements. Value messages
/// carry only the id, and the type tells an empty array's kind.
pub(crate) type TopicNames = Arc<Mutex<HashMap<u32, Topic>>>;

/// The publish and subscribe frames that make up this client's session,
/// replayed on every new connection.
pub(crate) type SessionState = Arc<Mutex<HashMap<String, Vec<u8>>>>;
