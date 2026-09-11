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

/// How often a telemetry subscriber re-registers with the relay.
///
/// The server drops a registration it has not heard from within its own TTL, so
/// this has to be comfortably shorter than that.
pub(crate) const TELEMETRY_KEEPALIVE: Duration = Duration::from_secs(3);

pub(crate) type TelemetryListener = Arc<dyn Fn(u64, &[u8]) + Send + Sync + 'static>;
pub(crate) struct TelemetryTopic {
    pub(crate) channel: String,
    pub(crate) listeners: SlotMap<DefaultKey, TelemetryListener>,
}

pub(crate) type TelemetryListenerMap = Arc<Mutex<HashMap<u32, TelemetryTopic>>>;

/// Registers `callback` against a channel, returning the key that cancels it.
///
/// `None` when another channel already holds this one's topic hash. Two names can
/// collide; the second is refused rather than cross-wired onto the first.
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

/// Resolve where telemetry datagrams are sent.
///
/// The WebSocket resolves names itself, so the control plane accepts a hostname
/// and this has to as well. Parsing the host as an address and quietly falling
/// back to loopback is what makes a client whose reads and publishes all work
/// send its telemetry nowhere.
pub(crate) fn resolve_telemetry_target(
    host: &str,
    port: u16,
) -> Result<std::net::SocketAddr, ConnectError> {
    use std::net::ToSocketAddrs;

    (host, port)
        .to_socket_addrs()
        .map_err(|source| ConnectError::Resolve {
            host: host.to_string(),
            source,
        })?
        .next()
        .ok_or_else(|| ConnectError::Resolve {
            host: host.to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the name resolved to no addresses",
            ),
        })
}

impl Client {
    /// Publish on the UDP telemetry plane, which trades delivery guarantees for latency.
    ///
    /// Roughly 3.6x faster than the WebSocket path. Subscribers must register with
    /// [`subscribe_telemetry`](Self::subscribe_telemetry). A datagram that cannot be
    /// sent is counted by [`dropped_publishes`](Self::dropped_publishes), not retried.
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

    /// Receive telemetry on a channel, with each payload handed over as bytes.
    ///
    /// Call the returned closure to unsubscribe; dropping it instead leaves the
    /// subscription in place, matching [`subscribe`](Self::subscribe). `None` if
    /// another channel already claimed this one's topic hash. A collision is
    /// refused rather than silently cross-wired.
    ///
    /// Registration is a datagram on the telemetry plane, not a request, so this
    /// does not wait on the server and `Some` does not mean the server heard.
    /// It is resent on a keepalive until it does.
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
    ///
    /// Sent from that socket, so the address the server routes to is the one the
    /// datagram arrived from - correct through NAT, and impossible to point at a
    /// machine that did not ask for it. UDP, so there is nothing to acknowledge;
    /// the keepalive resends until it lands.
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

    /// Renews every telemetry registration before the server's lease expires.
    ///
    /// The server drops a subscriber it has not heard from inside its TTL, and it
    /// sweeps whenever any client registers. Without renewal a subscriber goes
    /// silent as soon as a second client appears, while publishes keep reporting
    /// success. DDS calls the same arrangement a liveliness lease.
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
