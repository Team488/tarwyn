//! NT4 registry and subscription semantics.
//!
//! [`NtRegistry`] owns the server's topic state: topic-id allocation and
//! reuse, publisher and subscriber tracking, retained-value caching, and the
//! NT4 control-message emit surface (announce/unannounce/properties/publish/
//! subscribe/unsubscribe). Handlers return queued [`Outbound`] frames keyed
//! by client so Task 5 (the fan-out loop) can flush them to the right
//! `WsConnection`s.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::value::XtValue;
use crate::websocket::message::{CtMessage, RTT_TOPIC_ID, ValueMessage};
use crate::websocket::msgpack::encode_meta_payload;

// Rust guideline compliant 2026-02-21

/// A numeric NT4 data type for a value.
///
/// Mirrors the NT4 4.1 type table: `0=bool`, `1=double`, `2=int`, `3=float`,
/// `4=str`, `5=bin`, with array suffixes offset by `+16`.
pub fn xt_data_type(v: &XtValue) -> u32 {
    match v {
        XtValue::Bool(_) => 0,
        XtValue::Double(_) => 1,
        XtValue::Float(_) => 3,
        XtValue::Int8(_) | XtValue::Uint8(_) => 2,
        XtValue::BoolArray(_) => 16,
        XtValue::DoubleArray(_) => 17,
        XtValue::FloatArray(_) => 19,
        XtValue::StringArray(_) => 20,
        XtValue::String(_) => 4,
        XtValue::Bytes(_) | XtValue::BytesList(_) | XtValue::Coordinate(_) | XtValue::Bezier(_) => {
            5
        }
        XtValue::Int16(_)
        | XtValue::Uint16(_)
        | XtValue::Int32(_)
        | XtValue::Uint32(_)
        | XtValue::Int64(_)
        | XtValue::Uint64(_) => 2,
        XtValue::Int8Array(_)
        | XtValue::Uint8Array(_)
        | XtValue::Int16Array(_)
        | XtValue::Uint16Array(_)
        | XtValue::Int32Array(_)
        | XtValue::Uint32Array(_)
        | XtValue::Int64Array(_)
        | XtValue::Uint64Array(_) => 18,
    }
}

/// A numeric NT4 data type as its canonical type string.
pub fn type_string(data_type: u32) -> Option<&'static str> {
    match data_type {
        0 => Some("boolean"),
        1 => Some("double"),
        2 => Some("int"),
        3 => Some("float"),
        4 => Some("string"),
        5 => Some("raw"),
        16 => Some("boolean[]"),
        17 => Some("double[]"),
        18 => Some("int[]"),
        19 => Some("float[]"),
        20 => Some("string[]"),
        _ => None,
    }
}

/// The numeric NT4 data type for a type string.
///
/// The reverse of [`type_string`]: maps `"double"` back to `1`, `"int[]"` to
/// `18`, and so on. Per NT4 §"Supported Data Types", any string not in the
/// table is carried as data type 5 (binary) — that is how `json`, `msgpack`,
/// `protobuf` and the `struct:*` families travel — so this never fails.
pub fn data_type_from_string(s: &str) -> u32 {
    match s {
        "boolean" => 0,
        "double" => 1,
        "int" => 2,
        "float" => 3,
        "string" | "json" => 4,
        "boolean[]" => 16,
        "double[]" => 17,
        "int[]" => 18,
        "float[]" => 19,
        "string[]" => 20,
        _ => 5,
    }
}

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

/// A timestamped retained value.
#[derive(Debug, Clone, PartialEq)]
pub struct StampedValue {
    /// Timestamp in microseconds.
    pub ts_micros: u64,
    /// The retained value.
    pub value: XtValue,
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
    /// Several strings share one numeric type — `struct:Pose2d`, `msgpack`
    /// and `raw` are all data type 5 — and clients need the original back to
    /// decode the payload, so it is stored rather than derived.
    pub type_str: String,
    /// Topic properties.
    pub properties: Map<String, Value>,
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
        self.properties
            .get("cached")
            .and_then(Value::as_bool)
            .unwrap_or(self.cached)
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
                    .and_then(Value::as_bool)
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
    pub options: Map<String, Value>,
}

/// Connection-scoped publish and subscribe state for one client.
#[derive(Debug, Default)]
struct ClientState {
    /// `pubuid -> topic id` for this client's live publishes.
    pubs: HashMap<u32, u32>,
    /// `subuid -> subscription`.
    subs: HashMap<u32, Subscription>,
    /// The original client name from the handshake (before deduplication).
    original_name: String,
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
    /// Live deduplicated client names (`base` -> count of `@N` suffixes used).
    client_names: HashMap<String, u32>,
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
        properties: Map<String, Value>,
    ) -> Vec<(ClientId, Outbound)> {
        self.ensure_client(client);
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
                    properties,
                    current: None,
                    publishers: 0,
                    retained: false,
                    cached: true,
                },
            );
            self.by_name.insert(name.to_string(), id);
            is_new = true;
            id
        };

        self.clients
            .get_mut(&client)
            .expect("client must exist")
            .pubs
            .insert(pubuid, id);
        if let Some(topic) = self.topics.get_mut(&id) {
            topic.publishers += 1;
        }

        self.add_announced(client, id);
        routes.push((client, Outbound::Text(self.announce_json(id, Some(pubuid)))));

        // A brand-new topic is announced without `pubuid` to every other
        // client whose subscription matches the name.
        if is_new {
            let others: Vec<ClientId> = self
                .clients
                .iter()
                .filter(|(cid, cs)| {
                    **cid != client
                        && cs
                            .subs
                            .values()
                            .any(|s| sub_visible_to(s, name) && sub_matches(s, name))
                })
                .map(|(cid, _)| *cid)
                .collect();
            for cid in others {
                if self.add_announced(cid, id) {
                    routes.push((cid, Outbound::Text(self.announce_json(id, None))));
                }
                self.add_subscriber(cid, id);
            }
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
        options: Map<String, Value>,
    ) -> Vec<(ClientId, Outbound)> {
        self.ensure_client(client);
        // Re-issuing the same `subuid` replaces the prior subscription.
        if let Some(prev) = self
            .clients
            .get_mut(&client)
            .and_then(|cs| cs.subs.remove(&subuid))
        {
            for id in prev.matched {
                if let Some(list) = self.topic_subscribers.get_mut(&id) {
                    list.retain(|c| *c != client);
                }
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
        routes.extend(self.update_meta_sub_all());
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
        if let Some(sub) = removed {
            for id in sub.matched {
                if let Some(list) = self.topic_subscribers.get_mut(&id) {
                    list.retain(|c| *c != client);
                }
            }
        }
        let mut routes = Vec::new();
        routes.extend(self.update_meta_clientsub(client));
        routes.extend(self.update_meta_sub_all());
        routes
    }

    /// Handles a client value update, returning the fan-out frames.
    pub fn handle_value(
        &mut self,
        client: ClientId,
        pubuid: u32,
        value: XtValue,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(topic_id) = self.topic_id_for_pubuid(client, pubuid) else {
            return Vec::new();
        };
        self.handle_topic_value(topic_id, value, ts_micros)
    }

    /// The topic a client's publisher UID publishes to, if the server knows it.
    ///
    /// NT4 binary frames from a client carry the publisher UID the client
    /// chose, not the server's topic id, and the server must ignore UIDs it
    /// never assigned.
    pub fn topic_id_for_pubuid(&self, client: ClientId, pubuid: u32) -> Option<u32> {
        self.clients.get(&client)?.pubs.get(&pubuid).copied()
    }

    /// Fans a value out on an already-resolved topic id.
    ///
    /// This is the server's own publish path; a client's value message must
    /// resolve its publisher UID through [`NtRegistry::handle_value`] first.
    pub fn handle_topic_value(
        &mut self,
        topic_id: u32,
        value: XtValue,
        ts_micros: u64,
    ) -> Vec<(ClientId, Outbound)> {
        let Some(topic) = self.topics.get(&topic_id) else {
            return Vec::new();
        };
        // A publisher whose data type does not match the topic is ignored.
        if xt_data_type(&value) != topic.data_type {
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
        let frame = Outbound::Value(encode_once(&value, ts_micros, topic_id));
        let subscribers = self
            .topic_subscribers
            .get(&topic_id)
            .cloned()
            .unwrap_or_default();
        subscribers
            .into_iter()
            .map(|c| (c, frame.clone()))
            .collect()
    }

    /// Ensures a topic exists for `name`, then handles a value update for it.
    ///
    /// Used by the control plane (CAS) where a value may be assigned to a
    /// channel no NT4 client has published yet. The topic is created with the
    /// value's data type so it is readable and subscribeable.
    pub fn handle_upsert_value(
        &mut self,
        name: &str,
        value: XtValue,
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
        self.handle_topic_value(id, value, ts_micros)
    }

    /// Handles a `setproperties`, broadcasting the update.
    pub fn handle_setproperties(
        &mut self,
        client: ClientId,
        name: &str,
        update: Map<String, Value>,
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
        client_value: XtValue,
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
    fn ensure_meta_topic(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.by_name.get(name) {
            return id;
        }
        let id = self.alloc_id();
        self.topics.insert(
            id,
            TopicState {
                name: name.to_string(),
                data_type: 5, // msgpack/raw
                type_str: "msgpack".to_string(),
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

    /// Registers a client connection, assigning a deduplicated name and
    /// creating its per-client meta topics.
    ///
    /// Returns the outbound frames to dispatch (meta-topic updates).
    pub fn on_connect(&mut self, client: ClientId, base_name: &str) -> Vec<(ClientId, Outbound)> {
        let name = self.dedup_client_name(base_name);
        self.client_name_by_id.insert(client, name);
        self.ensure_client(client);
        // Store original name in ClientState
        if let Some(cs) = self.clients.get_mut(&client) {
            cs.original_name = base_name.to_string();
        }
        let mut routes = Vec::new();
        routes.extend(self.update_meta_clients());
        routes.extend(self.update_meta_clientpub(client));
        routes.extend(self.update_meta_clientsub(client));
        routes.extend(self.update_meta_serversub());
        routes.extend(self.update_meta_serverpub());
        routes
    }

    /// Removes a client connection and its per-client meta topics.
    ///
    /// Returns the outbound frames to dispatch (meta-topic updates).
    pub fn on_disconnect(&mut self, client: ClientId) -> Vec<(ClientId, Outbound)> {
        let name = self.client_name_by_id.remove(&client);
        // Collect topic names the client was publishing to before removing client
        let pub_topic_names: Vec<String> = if let Some(cs) = self.clients.get(&client) {
            cs.pubs
                .values()
                .filter_map(|tid| self.topics.get(tid).map(|t| t.name.clone()))
                .collect()
        } else {
            Vec::new()
        };
        self.clients.remove(&client);
        if let Some(name) = name {
            self.release_client_name(&name);
            self.delete_topic_if_exists(&format!("$clientpub${name}"));
            self.delete_topic_if_exists(&format!("$clientsub${name}"));
        }
        let mut routes = Vec::new();
        routes.extend(self.update_meta_clients());
        routes.extend(self.update_meta_sub_all());
        // Update $pub$<topic> for topics the client was publishing to
        for topic_name in pub_topic_names {
            routes.extend(self.update_meta_pub(&topic_name));
        }
        routes
    }

    /// Assigns a unique client name, appending `@N` when `base` is taken.
    pub fn dedup_client_name(&mut self, base: &str) -> String {
        let count = self.client_names.entry(base.to_string()).or_insert(0);
        let n = *count;
        *count += 1;
        if n == 0 {
            base.to_string()
        } else {
            format!("{base}@{n}")
        }
    }

    fn release_client_name(&mut self, name: &str) {
        if let Some((base, suffix)) = name.rsplit_once('@')
            && suffix.chars().all(|c| c.is_ascii_digit())
        {
            if let Some(count) = self.client_names.get_mut(base) {
                *count = count.saturating_sub(1);
            }
            return;
        }
        self.client_names.remove(name);
    }

    /// Publishes a meta topic's array-of-maps payload and fans it out.
    fn publish_meta(
        &mut self,
        name: &str,
        maps: Vec<Map<String, Value>>,
    ) -> Vec<(ClientId, Outbound)> {
        let id = self.ensure_meta_topic(name);
        let bytes = encode_meta_payload(&maps);
        self.handle_topic_value(id, XtValue::Bytes(bytes), now_micros())
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
            m.insert("id".into(), Value::String(name.clone()));
            m.insert("conn".into(), Value::String(cs.original_name.clone()));
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
                m.insert("uid".into(), Value::from(*uid));
                if let Some(topic) = self.topics.get(tid) {
                    m.insert("topic".into(), Value::String(topic.name.clone()));
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
                m.insert("uid".into(), Value::from(*uid));
                m.insert(
                    "topics".into(),
                    Value::Array(sub.patterns.iter().cloned().map(Value::String).collect()),
                );
                m.insert("options".into(), Value::Object(sub.options.clone()));
                maps.push(m);
            }
        }
        self.publish_meta(&format!("$clientsub${name}"), maps)
    }

    /// Updates `$sub$<topic>` for every topic with its subscribers.
    fn update_meta_sub_all(&mut self) -> Vec<(ClientId, Outbound)> {
        let mut routes = Vec::new();
        let topic_names: Vec<String> = self
            .topics
            .values()
            .filter(|t| !is_meta_topic(&t.name))
            .map(|t| t.name.clone())
            .collect();
        for name in topic_names {
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
            // Find the subscription for this topic to get subuid and options
            if let Some(cs) = self.clients.get(&cid) {
                for (subuid, sub) in &cs.subs {
                    if sub.matched.contains(&id) {
                        let mut m = Map::new();
                        m.insert("client".into(), Value::String(client_name.clone()));
                        m.insert("subuid".into(), Value::from(*subuid));
                        m.insert("options".into(), Value::Object(sub.options.clone()));
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
                    m.insert("client".into(), Value::String(client_name));
                    m.insert("pubuid".into(), Value::from(*uid));
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
        // Also delete the corresponding meta-topics
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
        CtMessage::Announce {
            name: topic.name.clone(),
            id,
            data_type: topic.type_str.clone(),
            properties: topic.properties.clone(),
            pubuid,
        }
        .to_json()
    }

    fn unannounce_json(&self, id: u32, name: &str) -> String {
        CtMessage::Unannounce {
            name: name.to_string(),
            id,
        }
        .to_json()
    }

    fn properties_json(
        &self,
        name: &str,
        update: &Map<String, Value>,
        ack: Option<bool>,
    ) -> String {
        CtMessage::PropertiesUpdate {
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
pub fn encode_once(v: &XtValue, ts_micros: u64, topic_id: u32) -> Arc<[u8]> {
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
mod tests {
    use super::{NtRegistry, Outbound, data_type_from_string, encode_once, type_string};
    use crate::value::XtValue;
    use crate::websocket::message::RTT_TOPIC_ID;
    use serde_json::{Value, json};

    fn texts(routes: &[(u64, Outbound)]) -> Vec<(u64, Value)> {
        routes
            .iter()
            .filter_map(|(c, o)| match o {
                Outbound::Text(s) => {
                    let frame: Value = serde_json::from_str(s).expect("valid control json");
                    let mut msgs = match frame {
                        Value::Array(items) => items,
                        other => vec![other],
                    };
                    assert_eq!(msgs.len(), 1, "one control message per route");
                    Some((*c, msgs.remove(0)))
                }
                Outbound::Value(_) => None,
            })
            .collect()
    }

    fn values(routes: &[(u64, Outbound)]) -> Vec<(u64, &[u8])> {
        routes
            .iter()
            .filter_map(|(c, o)| match o {
                Outbound::Value(b) => Some((*c, b.as_ref())),
                Outbound::Text(_) => None,
            })
            .collect()
    }

    #[test]
    fn type_strings_match_numeric_table() {
        for (data_type, name) in [
            (0, "boolean"),
            (1, "double"),
            (2, "int"),
            (3, "float"),
            (4, "string"),
            (5, "raw"),
            (16, "boolean[]"),
            (17, "double[]"),
            (18, "int[]"),
            (19, "float[]"),
            (20, "string[]"),
        ] {
            assert_eq!(type_string(data_type), Some(name));
            assert_eq!(data_type_from_string(name), data_type);
        }
        assert_eq!(type_string(99), None);
    }

    #[test]
    fn unknown_type_strings_are_carried_as_binary() {
        for name in [
            "json",
            "msgpack",
            "protobuf",
            "rpc",
            "struct:Pose2d",
            "structschema",
        ] {
            let expected = if name == "json" { 4 } else { 5 };
            assert_eq!(
                data_type_from_string(name),
                expected,
                "NT4 carries any type string outside the table as binary"
            );
        }
    }

    #[test]
    fn announce_echoes_the_publishers_own_type_string() {
        let mut reg = NtRegistry::new();
        let routes = reg.handle_publish(1, "pose", 1, "struct:Pose2d", serde_json::Map::new());
        let t = texts(&routes);
        assert_eq!(
            t[0].1["params"]["type"], "struct:Pose2d",
            "a struct topic must announce its own type string, not \"raw\""
        );
    }

    #[test]
    fn encode_once_is_wire_exact_4tuple() {
        // Mirror of message.rs's NT4 golden: id=50, ts=0x07270E00, type=1,
        // double 5.545.
        let bytes = encode_once(&XtValue::Double(5.545), 0x0727_0E00, 50);
        assert_eq!(
            bytes.as_ref(),
            [
                0x94, 0x32, 0xd2, 0x07, 0x27, 0x0e, 0x00, 0x01, 0xcb, 0x40, 0x16, 0x2e, 0x14, 0x7a,
                0xe1, 0x47, 0xae,
            ]
        );
    }

    #[test]
    fn publish_announces_with_pubuid_and_creates_topic() {
        let mut reg = NtRegistry::new();
        let routes = reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        assert_eq!(
            texts(&routes),
            vec![(
                1,
                json!({"method":"announce","params":{
                    "id":0,"name":"gyro","properties":{},"type":"double","pubuid":7
                }})
            )]
        );
    }

    #[test]
    fn duplicate_publish_reuses_same_id_and_reamounces() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        let routes = reg.handle_publish(1, "gyro", 8, "double", serde_json::Map::new());
        let t = texts(&routes);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].0, 1);
        assert_eq!(
            t[0].1["params"]["id"], 0,
            "duplicate publish must reuse id 0"
        );
        assert_eq!(t[0].1["params"]["pubuid"], 8);
    }

    #[test]
    fn unpublish_deletes_when_last_publisher_and_unannounces() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        let routes = reg.handle_unpublish(1, 7);
        assert_eq!(
            texts(&routes),
            vec![(
                1,
                json!({"method":"unannounce","params":{"id":0,"name":"gyro"}})
            )]
        );
    }

    #[test]
    fn persistent_property_survives_last_publisher() {
        for key in ["persistent", "retained"] {
            let mut reg = NtRegistry::new();
            reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
            let mut update = serde_json::Map::new();
            update.insert(key.into(), json!(true));
            reg.handle_setproperties(1, "gyro", update);
            let routes = reg.handle_unpublish(1, 7);
            assert!(
                texts(&routes).is_empty(),
                "a topic marked {key} must not unannounce on last publisher"
            );
        }
    }

    #[test]
    fn publish_time_persistent_property_survives_last_publisher() {
        let mut reg = NtRegistry::new();
        let mut props = serde_json::Map::new();
        props.insert("persistent".into(), json!(true));
        reg.handle_publish(1, "gyro", 7, "double", props);
        let routes = reg.handle_unpublish(1, 7);
        assert!(
            texts(&routes).is_empty(),
            "persistent set at publish time must also keep the topic"
        );
    }

    #[test]
    fn retained_topic_survives_last_publisher() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        reg.set_retained("gyro", true);
        let routes = reg.handle_unpublish(1, 7);
        assert!(
            texts(&routes).is_empty(),
            "retained topic must not unannounce on last publisher"
        );
    }

    #[test]
    fn topic_id_reused_after_delete() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "a", 1, "double", serde_json::Map::new());
        reg.handle_unpublish(1, 1);
        let routes = reg.handle_publish(1, "b", 2, "double", serde_json::Map::new());
        let t = texts(&routes);
        assert_eq!(t[0].1["params"]["id"], 0, "freed id 0 must be reused");
        assert_eq!(t[0].1["params"]["name"], "b");
    }

    #[test]
    fn multiple_subscribers_receive_value() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
        reg.handle_subscribe(
            2,
            &["child".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        reg.handle_subscribe(
            3,
            &["child".to_string()],
            11,
            false,
            false,
            serde_json::Map::new(),
        );
        let routes = reg.handle_value(1, 1, XtValue::Double(1.5), 100);
        let v = values(&routes);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, 2);
        assert_eq!(v[1].0, 3);
        assert_eq!(v[0].1, encode_once(&XtValue::Double(1.5), 100, 0).as_ref());
    }

    #[test]
    fn data_type_mismatch_ignores_value() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
        let routes = reg.handle_value(1, 1, XtValue::Int32(7), 100);
        assert!(
            routes.is_empty(),
            "mismatched data_type value must be ignored"
        );
    }

    #[test]
    fn properties_update_ack_only_to_same_client() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        reg.handle_subscribe(
            2,
            &["gyro".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        let mut update = serde_json::Map::new();
        update.insert("unit".into(), json!("deg"));
        let routes = reg.handle_setproperties(1, "gyro", update);
        let t = texts(&routes);
        assert_eq!(t.len(), 2);
        let with_ack = t.iter().find(|(c, _)| *c == 1).expect("publisher ack");
        assert_eq!(
            with_ack.1,
            json!({"method":"properties","params":{
                "name":"gyro","update":{"unit":"deg"},"ack":true
            }})
        );
        let no_ack = t.iter().find(|(c, _)| *c == 2).expect("subscriber");
        assert_eq!(
            no_ack.1,
            json!({"method":"properties","params":{
                "name":"gyro","update":{"unit":"deg"}
            }})
        );
    }

    #[test]
    fn subscribe_sends_retained_value() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
        reg.handle_value(1, 1, XtValue::Double(1.5), 100);
        let routes = reg.handle_subscribe(
            2,
            &["child".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        let v = values(&routes);
        assert_eq!(
            v,
            vec![(2, encode_once(&XtValue::Double(1.5), 100, 0).as_ref())]
        );
    }

    #[test]
    fn prefix_subscribe_receives_announce_for_new_topic_without_pubuid() {
        let mut reg = NtRegistry::new();
        reg.handle_subscribe(
            2,
            &["gyro".to_string()],
            10,
            true,
            false,
            serde_json::Map::new(),
        );
        let routes = reg.handle_publish(1, "gyro/yaw", 7, "double", serde_json::Map::new());
        let t = texts(&routes);
        assert_eq!(t.len(), 2);
        let sub = t
            .iter()
            .find(|(c, _)| *c == 2)
            .expect("prefix subscriber must be announced");
        assert!(
            sub.1["params"].get("pubuid").is_none(),
            "no pubuid on shared announce"
        );
        assert_eq!(sub.1["params"]["name"], "gyro/yaw");
        assert_eq!(sub.1["params"]["type"], "double");
    }

    #[test]
    fn timestamp_echoes_id_minus_one_with_server_time() {
        let mut reg = NtRegistry::new();
        let routes = reg.handle_timestamp(1, XtValue::Double(1.5), 1234);
        let v = values(&routes);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, 1);
        assert_eq!(
            v[0].1,
            encode_once(&XtValue::Double(1.5), 1234, RTT_TOPIC_ID).as_ref()
        );
    }

    #[test]
    fn a_topicsonly_subscriber_is_announced_but_sent_no_values() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        reg.handle_value(1, 7, XtValue::Double(1.5), 100);
        let routes = reg.handle_subscribe(
            2,
            &["gyro".to_string()],
            10,
            false,
            true,
            serde_json::Map::new(),
        );
        assert_eq!(
            texts(&routes).len(),
            1,
            "a topicsonly subscriber is still announced"
        );
        assert!(
            values(&routes).is_empty(),
            "a topicsonly subscriber must not receive the retained value"
        );
        let routes = reg.handle_value(1, 7, XtValue::Double(2.5), 200);
        assert!(
            values(&routes).is_empty(),
            "a topicsonly subscriber must not receive later values either"
        );
    }

    #[test]
    fn an_uncached_topic_replays_nothing_to_a_late_subscriber() {
        let mut reg = NtRegistry::new();
        let mut props = serde_json::Map::new();
        props.insert("cached".into(), json!(false));
        reg.handle_publish(1, "gyro", 7, "double", props);
        reg.handle_value(1, 7, XtValue::Double(1.5), 100);
        let routes = reg.handle_subscribe(
            2,
            &["gyro".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        assert!(
            values(&routes).is_empty(),
            "an uncached topic must not retain a value for late subscribers"
        );
    }

    #[test]
    fn a_null_property_update_deletes_the_property() {
        let mut reg = NtRegistry::new();
        let mut props = serde_json::Map::new();
        props.insert("unit".into(), json!("deg"));
        reg.handle_publish(1, "gyro", 7, "double", props);
        let mut update = serde_json::Map::new();
        update.insert("unit".into(), Value::Null);
        reg.handle_setproperties(1, "gyro", update);
        let routes = reg.handle_subscribe(
            2,
            &["gyro".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        let announce = &texts(&routes)[0].1;
        assert!(
            announce["params"]["properties"].get("unit").is_none(),
            "a null update must delete the property, not store a json null"
        );
    }

    #[test]
    fn unsubscribe_removes_fan_out() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
        reg.handle_subscribe(
            2,
            &["child".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        reg.handle_unsubscribe(2, 10);
        let routes = reg.handle_value(1, 7, XtValue::Double(1.5), 100);
        assert!(
            routes.is_empty(),
            "no subscribers must receive the value after unsubscribe"
        );
    }

    #[test]
    fn stale_pubuid_unpublish_after_topic_deleted_is_ignored() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        reg.handle_publish(2, "gyro", 9, "double", serde_json::Map::new());
        // Client 1 unannounces, deleting the topic while client 2's pubuid
        // still points at it.
        reg.handle_unannounce(1, "gyro");
        let routes = reg.handle_unpublish(2, 9);
        assert!(
            routes.is_empty(),
            "stale pubuid after topic deletion must be ignored, not panic"
        );
    }

    #[test]
    fn prefix_subscriber_receives_values_on_new_topic_without_resubscribe() {
        let mut reg = NtRegistry::new();
        reg.handle_subscribe(
            1,
            &["/robot".to_string()],
            1,
            true,
            false,
            serde_json::Map::new(),
        );
        let routes = reg.handle_publish(2, "/robot/arm", 9, "double", serde_json::Map::new());
        let t = texts(&routes);
        assert!(
            t.iter().any(|(c, _)| *c == 1),
            "prefix subscriber must be announced for the new topic"
        );
        let routes = reg.handle_value(2, 9, XtValue::Double(1.5), 100);
        let v = values(&routes);
        assert!(
            v.iter().any(|(c, _)| *c == 1),
            "prefix subscriber must receive values without re-subscribing"
        );
    }

    #[test]
    fn publisher_receives_own_value_when_subscribed() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "x", 7, "double", serde_json::Map::new());
        reg.handle_subscribe(
            1,
            &["x".to_string()],
            1,
            false,
            false,
            serde_json::Map::new(),
        );
        let routes = reg.handle_value(1, 7, XtValue::Double(1.5), 100);
        let v = values(&routes);
        assert!(
            v.iter().any(|(c, _)| *c == 1),
            "a subscribed publisher must receive its own value"
        );
    }

    #[test]
    fn explicit_unannounce_removes_topic() {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        let t = texts(&reg.handle_unannounce(1, "gyro"));
        assert_eq!(
            t,
            vec![(
                1,
                json!({"method":"unannounce","params":{"id":0,"name":"gyro"}})
            )]
        );
    }

    #[test]
    fn on_connect_creates_meta_topics() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        for name in [
            "$clients",
            "$clientpub$robot",
            "$clientsub$robot",
            "$serversub",
            "$serverpub",
        ] {
            assert!(
                reg.topic_id(name).is_some(),
                "meta topic {name} must exist after connect"
            );
        }
        assert_eq!(reg.client_name(1), Some("robot"));
    }

    #[test]
    fn dedup_client_name_appends_at_n() {
        let mut reg = NtRegistry::new();
        assert_eq!(reg.dedup_client_name("robot"), "robot");
        assert_eq!(reg.dedup_client_name("robot"), "robot@1");
        assert_eq!(reg.dedup_client_name("robot"), "robot@2");
    }

    #[test]
    fn meta_topics_hidden_from_empty_prefix_subscribers() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        // Empty-prefix subscriber must not see meta topics.
        let routes = reg.handle_subscribe(
            2,
            &["".to_string()],
            10,
            true,
            false,
            serde_json::Map::new(),
        );
        let t = texts(&routes);
        assert!(
            t.iter()
                .all(|(_, m)| !m["params"]["name"].as_str().unwrap().starts_with('$')),
            "empty-prefix subscriber must not be announced meta topics"
        );
    }

    #[test]
    fn meta_topics_visible_to_dollar_subscribers() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        let routes = reg.handle_subscribe(
            2,
            &["$".to_string()],
            10,
            true,
            false,
            serde_json::Map::new(),
        );
        let t = texts(&routes);
        assert!(
            t.iter()
                .any(|(_, m)| m["params"]["name"].as_str().unwrap().starts_with('$')),
            "a $ subscriber must be announced meta topics"
        );
    }

    #[test]
    fn publish_updates_clientpub_and_pub_meta() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        assert!(
            reg.topic_id("$clientpub$robot").is_some(),
            "$clientpub$robot must exist"
        );
        assert!(reg.topic_id("$pub$gyro").is_some(), "$pub$gyro must exist");
    }

    #[test]
    fn subscribe_updates_clientsub_and_sub_meta() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        reg.handle_subscribe(
            1,
            &["gyro".to_string()],
            10,
            false,
            false,
            serde_json::Map::new(),
        );
        assert!(
            reg.topic_id("$clientsub$robot").is_some(),
            "$clientsub$robot must exist"
        );
        assert!(reg.topic_id("$sub$gyro").is_some(), "$sub$gyro must exist");
    }

    #[test]
    fn on_disconnect_removes_per_client_meta_topics() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        reg.on_disconnect(1);
        assert!(
            reg.topic_id("$clientpub$robot").is_none(),
            "$clientpub$robot must be removed on disconnect"
        );
        assert!(
            reg.topic_id("$clientsub$robot").is_none(),
            "$clientsub$robot must be removed on disconnect"
        );
        assert!(
            reg.topic_id("$clients").is_some(),
            "$clients must survive a single disconnect"
        );
    }

    #[test]
    fn meta_payload_is_msgpack_array_of_maps() {
        let mut reg = NtRegistry::new();
        reg.on_connect(1, "robot");
        let id = reg.topic_id("$clients").unwrap();
        let topic = reg.topics.get(&id).unwrap();
        assert_eq!(topic.type_str, "msgpack");
        assert!(topic.retained, "meta topics must be retained");
        assert!(topic.current.is_some(), "meta topics must cache a value");
        let bytes = match &topic.current.as_ref().unwrap().value {
            XtValue::Bytes(b) => b.clone(),
            other => panic!("meta value must be Bytes, got {other:?}"),
        };
        // fixarray(1) then fixmap(2): {id, conn}
        assert_eq!(bytes[0] & 0xf0, 0x90, "payload must be an array");
        assert_eq!(bytes[1] & 0xf0, 0x80, "element must be a map");
    }
}
