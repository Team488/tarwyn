//! A subscription that keeps the latest value.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tarwyn_server::Value;

/// A bounded queue of the values a subscription has seen.
///
/// Handed out by [`Client::subscribe_cached`](crate::Client::subscribe_cached) for call sites that poll
/// rather than run a callback. Oldest values are evicted once it is full.
#[derive(Debug)]
pub struct CachedSubscriber {
    pub(crate) values: Arc<Mutex<VecDeque<Value>>>,
}

impl CachedSubscriber {
    /// Take everything buffered, leaving the queue empty.
    pub fn read_all(&self) -> Vec<Value> {
        match self.values.lock() {
            Ok(mut values) => values.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The most recent value, without draining the rest.
    pub fn latest(&self) -> Option<Value> {
        self.values.lock().ok()?.back().cloned()
    }

    /// How many values are buffered.
    pub fn len(&self) -> usize {
        self.values.lock().map(|v| v.len()).unwrap_or(0)
    }

    /// Whether nothing is buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
