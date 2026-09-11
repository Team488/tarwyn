//! NT4 registry and subscription semantics.
//!
//! [`NtRegistry`] owns the server's topic state: topic-id allocation and
//! reuse, publisher and subscriber tracking, retained-value caching, and the
//! NT4 control-message emit surface (announce/unannounce/properties/publish/
//! subscribe/unsubscribe). Handlers return queued [`Outbound`] frames keyed
//! by client so Task 5 (the fan-out loop) can flush them to the right
//! `WebsocketConnection`s.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value as Json};

use crate::value::Value;
use crate::websocket::message::{ControlMessage, RTT_TOPIC_ID, ValueMessage};
use crate::websocket::msgpack::encode_meta_payload;

mod datatype;
pub use datatype::{data_type_from_string, type_string, xt_data_type};

/// Publishers, and separately subscriptions, one client may hold at once.
///
/// ntcore caps each of these at 512, added to "help find resource leaks and
/// prevent them from causing excessive slowdowns/crashes"; the same number
/// serves the same purpose here, and is far above what a robot program or a
/// dashboard actually opens.
const MAX_PER_CLIENT: usize = 512;

/// A client identity, owned by the fan-out layer.
pub type ClientId = u64;

/// Whether a topic name is an NT4 meta topic (starts with `$`).
pub fn is_meta_topic(name: &str) -> bool {
    name.starts_with('$')
}

/// Whether a subscription can see a given topic, considering the `$`-hidden rule.
///
/// Meta topics (names starting with `$`) are hidden from subscribers whose
/// patterns do not themselves start with `$`.
fn sub_visible_to(sub: &Subscription, topic_name: &str) -> bool {
    !topic_name.starts_with('$') || sub.patterns.iter().any(|p| p.starts_with('$'))
}

fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// An outbound frame for one client.
#[derive(Debug, Clone, PartialEq)]
pub enum Outbound {
    /// A JSON control message, sent as a binary frame.
    Text(String),
    /// A pre-encoded MessagePack value message, shared across subscribers.
    Value(Arc<[u8]>),
}

/// One saved topic: its name, type string, last value and properties.
pub type PersistentTopic = (String, String, Value, Map<String, Json>);

/// A timestamped retained value.
#[derive(Debug, Clone, PartialEq)]
pub struct StampedValue {
    /// Timestamp in microseconds.
    pub ts_micros: u64,
    /// The retained value.
    pub value: Value,
}

/// Server-side state for one topic.
#[derive(Debug)]
pub struct TopicState {
    /// Stable topic name.
    pub name: String,
    /// Numeric NT4 data type.
    pub data_type: u32,
    /// The type string the publisher announced.
    ///
    /// Several strings share one numeric type. `struct:Pose2d`, `msgpack`
    /// and `raw` are all data type 5, and clients need the original back to
    /// decode the payload, so it is stored rather than derived.
    pub type_str: String,
    /// Topic properties.
    pub properties: Map<String, Json>,
    /// Retained value, when cached.
    pub current: Option<StampedValue>,
    /// Live publisher count.
    pub publishers: usize,
    /// Keep the topic alive after the last publisher leaves.
    pub retained: bool,
    /// Whether to cache the retained value for late subscribers.
    pub cached: bool,
}

impl TopicState {
    /// Whether the server stores the topic's value for late subscribers.
    ///
    /// The NT4 `cached` property turns this off; [`TopicState::cached`] is the
    /// server's own default for topics it creates itself.
    pub fn is_cached(&self) -> bool {
        self.cached
    }

    /// Recomputes [`TopicState::cached`] from the topic's properties.
    ///
    /// The NT4 `cached` property turns retention off. It is folded into the
    /// field whenever properties change so the value path never pays for a
    /// map lookup keyed by a string.
    fn sync_cached(&mut self) {
        self.cached = self
            .properties
            .get("cached")
            .and_then(Json::as_bool)
            .unwrap_or(true);
    }

    /// Whether the topic outlives its last publisher.
    ///
    /// NT4 gives both the `persistent` and `retained` properties this meaning,
    /// and either may be set at publish time or later through `setproperties`,
    /// so the answer is read from the properties rather than mirrored into a
    /// field that could drift. [`TopicState::retained`] is the server's own
    /// override, used for topics it creates itself.
    pub fn is_retained(&self) -> bool {
        self.retained
            || ["persistent", "retained"].iter().any(|key| {
                self.properties
                    .get(*key)
                    .and_then(Json::as_bool)
                    .unwrap_or(false)
            })
    }
}

/// A client subscription: patterns plus the topics currently matched.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Topic names or prefixes.
    pub patterns: Vec<String>,
    /// Interpret patterns as prefixes.
    pub prefix: bool,
    /// Announce matching topics but send no value updates.
    pub topics_only: bool,
    /// Topic ids currently matched by this subscription.
    pub matched: HashSet<u32>,
    /// Original subscription options map, preserved for meta-topic payloads.
    pub options: Map<String, Json>,
}

/// Connection-scoped publish and subscribe state for one client.
#[derive(Debug, Default)]
struct ClientState {
    /// `pubuid -> topic id` for this client's live publishes.
    pubs: HashMap<u32, u32>,
    /// `subuid -> subscription`.
    subs: HashMap<u32, Subscription>,
    /// The original client name from the handshake (before deduplication).
    /// The peer address this client connected from, as `host:port`.
    conn_info: String,
}

/// The NT4 registry: topic + connection state and control-message emission.
#[derive(Debug, Default)]
pub struct NtRegistry {
    /// `topic id -> state`.
    topics: HashMap<u32, TopicState>,
    /// `topic name -> id`.
    by_name: HashMap<String, u32>,
    /// `client -> connection state`.
    clients: HashMap<ClientId, ClientState>,
    /// `topic id -> subscribed clients` (fan-out).
    topic_subscribers: HashMap<u32, Vec<ClientId>>,
    /// `topic id -> clients announced this topic`.
    topic_announced: HashMap<u32, Vec<ClientId>>,
    /// Freed topic ids, reused lowest-first before `next_id` grows.
    freed: Vec<u32>,
    /// Next brand-new topic id.
    next_id: u32,
    /// Every deduplicated client name currently in use.
    ///
    /// A set rather than a per-base counter: a counter that goes back down on
    /// disconnect re-issues a name a live client is still answering to, which
    /// puts two connections on one `$clientpub$`/`$clientsub$` pair.
    client_names: HashSet<String>,
    /// `client id -> deduplicated client name`.
    client_name_by_id: HashMap<ClientId, String>,
}

impl NtRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The numeric topic id for `name`, if the topic exists.
    pub fn topic_id(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    /// The topic name for `id`, if the topic exists.
    pub fn topic_name(&self, id: u32) -> Option<String> {
        self.topics.get(&id).map(|topic| topic.name.clone())
    }

    /// Handles a client `publish`, returning the outbound frames to send.
    pub fn handle_publish(
        &mut self,
        client: ClientId,
        name: &str,
        pubuid: u32,
        type_str: &str,
        properties: Map<String, Json>,
    ) -> Vec<(ClientId, Outbound)> {
        self.ensure_client(client);
        if self
            .clients
            .get(&client)
            .is_some_and(|cs| !cs.pubs.contains_key(&pubuid) && cs.pubs.len() >= MAX_PER_CLIENT)
        {
            return Vec::new();
        }
        let mut routes = Vec::new();

        let mut is_new = false;
        let id = if let Some(&id) = self.by_name.get(name) {
            id
        } else {
            let id = self.alloc_id();
            self.topics.insert(
                id,
                TopicState {
                    name: name.to_string(),
                    data_type: data_type_from_string(type_str),
                    type_str: type_str.to_string(),
                    cached: properties
                        .get("cached")
                        .and_then(Json::as_bool)
                        .unwrap_or(true),
                    properties,
                    current: None,
                    publishers: 0,
                    retained: false,
                },
            );
            self.by_name.insert(name.to_string(), id);
            is_new = true;
            id
        };

        let reannounce = self
            .clients
            .get_mut(&client)
            .expect("client must exist")
            .pubs
            .insert(pubuid, id)
            == Some(id);
        if !reannounce && let Some(topic) = self.topics.get_mut(&id) {
            topic.publishers += 1;
        }

        self.add_announced(client, id);
        routes.push((client, Outbound::Text(self.announce_json(id, Some(pubuid)))));

        if is_new {
            routes.extend(self.announce_to_matching(id, name, Some(client)));
        }
        routes.extend(self.update_meta_clientpub(client));
        routes.extend(self.update_meta_pub(name));
        routes
    }

    /// Handles a client `unpublish`, deleting the topic when the last
    /// publisher leaves a non-retained topic.
    pub fn handle_unpublish(&mut self, client: ClientId, pubuid: u32) -> Vec<(ClientId, Outbound)> {
        let Some(id) = self
            .clients
            .get_mut(&client)
            .and_then(|cs| cs.pubs.remove(&pubuid))
        else {
            return Vec::new();
        };
        let Some(topic) = self.topics.get_mut(&id) else {
            for cs in self.clients.values_mut() {
                cs.pubs.retain(|_, tid| *tid != id);
            }
            return Vec::new();
        };
        let topic_name = topic.name.clone();
        topic.publishers = topic.publishers.saturating_sub(1);
        let current = topic.publishers;
        let retained = self.topics.get(&id).is_some_and(TopicState::is_retained);
        let mut routes = if current == 0 && !retained {
            self.delete_topic(id)
        } else {
            Vec::new()
        };
        routes.extend(self.update_meta_clientpub(client));
        routes.extend(self.update_meta_pub(&topic_name));
        routes
    }

    /// Handles a client `subscribe`, emitting announces and retained values.
    pub fn handle_subscribe(
        &mut self,
        client: ClientId,
        topics: &[String],
        subuid: u32,
        prefix: bool,
        topics_only: bool,
        options: Map<String, Json>,
    ) -> Vec<(ClientId, Outbound)> {
        self.ensure_client(client);
        if self
            .clients
            .get(&client)
            .is_some_and(|cs| !cs.subs.contains_key(&subuid) && cs.subs.len() >= MAX_PER_CLIENT)
        {
            return Vec::new();
        }
        // Re-issuing the same `subuid` replaces the prior subscription.
        let mut touched: HashSet<u32> = HashSet::new();
        if let Some(prev) = self
            .clients
            .get_mut(&client)
            .and_then(|cs| cs.subs.remove(&subuid))
        {
            for id in prev.matched {
                if let Some(list) = self.topic_subscribers.get_mut(&id) {
                    list.retain(|c| *c != client);
                }
                touched.insert(id);
            }
        }

        let mut sub = Subscription {
            patterns: topics.to_vec(),
            prefix,
            topics_only,
            matched: HashSet::new(),
            options,
        };
        let mut routes = Vec::new();

        let mut matched_ids: Vec<u32> = self
            .by_name
            .iter()
            .filter(|(nm, _)| {
                sub_visible_to(&sub, nm)
                    && sub
                        .patterns
                        .iter()
                        .any(|p| pattern_matches(sub.prefix, p, nm))
            })
            .map(|(_, id)| *id)
            .collect();
        matched_ids.sort_unstable();

        for id in matched_ids {
            sub.matched.insert(id);
            touched.insert(id);
            if !topics_only {
                self.add_subscriber(client, id);
            }
            if self.add_announced(client, id) {
                routes.push((client, Outbound::Text(self.announce_json(id, None))));
            }
            if topics_only {
                continue;
            }
            let retained_for_late = self
                .topics
                .get(&id)
                .filter(|topic| topic.is_cached())
                .and_then(|topic| topic.current.as_ref());
            if let Some(stamped) = retained_for_late {
                let bytes = encode_once(&stamped.value, stamped.ts_micros, id);
                routes.push((client, Outbound::Value(bytes)));
            }
        }
        self.clients
            .get_mut(&client)
            .expect("client must exist")
            .subs
            .insert(subuid, sub);
        routes.extend(self.update_meta_clientsub(client));
        routes.extend(self.update_meta_sub_for(&touched));
        routes
    }

    /// Handles a client `unsubscribe`.
    pub fn handle_unsubscribe(
        &mut self,
        client: ClientId,
        subuid: u32,
    ) -> Vec<(ClientId, Outbound)> {
        let removed = self
            .clients
            .get_mut(&client)
            .and_then(|cs| cs.subs.remove(&subuid));
        let mut touched: HashSet<u32> = HashSet::new();
        if let Some(sub) = removed {
            for id in sub.matched {
                if let Some(list) = self.topic_subscribers.get_mut(&id) {
                    list.retain(|c| *c != client);
                }
                touched.insert(id);
            }
        }
        let mut routes = Vec::new();
        routes.extend(self.update_meta_clientsub(client));
        routes.extend(self.update_meta_sub_for(&touched));
        routes
    }

    /// Handles a client value update, returning the fan-out frames.
    pub fn handle_value(
        &mut self,
        client: ClientId,
        pubuid: u32,
        value: Value,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(topic_id) = self.topic_id_for_pubuid(client, pubuid) else {
            return Vec::new();
        };
        self.handle_topic_value(topic_id, &value, ts_micros)
    }

    /// The topic a client's publisher UID publishes to, if the server knows it.
    ///
    /// NT4 binary frames from a client carry the publisher UID the client
    /// chose, not the server's topic id, and the server must ignore UIDs it
    /// never assigned.
    pub fn topic_id_for_pubuid(&self, client: ClientId, pubuid: u32) -> Option<u32> {
        self.clients.get(&client)?.pubs.get(&pubuid).copied()
    }

    /// Whether a value would be accepted for `topic_id`.
    ///
    /// [`NtRegistry::handle_topic_value`] drops a value whose type does not
    /// match the topic, and an empty route list cannot say whether that
    /// happened or the topic simply had no subscribers. Callers that mirror
    /// values elsewhere ask here so the two never disagree.
    pub fn accepts_value(&self, topic_id: u32, value: &Value) -> bool {
        self.topics
            .get(&topic_id)
            .is_some_and(|topic| xt_data_type(value) == topic.data_type)
    }

    /// Fans a value out on an already-resolved topic id.
    ///
    /// This is the server's own publish path; a client's value message must
    /// resolve its publisher UID through [`NtRegistry::handle_value`] first.
    pub fn handle_topic_value(
        &mut self,
        topic_id: u32,
        value: &Value,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(topic) = self.topics.get(&topic_id) else {
            return Vec::new();
        };
        // A publisher whose data type does not match the topic is ignored.
        if xt_data_type(value) != topic.data_type {
            return Vec::new();
        }
        let cached = topic.is_cached();
        let retain = match &topic.current {
            None => true,
            Some(cur) => ts_micros >= cur.ts_micros,
        };
        if retain
            && cached
            && let Some(topic) = self.topics.get_mut(&topic_id)
        {
            topic.current = Some(StampedValue {
                ts_micros,
                value: value.clone(),
            });
        }
        let frame = encode_once(value, ts_micros, topic_id);
        self.topic_subscribers
            .get(&topic_id)
            .map(|subscribers| {
                subscribers
                    .iter()
                    .map(|c| (*c, Outbound::Value(Arc::clone(&frame))))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Ensures a topic exists for `name`, then handles a value update for it.
    ///
    /// Used by the control plane (CAS) where a value may be assigned to a
    /// channel no NT4 client has published yet. The topic is created with the
    /// value's data type so it is readable and subscribeable.
    pub fn handle_upsert_value(
        &mut self,
        name: &str,
        value: Value,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let id = match self.by_name.get(name) {
            Some(&id) => id,
            None => {
                let id = self.alloc_id();
                self.topics.insert(
                    id,
                    TopicState {
                        name: name.to_string(),
                        data_type: xt_data_type(&value),
                        type_str: type_string(xt_data_type(&value))
                            .expect("a value's own data type is always representable")
                            .to_string(),
                        properties: Map::new(),
                        current: None,
                        publishers: 0,
                        retained: true,
                        cached: true,
                    },
                );
                self.by_name.insert(name.to_string(), id);
                id
            }
        };
        self.handle_topic_value(id, &value, ts_micros)
    }

    /// Handles a `setproperties`, broadcasting the update.
    pub fn handle_setproperties(
        &mut self,
        client: ClientId,
        name: &str,
        update: Map<String, Json>,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(&id) = self.by_name.get(name) else {
            return Vec::new();
        };
        if let Some(topic) = self.topics.get_mut(&id) {
            for (key, value) in &update {
                if value.is_null() {
                    topic.properties.remove(key);
                } else {
                    topic.properties.insert(key.clone(), value.clone());
                }
            }
            topic.sync_cached();
        }
        let announced = self.topic_announced.get(&id).cloned().unwrap_or_default();
        let with_ack = self.properties_json(name, &update, Some(true));
        let no_ack = self.properties_json(name, &update, None);
        announced
            .into_iter()
            .map(|c| {
                let msg = if c == client { &with_ack } else { &no_ack };
                (c, Outbound::Text(msg.clone()))
            })
            .collect()
    }

    /// Forcibly removes a topic by name and broadcasts `unannounce`.
    pub fn handle_unannounce(&mut self, client: ClientId, name: &str) -> Vec<(ClientId, Outbound)> {
        let Some(&id) = self.by_name.get(name) else {
            return Vec::new();
        };
        let pubuids: Vec<u32> = self
            .clients
            .get(&client)
            .map(|cs| {
                cs.pubs
                    .iter()
                    .filter(|(_, tid)| **tid == id)
                    .map(|(pu, _)| *pu)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(cs) = self.clients.get_mut(&client) {
            for pu in pubuids {
                cs.pubs.remove(&pu);
            }
        }
        let mut routes = self.delete_topic(id);
        routes.extend(self.update_meta_clientpub(client));
        routes.extend(self.update_meta_pub(name));
        routes
    }

    /// Every persistent topic with a value, as `(name, type string, value)`.
    ///
    /// NT4 asks a server to save these and hand them back at startup, so a
    /// dashboard that set one still sees it after the robot reboots.
    pub fn persistent_snapshot(&self) -> Vec<PersistentTopic> {
        self.topics
            .values()
            .filter(|topic| {
                topic
                    .properties
                    .get("persistent")
                    .and_then(Json::as_bool)
                    .unwrap_or(false)
            })
            .filter_map(|topic| {
                let stamped = topic.current.as_ref()?;
                Some((
                    topic.name.clone(),
                    topic.type_str.clone(),
                    stamped.value.clone(),
                    topic.properties.clone(),
                ))
            })
            .collect()
    }

    /// Recreates persistent topics saved by a previous run.
    ///
    /// The topics come back with no publisher, so they are retained until one
    /// appears, exactly as they were when the server stopped.
    pub fn restore_persistent(&mut self, entries: Vec<PersistentTopic>, ts_micros: u64) {
        for (name, type_str, value, properties) in entries {
            if self.by_name.contains_key(&name) {
                continue;
            }
            let id = self.alloc_id();
            self.topics.insert(
                id,
                TopicState {
                    name: name.clone(),
                    data_type: data_type_from_string(&type_str),
                    type_str,
                    properties,
                    current: Some(StampedValue { ts_micros, value }),
                    publishers: 0,
                    retained: true,
                    cached: true,
                },
            );
            self.by_name.insert(name, id);
        }
    }

    /// Marks a topic retained, so it survives the last publisher leaving.
    pub fn set_retained(&mut self, name: &str, retained: bool) {
        if let Some(id) = self.by_name.get(name).copied()
            && let Some(topic) = self.topics.get_mut(&id)
        {
            topic.retained = retained;
        }
    }

    /// Responds to a timestamp sync, echoing the client value server-side.
    pub fn handle_timestamp(
        &mut self,
        client: ClientId,
        client_value: Value,
        server_ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        vec![(
            client,
            Outbound::Value(encode_once(&client_value, server_ts_micros, RTT_TOPIC_ID)),
        )]
    }

    fn ensure_client(&mut self, client: ClientId) {
        self.clients.entry(client).or_default();
    }

    /// Ensures a meta-topic exists with the correct configuration.
    fn ensure_meta_topic(&mut self, name: &str) -> (u32, Vec<(ClientId, Outbound)>) {
        if let Some(&id) = self.by_name.get(name) {
            return (id, Vec::new());
        }
        let id = self.alloc_id();
        self.topics.insert(
            id,
            TopicState {
                name: name.to_string(),
                data_type: 5,
                type_str: "msgpack".to_string(),
                properties: Map::new(),
                current: None,
                publishers: 0,
                retained: true,
                cached: true,
            },
        );
        self.by_name.insert(name.to_string(), id);
        let routes = self.announce_to_matching(id, name, None);
        (id, routes)
    }

    /// Registers a client connection, assigning a deduplicated name and
    /// creating its per-client meta topics.
    ///
    /// Returns the outbound frames to dispatch (meta-topic updates).
    pub fn on_connect(
        &mut self,
        client: ClientId,
        base_name: &str,
        conn_info: &str,
    ) -> Vec<(ClientId, Outbound)> {
        let name = self.dedup_client_name(base_name);
        self.client_name_by_id.insert(client, name);
        self.ensure_client(client);
        if let Some(cs) = self.clients.get_mut(&client) {
            cs.conn_info = conn_info.to_string();
        }
        let mut routes = Vec::new();
        routes.extend(self.update_meta_clients());
        routes.extend(self.update_meta_clientpub(client));
        routes.extend(self.update_meta_clientsub(client));
        routes.extend(self.update_meta_serversub());
        routes.extend(self.update_meta_serverpub());
        routes
    }

    /// Removes a client connection, its publishers and its per-client meta topics.
    ///
    /// A dropped connection releases its publishers exactly as an explicit
    /// `unpublish` would, one release per publisher UID rather than per topic:
    /// [`NtRegistry::handle_publish`] counts every publish, so a client holding
    /// two UIDs on one topic contributed two. Without that the count never
    /// returns to zero and the topic is pinned for the life of the server.
    ///
    /// Returns the outbound frames to dispatch (unannounces and meta updates).
    pub fn on_disconnect(&mut self, client: ClientId) -> Vec<(ClientId, Outbound)> {
        let name = self.client_name_by_id.remove(&client);
        let (published, pub_topic_names, subscribed): (Vec<u32>, Vec<String>, HashSet<u32>) =
            match self.clients.get(&client) {
                Some(cs) => (
                    cs.pubs.values().copied().collect(),
                    cs.pubs
                        .values()
                        .filter_map(|tid| self.topics.get(tid).map(|t| t.name.clone()))
                        .collect(),
                    cs.subs
                        .values()
                        .flat_map(|s| s.matched.iter().copied())
                        .collect(),
                ),
                None => (Vec::new(), Vec::new(), HashSet::new()),
            };
        self.clients.remove(&client);
        if let Some(name) = name {
            self.release_client_name(&name);
            self.delete_topic_if_exists(&format!("$clientpub${name}"));
            self.delete_topic_if_exists(&format!("$clientsub${name}"));
        }
        let mut routes = Vec::new();
        let mut candidates: Vec<u32> = Vec::new();
        for id in published {
            if let Some(topic) = self.topics.get_mut(&id) {
                topic.publishers = topic.publishers.saturating_sub(1);
                if !candidates.contains(&id) {
                    candidates.push(id);
                }
            }
        }
        for id in candidates {
            let orphaned = self
                .topics
                .get(&id)
                .is_some_and(|topic| topic.publishers == 0 && !topic.is_retained());
            if orphaned {
                routes.extend(self.delete_topic(id));
            }
        }
        routes.extend(self.update_meta_clients());
        routes.extend(self.update_meta_sub_for(&subscribed));
        for topic_name in pub_topic_names {
            routes.extend(self.update_meta_pub(&topic_name));
        }
        routes
    }

    /// Assigns a unique client name, appending `@N` when `base` is taken.
    pub fn dedup_client_name(&mut self, base: &str) -> String {
        if self.client_names.insert(base.to_string()) {
            return base.to_string();
        }
        for suffix in 1u32.. {
            let candidate = format!("{base}@{suffix}");
            if self.client_names.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!("u32 suffixes outnumber the connections a server can hold")
    }

    fn release_client_name(&mut self, name: &str) {
        self.client_names.remove(name);
    }

    /// Publishes a meta topic's array-of-maps payload and fans it out.
    fn publish_meta(
        &mut self,
        name: &str,
        maps: Vec<Map<String, Json>>,
    ) -> Vec<(ClientId, Outbound)> {
        let (id, mut routes) = self.ensure_meta_topic(name);
        let bytes = encode_meta_payload(&maps);
        routes.extend(self.handle_topic_value(id, &Value::Bytes(bytes), now_micros()));
        routes
    }

    fn delete_topic_if_exists(&mut self, name: &str) {
        if let Some(&id) = self.by_name.get(name) {
            self.delete_topic(id);
        }
    }

    /// The deduplicated name for a client, if registered.
    pub fn client_name(&self, client: ClientId) -> Option<&str> {
        self.client_name_by_id.get(&client).map(String::as_str)
    }

    /// Updates `$clients` with all live connections.
    fn update_meta_clients(&mut self) -> Vec<(ClientId, Outbound)> {
        let mut maps = Vec::new();
        for (cid, cs) in &self.clients {
            let name = self.client_name_by_id.get(cid).cloned().unwrap_or_default();
            let mut m = Map::new();
            m.insert("id".into(), Json::String(name.clone()));
            m.insert("conn".into(), Json::String(cs.conn_info.clone()));
            maps.push(m);
        }
        self.publish_meta("$clients", maps)
    }

    /// Updates `$clientpub$<client>` with the client's live publishes.
    fn update_meta_clientpub(&mut self, client: ClientId) -> Vec<(ClientId, Outbound)> {
        let Some(name) = self.client_name_by_id.get(&client).cloned() else {
            return Vec::new();
        };
        let mut maps = Vec::new();
        if let Some(cs) = self.clients.get(&client) {
            for (uid, tid) in &cs.pubs {
                let mut m = Map::new();
                m.insert("uid".into(), Json::from(*uid));
                if let Some(topic) = self.topics.get(tid) {
                    m.insert("topic".into(), Json::String(topic.name.clone()));
                }
                maps.push(m);
            }
        }
        self.publish_meta(&format!("$clientpub${name}"), maps)
    }

    /// Updates `$clientsub$<client>` with the client's live subscriptions.
    fn update_meta_clientsub(&mut self, client: ClientId) -> Vec<(ClientId, Outbound)> {
        let Some(name) = self.client_name_by_id.get(&client).cloned() else {
            return Vec::new();
        };
        let mut maps = Vec::new();
        if let Some(cs) = self.clients.get(&client) {
            for (uid, sub) in &cs.subs {
                let mut m = Map::new();
                m.insert("uid".into(), Json::from(*uid));
                m.insert(
                    "topics".into(),
                    Json::Array(sub.patterns.iter().cloned().map(Json::String).collect()),
                );
                m.insert("options".into(), Json::Object(sub.options.clone()));
                maps.push(m);
            }
        }
        self.publish_meta(&format!("$clientsub${name}"), maps)
    }

    /// Updates `$sub$<topic>` for exactly the topics whose subscribers changed.
    ///
    /// NT4 updates the meta topic when a client subscribes or unsubscribes to
    /// that topic, so republishing every `$sub$` would emit no-change updates
    /// that a publisher watching them would act on.
    fn update_meta_sub_for(&mut self, ids: &HashSet<u32>) -> Vec<(ClientId, Outbound)> {
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| self.topics.get(id))
            .filter(|t| !is_meta_topic(&t.name))
            .map(|t| t.name.clone())
            .collect();
        let mut routes = Vec::new();
        for name in names {
            routes.extend(self.update_meta_sub(&name));
        }
        routes
    }

    /// Updates `$sub$<topic>` with the topic's subscribers.
    fn update_meta_sub(&mut self, topic_name: &str) -> Vec<(ClientId, Outbound)> {
        let Some(&id) = self.by_name.get(topic_name) else {
            return Vec::new();
        };
        let mut maps = Vec::new();
        let subscribers = self.topic_subscribers.get(&id).cloned().unwrap_or_default();
        for cid in subscribers {
            let client_name = self
                .client_name_by_id
                .get(&cid)
                .cloned()
                .unwrap_or_default();
            if let Some(cs) = self.clients.get(&cid) {
                for (subuid, sub) in &cs.subs {
                    if sub.matched.contains(&id) {
                        let mut m = Map::new();
                        m.insert("client".into(), Json::String(client_name.clone()));
                        m.insert("subuid".into(), Json::from(*subuid));
                        m.insert("options".into(), Json::Object(sub.options.clone()));
                        maps.push(m);
                    }
                }
            }
        }
        self.publish_meta(&format!("$sub${topic_name}"), maps)
    }

    /// Updates `$pub$<topic>` with the topic's publishers.
    fn update_meta_pub(&mut self, topic_name: &str) -> Vec<(ClientId, Outbound)> {
        let Some(&id) = self.by_name.get(topic_name) else {
            return Vec::new();
        };
        let mut maps = Vec::new();
        for (cid, cs) in &self.clients {
            for (uid, tid) in &cs.pubs {
                if *tid == id {
                    let mut m = Map::new();
                    let client_name = self.client_name_by_id.get(cid).cloned().unwrap_or_default();
                    m.insert("client".into(), Json::String(client_name));
                    m.insert("pubuid".into(), Json::from(*uid));
                    maps.push(m);
                }
            }
        }
        self.publish_meta(&format!("$pub${topic_name}"), maps)
    }

    /// Updates `$serversub` (empty; the server holds no subscriptions).
    fn update_meta_serversub(&mut self) -> Vec<(ClientId, Outbound)> {
        self.publish_meta("$serversub", Vec::new())
    }

    /// Updates `$serverpub` (empty; the server holds no publishers).
    fn update_meta_serverpub(&mut self) -> Vec<(ClientId, Outbound)> {
        self.publish_meta("$serverpub", Vec::new())
    }

    /// Allocates the lowest freed topic id, or a brand-new one when none are
    /// free.
    fn alloc_id(&mut self) -> u32 {
        if let Some(pos) = self
            .freed
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| **id)
            .map(|(i, _)| i)
        {
            self.freed.remove(pos)
        } else {
            let id = self.next_id;
            self.next_id += 1;
            id
        }
    }

    fn free_id(&mut self, id: u32) {
        self.freed.push(id);
    }

    fn delete_topic(&mut self, id: u32) -> Vec<(ClientId, Outbound)> {
        let Some(topic) = self.topics.remove(&id) else {
            return Vec::new();
        };
        let topic_name = topic.name;
        self.by_name.remove(&topic_name);
        self.topic_subscribers.remove(&id);
        let announced = self.topic_announced.remove(&id).unwrap_or_default();
        for cs in self.clients.values_mut() {
            for sub in cs.subs.values_mut() {
                sub.matched.remove(&id);
            }
        }
        self.free_id(id);
        let msg = self.unannounce_json(id, &topic_name);
        let routes: Vec<(ClientId, Outbound)> = announced
            .into_iter()
            .map(|c| (c, Outbound::Text(msg.clone())))
            .collect();
        self.delete_topic_if_exists(&format!("$pub${topic_name}"));
        self.delete_topic_if_exists(&format!("$sub${topic_name}"));
        routes
    }

    fn add_subscriber(&mut self, client: ClientId, id: u32) {
        let list = self.topic_subscribers.entry(id).or_default();
        if !list.contains(&client) {
            list.push(client);
        }
    }

    /// Announces a newly created topic to every subscription that matches it.
    ///
    /// A subscription made before the topic existed must still see it, so both
    /// [`NtRegistry::handle_publish`] and meta-topic creation route through
    /// here. `exclude` skips the publisher, which is announced separately with
    /// its `pubuid`. Only subscriptions that want values add a fan-out entry.
    fn announce_to_matching(
        &mut self,
        id: u32,
        name: &str,
        exclude: Option<ClientId>,
    ) -> Vec<(ClientId, Outbound)> {
        let targets: Vec<(ClientId, bool)> = self
            .clients
            .iter()
            .filter(|(cid, _)| Some(**cid) != exclude)
            .filter_map(|(cid, cs)| {
                let mut matching = cs
                    .subs
                    .values()
                    .filter(|s| sub_visible_to(s, name) && sub_matches(s, name))
                    .peekable();
                matching.peek()?;
                let wants_values = cs
                    .subs
                    .values()
                    .any(|s| sub_visible_to(s, name) && sub_matches(s, name) && !s.topics_only);
                Some((*cid, wants_values))
            })
            .collect();
        let mut routes = Vec::new();
        for (cid, wants_values) in targets {
            if self.add_announced(cid, id) {
                routes.push((cid, Outbound::Text(self.announce_json(id, None))));
            }
            if wants_values {
                self.add_subscriber(cid, id);
            }
        }
        routes
    }

    /// Records that `client` has been announced `id`; returns true when first
    /// recorded for this topic.
    fn add_announced(&mut self, client: ClientId, id: u32) -> bool {
        let list = self.topic_announced.entry(id).or_default();
        if list.contains(&client) {
            false
        } else {
            list.push(client);
            true
        }
    }

    fn announce_json(&self, id: u32, pubuid: Option<u32>) -> String {
        let topic = &self.topics[&id];
        ControlMessage::Announce {
            name: topic.name.clone(),
            id,
            data_type: topic.type_str.clone(),
            properties: topic.properties.clone(),
            pubuid,
        }
        .to_json()
    }

    fn unannounce_json(&self, id: u32, name: &str) -> String {
        ControlMessage::Unannounce {
            name: name.to_string(),
            id,
        }
        .to_json()
    }

    fn properties_json(&self, name: &str, update: &Map<String, Json>, ack: Option<bool>) -> String {
        ControlMessage::PropertiesUpdate {
            name: name.to_string(),
            update: update.clone(),
            ack,
        }
        .to_json()
    }
}

/// Whether a single subscription pattern matches `name`.
fn pattern_matches(prefix: bool, pattern: &str, name: &str) -> bool {
    if prefix {
        name.starts_with(pattern)
    } else {
        name == pattern
    }
}

/// Whether any pattern of `sub` matches `name`.
fn sub_matches(sub: &Subscription, name: &str) -> bool {
    sub.patterns
        .iter()
        .any(|p| pattern_matches(sub.prefix, p, name))
}

/// Encodes one complete NT4 value message (the 4-tuple `[id, ts, type, value]`).
pub fn encode_once(v: &Value, ts_micros: u64, topic_id: u32) -> Arc<[u8]> {
    let mut buf = Vec::new();
    ValueMessage {
        topic_id,
        timestamp_micros: ts_micros,
        data_type: xt_data_type(v),
        value: v.clone(),
    }
    .encode(&mut buf);
    Arc::from(buf)
}

#[cfg(test)]
mod tests;
