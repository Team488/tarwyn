//! The NT4 registry: topics, publishers, subscribers and retained values.
//! Handlers return [`Outbound`] frames keyed by client.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value as Json};

use crate::value::Value;
use crate::websocket::message::{ControlMessage, RTT_TOPIC_ID, encode_value_message};
use crate::websocket::msgpack::encode_meta_payload;
use tarwyn_protobuf::telemetry::now_micros;

mod datatype;
pub use datatype::{data_type_from_string, type_string, xt_data_type};

/// Publishers, and separately subscriptions, one client may hold at once.
/// ntcore uses the same cap to catch leaks.
const MAX_PER_CLIENT: usize = 512;

/// Topics the server will hold at once, meta topics included. Persistent and
/// compare-and-set topics outlive their connection, so this bounds them.
pub const MAX_TOPICS: usize = 16_384;

/// A client identity, owned by the fan-out layer.
pub type ClientId = u64;

/// Whether a topic name is an NT4 meta topic (starts with `$`).
pub fn is_meta_topic(name: &str) -> bool {
    name.starts_with('$')
}

/// Whether a subscription can see a topic. Meta topics (starting with `$`)
/// are visible only to patterns that also start with `$`.
fn sub_visible_to(sub: &Subscription, topic_name: &str) -> bool {
    !topic_name.starts_with('$') || sub.patterns.iter().any(|p| p.starts_with('$'))
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
    /// The type string the publisher announced, such as `struct:Pose2d`,
    /// which the numeric type alone cannot recover.
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
    /// Whether the topic is written to disk, per the NT4 `persistent` property.
    pub persistent: bool,
}

impl TopicState {
    /// Whether the server stores the topic's value for late subscribers,
    /// which the NT4 `cached` property can turn off.
    pub fn is_cached(&self) -> bool {
        self.cached
    }

    /// Recomputes [`TopicState::cached`] and [`TopicState::persistent`] from
    /// the properties, so the value path never looks them up by string.
    fn sync_properties(&mut self) {
        self.cached = self
            .properties
            .get("cached")
            .and_then(Json::as_bool)
            .unwrap_or(true);
        self.persistent = self
            .properties
            .get("persistent")
            .and_then(Json::as_bool)
            .unwrap_or(false);
    }

    /// Whether the topic outlives its last publisher: the `persistent` or
    /// `retained` property, or the server's own [`TopicState::retained`].
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
    /// Every deduplicated client name in use.
    client_names: HashSet<String>,
    /// `client id -> deduplicated client name`.
    client_name_by_id: HashMap<ClientId, String>,
    /// Moves whenever [`NtRegistry::persistent_snapshot`] would return
    /// something different, so a saver can tell an unchanged snapshot from
    /// the file it already wrote.
    persistent_generation: u64,
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
    pub fn topic_name(&self, id: u32) -> Option<&str> {
        self.topics.get(&id).map(|topic| topic.name.as_str())
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
            if self.topics.len() >= MAX_TOPICS {
                return Vec::new();
            }
            let id = self.alloc_id();
            self.topics.insert(
                id,
                TopicState {
                    name: name.to_string(),
                    data_type: data_type_from_string(type_str),
                    type_str: type_str.to_string(),
                    cached: true,
                    persistent: false,
                    properties,
                    current: None,
                    publishers: 0,
                    retained: false,
                },
            );
            let topic = self.topics.get_mut(&id).expect("just inserted");
            topic.sync_properties();
            if topic.persistent {
                self.persistent_generation += 1;
            }
            self.by_name.insert(name.to_string(), id);
            is_new = true;
            id
        };

        let previous = self
            .clients
            .get_mut(&client)
            .expect("client must exist")
            .pubs
            .insert(pubuid, id);
        if previous != Some(id)
            && let Some(topic) = self.topics.get_mut(&id)
        {
            topic.publishers += 1;
        }

        self.add_announced(client, id);
        routes.push((client, Outbound::Text(self.announce_json(id, Some(pubuid)))));

        if is_new {
            routes.extend(self.announce_to_matching(id, name));
        }
        if let Some(moved_from) = previous.filter(|old| *old != id) {
            routes.extend(self.release_publisher(moved_from));
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
        if !self.topics.contains_key(&id) {
            for cs in self.clients.values_mut() {
                cs.pubs.retain(|_, tid| *tid != id);
            }
            return Vec::new();
        }
        let mut routes = self.release_publisher(id);
        routes.extend(self.update_meta_clientpub(client));
        routes
    }

    /// Drops one publisher from topic `id`, and deletes the topic if it was the
    /// last one on a non-retained topic.
    fn release_publisher(&mut self, id: u32) -> Vec<(ClientId, Outbound)> {
        let Some(topic) = self.topics.get_mut(&id) else {
            return Vec::new();
        };
        let topic_name = topic.name.clone();
        topic.publishers = topic.publishers.saturating_sub(1);
        let orphaned = topic.publishers == 0 && !topic.is_retained();
        let mut routes = if orphaned {
            self.delete_topic(id)
        } else {
            Vec::new()
        };
        routes.extend(self.update_meta_pub(&topic_name));
        routes
    }

    /// Handles a client `subscribe`, emitting announces and retained values.
    /// Re-issuing a `subuid` replaces the subscription it named.
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

    /// The topic a client's publisher UID publishes to. `None` for a UID the
    /// server never assigned.
    pub fn topic_id_for_pubuid(&self, client: ClientId, pubuid: u32) -> Option<u32> {
        self.clients.get(&client)?.pubs.get(&pubuid).copied()
    }

    /// `value` in the shape topic `topic_id` declares. See [`Value::conformed`].
    pub fn conform(&self, topic_id: u32, value: Value) -> Value {
        match self.topics.get(&topic_id) {
            Some(topic) => value.conformed(topic.data_type),
            None => value,
        }
    }

    /// Whether a value would be accepted for `topic_id`, for callers that
    /// mirror values elsewhere and must agree with the registry.
    pub fn accepts_value(&self, topic_id: u32, value: &Value) -> bool {
        self.topics
            .get(&topic_id)
            .is_some_and(|topic| xt_data_type(value) == topic.data_type)
    }

    /// Fans a value out on a resolved topic id. A value of the wrong data type
    /// is ignored.
    pub fn handle_topic_value(
        &mut self,
        topic_id: u32,
        value: &Value,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(topic) = self.topics.get(&topic_id) else {
            return Vec::new();
        };
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
            if topic.persistent {
                self.persistent_generation += 1;
            }
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

    /// Ensures a topic exists for `name`, typed from the value, then handles
    /// the value.
    pub fn handle_upsert_value(
        &mut self,
        name: &str,
        value: Value,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let id = match self.by_name.get(name) {
            Some(&id) => id,
            None => {
                if self.topics.len() >= MAX_TOPICS {
                    return Vec::new();
                }
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
                        persistent: false,
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
            let was_persistent = topic.persistent;
            for (key, value) in &update {
                if value.is_null() {
                    topic.properties.remove(key);
                } else {
                    topic.properties.insert(key.clone(), value.clone());
                }
            }
            topic.sync_properties();
            if was_persistent || topic.persistent {
                self.persistent_generation += 1;
            }
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

    /// Every persistent topic with a value, as `(name, type string, value,
    /// properties)`.
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

    /// Recreates persistent topics saved by a previous run, retained with no
    /// publisher.
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
                    persistent: false,
                },
            );
            self.topics
                .get_mut(&id)
                .expect("just inserted")
                .sync_properties();
            self.by_name.insert(name, id);
        }
        self.persistent_generation += 1;
    }

    /// A counter that moves whenever the persistent snapshot would differ, so
    /// a saver can skip writes that change nothing.
    pub fn persistent_generation(&self) -> u64 {
        self.persistent_generation
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
                persistent: false,
            },
        );
        self.by_name.insert(name.to_string(), id);
        let routes = self.announce_to_matching(id, name);
        (id, routes)
    }

    /// Registers a client connection, assigning a deduplicated name and
    /// creating its per-client meta topics. Returns the frames to dispatch.
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

    /// Removes a client, its publishers and its per-client meta topics, and
    /// returns the frames to dispatch.
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

    /// Publishes a meta topic's array-of-maps payload and fans it out. A
    /// payload that will not encode is dropped.
    fn publish_meta(
        &mut self,
        name: &str,
        maps: Vec<Map<String, Json>>,
    ) -> Vec<(ClientId, Outbound)> {
        let (id, mut routes) = self.ensure_meta_topic(name);
        let Ok(bytes) = encode_meta_payload(&maps) else {
            return routes;
        };
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

    /// Updates `$sub$<topic>` for only the topics whose subscribers changed,
    /// since a publisher may act on every update.
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

    /// Updates `$serversub`, which is always empty.
    fn update_meta_serversub(&mut self) -> Vec<(ClientId, Outbound)> {
        self.publish_meta("$serversub", Vec::new())
    }

    /// Updates `$serverpub`, which is always empty.
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
        if topic.persistent {
            self.persistent_generation += 1;
        }
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

    /// Announces a new topic to every subscription that matches it, and
    /// records it in their `matched` sets.
    fn announce_to_matching(&mut self, id: u32, name: &str) -> Vec<(ClientId, Outbound)> {
        let mut targets: Vec<(ClientId, bool)> = Vec::new();
        for (cid, cs) in &mut self.clients {
            let mut matched = false;
            let mut wants_values = false;
            for sub in cs.subs.values_mut() {
                if sub_visible_to(sub, name) && sub_matches(sub, name) {
                    // Unsubscribe walks `matched`, so a topic left out would keep sending.
                    sub.matched.insert(id);
                    matched = true;
                    wants_values |= !sub.topics_only;
                }
            }
            if matched {
                targets.push((*cid, wants_values));
            }
        }
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

    /// Records that `client` has been announced `id`. Returns true when first
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
    let mut buf = Vec::with_capacity(VALUE_FRAME_HINT);
    encode_into(v, ts_micros, topic_id, &mut buf);
    Arc::from(buf)
}

/// Appends the NT4 value message for `v` to `buf`: the same bytes as
/// [`encode_once`], in a buffer the caller owns.
pub fn encode_into(v: &Value, ts_micros: u64, topic_id: u32, buf: &mut Vec<u8>) {
    encode_value_message(topic_id, ts_micros, xt_data_type(v), v, buf);
}

/// Bytes reserved for a value frame before its size is known, enough for the
/// header and any scalar.
pub const VALUE_FRAME_HINT: usize = 64;

#[cfg(test)]
mod tests;
