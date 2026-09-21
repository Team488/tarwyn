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

/// A subscription callback that holds values back until its snapshot has been
/// delivered.
///
/// `subscribe` has to tell the server about the topic before it reads the
/// current value, or a value published in between reaches nobody and the
/// subscriber is left behind the server for as long as the channel stays quiet.
/// Subscribing first opens the opposite race, a live value arriving before the
/// snapshot, so values that arrive early are buffered here and replayed once
/// the snapshot is through.
///
/// Once the gate is open every value goes straight to the callback; the
/// `open` flag lets that path skip the lock, which is otherwise taken once
/// per value for the life of the subscription.
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

    /// Replay what arrived while the gate was closed, then open it.
    ///
    /// Values delivered during the replay land in the buffer rather than
    /// overtaking it, so the loop runs until the buffer is empty under the lock.
    /// The callback is never run while that lock is held.
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

/// `server topic id -> topic name`, filled in from the server's announcements.
///
/// Keyed by id because the value path looks up by id: a value message carries
/// the topic id and nothing else, and that lookup runs once per inbound value.
pub(crate) type TopicNames = Arc<Mutex<HashMap<u32, String>>>;

/// The control frames that re-establish this client's session, by key.
///
/// A publish or a subscribe is registered once, on the connection it was sent
/// on. Reconnecting gets a server that has never heard of either, so it drops
/// every value the client publishes and sends it nothing it subscribed to.
/// These are replayed on each new connection to put the session back.
pub(crate) type SessionState = Arc<Mutex<HashMap<String, Vec<u8>>>>;
