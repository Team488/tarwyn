//! The UDP telemetry plane: registering listeners and publishing on it.

use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Instant,
};

use slotmap::{DefaultKey, SlotMap};

use tarwyn_protobuf::telemetry;
use tarwyn_server::Value;

use crate::client::Client;
use crate::config::ConnectError;
use crate::connection::POLL_INTERVAL;

/// How often a telemetry subscriber re-registers, well inside the server's
/// lease.
pub(crate) const TELEMETRY_KEEPALIVE: Duration = Duration::from_secs(3);

pub(crate) type TelemetryListener = Arc<dyn Fn(u64, &[u8]) + Send + Sync + 'static>;
pub(crate) struct TelemetryTopic {
    pub(crate) channel: String,
    pub(crate) listeners: SlotMap<DefaultKey, TelemetryListener>,
}

pub(crate) type TelemetryListenerMap = Arc<Mutex<HashMap<u32, TelemetryTopic>>>;

/// Registers `callback` against a channel, returning the key that cancels it.
///
/// `None` when another channel already holds this one's topic hash. Two names
/// can collide, and the second is refused instead of cross-wired onto the first.
pub(crate) fn register_telemetry_listener(
    listeners: &mut HashMap<u32, TelemetryTopic>,
    channel: &str,
    callback: TelemetryListener,
) -> Option<DefaultKey> {
    let hash = telemetry::topic_hash(channel);
    let topic = listeners.entry(hash).or_insert_with(|| TelemetryTopic {
        channel: channel.to_string(),
        listeners: SlotMap::new(),
    });
    if topic.channel != channel {
        if topic.listeners.is_empty() {
            listeners.remove(&hash);
        }
        return None;
    }
    Some(topic.listeners.insert(callback))
}

/// Resolve where telemetry datagrams are sent, by name like the WebSocket.
///
/// Prefers IPv4, since the socket is IPv4 and `localhost` often resolves to
/// `::1` first.
pub(crate) fn resolve_telemetry_target(
    host: &str,
    port: u16,
) -> Result<std::net::SocketAddr, ConnectError> {
    use std::net::ToSocketAddrs;

    let addresses: Vec<std::net::SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|source| ConnectError::Resolve {
            host: host.to_string(),
            source,
        })?
        .collect();
    addresses
        .iter()
        .find(|address| address.is_ipv4())
        .or(addresses.first())
        .copied()
        .ok_or_else(|| ConnectError::Resolve {
            host: host.to_string(),
            source: std::io::ErrorKind::NotFound.into(),
        })
}

impl Client {
    /// Publish on the UDP telemetry plane: low latency, no delivery guarantee.
    /// Unsendable datagrams count in [`dropped_publishes`](Self::dropped_publishes).
    pub fn publish_telemetry(&self, channel: &str, payload: &[u8]) {
        if let Some(logger) = self.logger.get() {
            logger.record_raw(channel, payload);
        }
        let mut buf = vec![0u8; telemetry::HEADER_LEN + payload.len()];
        let len = telemetry::encode(
            &mut buf,
            telemetry::topic_hash(channel),
            telemetry::now_micros(),
            payload,
        );
        if self
            .telemetry_socket
            .send_to(&buf[..len], self.telemetry_target)
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Receive telemetry on a channel as bytes. Call the returned closure to
    /// unsubscribe.
    ///
    /// `None` if another channel already holds this topic hash. `Some` does not
    /// mean the server heard: registration is a datagram, resent until it lands.
    pub fn subscribe_telemetry<F>(
        &self,
        channel: &str,
        callback: F,
    ) -> Option<impl FnOnce() + Send + 'static>
    where
        F: Fn(&Value) + Send + Sync + 'static,
    {
        self.subscribe_telemetry_timestamped(channel, move |_timestamp_us, payload| {
            callback(&Value::Bytes(payload.to_vec()));
        })
    }

    /// As [`subscribe_telemetry`](Self::subscribe_telemetry), but the callback also
    /// receives the publisher's timestamp in microseconds since the Unix epoch.
    pub fn subscribe_telemetry_timestamped<F>(
        &self,
        channel: &str,
        callback: F,
    ) -> Option<impl FnOnce() + Send + 'static>
    where
        F: Fn(u64, &[u8]) + Send + Sync + 'static,
    {
        let hash = telemetry::topic_hash(channel);
        let mut listeners = self.telemetry_listeners.lock().ok()?;
        let key = register_telemetry_listener(&mut listeners, channel, Arc::new(callback))?;
        drop(listeners);
        self.register_telemetry(hash);
        self.start_telemetry_receiver();
        self.start_telemetry_keepalive();

        let listeners = Arc::clone(&self.telemetry_listeners);
        Some(move || {
            let Ok(mut listeners) = listeners.lock() else {
                return;
            };
            if let Some(topic) = listeners.get_mut(&hash) {
                topic.listeners.remove(key);
                if topic.listeners.is_empty() {
                    listeners.remove(&hash);
                }
            }
        })
    }

    /// Ask the server to relay a channel to this client's telemetry socket.
    fn register_telemetry(&self, channel_hash: u32) {
        let mut buf = [0u8; telemetry::HEADER_LEN];
        let len = telemetry::encode_registration(&mut buf, channel_hash);
        let _ = self
            .telemetry_socket
            .send_to(&buf[..len], self.telemetry_target);
    }

    pub(crate) fn start_telemetry_receiver(&self) {
        if self.stop.load(Ordering::SeqCst) || self.telemetry_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let socket = Arc::clone(&self.telemetry_socket);
        let listeners = Arc::clone(&self.telemetry_listeners);
        let stop = Arc::clone(&self.stop);
        let _ = socket.set_read_timeout(Some(POLL_INTERVAL));

        let handle = std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok((len, _from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((channel_hash, timestamp_us, payload)) = telemetry::decode(&buf[..len])
                else {
                    continue;
                };
                let Ok(listeners) = listeners.lock() else {
                    continue;
                };
                let callbacks: Vec<TelemetryListener> = listeners
                    .get(&channel_hash)
                    .map(|topic| topic.listeners.values().map(Arc::clone).collect())
                    .unwrap_or_default();
                drop(listeners);

                for callback in callbacks {
                    callback(timestamp_us, payload);
                }
            }
        });
        self.track(handle);
    }

    /// Renews every telemetry registration before the server's lease expires,
    /// or the subscriber goes silent at the next sweep.
    pub(crate) fn start_telemetry_keepalive(&self) {
        if self.stop.load(Ordering::SeqCst) || self.telemetry_keepalive.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let listeners = Arc::clone(&self.telemetry_listeners);
        let telemetry_socket = Arc::clone(&self.telemetry_socket);
        let stop = Arc::clone(&self.stop);
        let target = self.telemetry_target;

        let handle = std::thread::spawn(move || {
            loop {
                let due = Instant::now() + TELEMETRY_KEEPALIVE;
                while Instant::now() < due {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }

                let hashes: Vec<u32> = match listeners.lock() {
                    Ok(listeners) => listeners.keys().copied().collect(),
                    Err(_) => continue,
                };
                for hash in hashes {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut buf = [0u8; telemetry::HEADER_LEN];
                    let len = telemetry::encode_registration(&mut buf, hash);
                    let _ = telemetry_socket.send_to(&buf[..len], target);
                }
            }
        });
        self.track(handle);
    }

    pub(crate) fn track(&self, handle: std::thread::JoinHandle<()>) {
        if let Ok(mut threads) = self.threads.lock() {
            threads.push(handle);
        }
    }
}
