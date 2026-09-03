//! The NT4 protocol message model.
//!
//! [`CtMessage`] is the JSON control-message form and [`ValueMessage`] the
//! MessagePack value-message form; the codec lives in
//! [`crate::websocket::msgpack`] and the value type in [`crate::value`].

// Rust guideline compliant 2026-02-21

use std::fmt;

use serde_json::{Map, Value};

use crate::value::XtValue;
use crate::websocket::msgpack::{self, MsgpackError};

/// An error from parsing or serializing a [`CtMessage`].
///
/// Carries a human-readable message; no payload is needed beyond that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtMessageError {
    message: String,
}

impl CtMessageError {
    /// A generic error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The JSON document was not an object.
    pub fn not_an_object() -> Self {
        Self::new("control message is not a JSON object")
    }

    /// A required parameter was absent.
    pub fn missing(field: &str) -> Self {
        Self::new(format!("missing parameter: {field}"))
    }

    /// A parameter was present with the wrong type.
    pub fn wrong_type(field: &str) -> Self {
        Self::new(format!("parameter has the wrong type: {field}"))
    }

    /// The `method` value is not a known control message.
    pub fn unknown_method(method: &str) -> Self {
        Self::new(format!("unknown method: {method}"))
    }
}

impl fmt::Display for CtMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CtMessageError {}

/// An NT4 control message, carried as JSON.
///
/// Field names and shapes follow the NT4 4.1 spec (`/tmp/opencode/nt4.adoc`).
/// `ControlValue`, `Timestamp` and `KeepAlive` are this crate's JSON forms of
/// the spec's MessagePack topic-id -1 timestamp exchange and WebSocket ping
/// keepalive.
#[derive(Debug, Clone, PartialEq)]
pub enum CtMessage {
    /// Topic announcement (server to client).
    Announce {
        /// Topic name.
        name: String,
        /// Topic ID used in MessagePack messages.
        id: u32,
        /// Data type as a string (e.g. `"double"`).
        data_type: String,
        /// Topic properties.
        properties: Map<String, Value>,
        /// Publisher UID, present when answering a `publish`.
        pubuid: Option<u32>,
    },
    /// Topic removed (server to client).
    Unannounce {
        /// Topic name.
        name: String,
        /// Topic ID that was in use.
        id: u32,
    },
    /// A client's request to change a topic's properties.
    ///
    /// The client-to-server direction; the server answers matching clients
    /// with [`CtMessage::PropertiesUpdate`].
    SetProperties {
        /// Topic name.
        name: String,
        /// Properties to set; a null value removes the property.
        update: Map<String, Value>,
    },
    /// Topic properties changed (server to client).
    PropertiesUpdate {
        /// Topic name.
        name: String,
        /// Properties to update.
        update: Map<String, Value>,
        /// True when answering a `setproperties` from the same client.
        ack: Option<bool>,
    },
    /// Publish request (client to server).
    Publish {
        /// Topic name.
        name: String,
        /// Publisher UID, used in MessagePack messages.
        pubuid: u32,
        /// Requested data type as a string.
        data_type: String,
        /// Initial topic properties.
        properties: Map<String, Value>,
    },
    /// Publish release (client to server).
    Unpublish {
        /// Publisher UID from the matching `publish`.
        pubuid: u32,
    },
    /// Subscribe request (client to server).
    Subscribe {
        /// Topic names or prefixes.
        topics: Vec<String>,
        /// Subscription UID.
        subuid: u32,
        /// Subscription options.
        options: Map<String, Value>,
    },
    /// Unsubscribe request (client to server).
    Unsubscribe {
        /// Subscription UID from the matching `subscribe`.
        subuid: u32,
    },
    /// A control value for a topic.
    ///
    /// Crate-internal lowercase JSON method (`"controlvalue"`), not an
    /// NT4-standard method: the spec sends timestamps as MessagePack topic-id
    /// -1 and keepalives as WebSocket pings.
    ControlValue {
        /// Topic ID.
        topic_id: u32,
        /// The value.
        value: Value,
    },
    /// A timestamp exchange.
    ///
    /// Crate-internal lowercase JSON method (`"timestamp"`), not an
    /// NT4-standard method: the spec sends timestamps as MessagePack topic-id
    /// -1 and keepalives as WebSocket pings.
    Timestamp {
        /// Timestamp in microseconds.
        timestamp: u64,
        /// The value.
        value: Value,
    },
    /// A keepalive.
    ///
    /// Crate-internal lowercase JSON method (`"keepalive"`), not an
    /// NT4-standard method: the spec sends timestamps as MessagePack topic-id
    /// -1 and keepalives as WebSocket pings.
    KeepAlive,
}

impl CtMessage {
    /// Parses a control message from its JSON form.
    ///
    /// Accepts both the NT4 text-frame form (a JSON array holding exactly one
    /// message) and a bare message object.
    ///
    /// # Errors
    ///
    /// Returns [`CtMessageError`] when the JSON is malformed, holds more than
    /// one message, or names an unknown method.
    pub fn from_json(json: &str) -> Result<Self, CtMessageError> {
        let mut batch = Self::from_json_batch(json)?;
        if batch.len() != 1 {
            return Err(CtMessageError::new(format!(
                "expected exactly one control message, got {}",
                batch.len()
            )));
        }
        Ok(batch.remove(0))
    }

    /// Parses every control message in one NT4 text frame.
    ///
    /// An NT4 text frame is a JSON array of message objects; a bare object is
    /// accepted as a one-element frame. Individual messages that are not
    /// objects, lack `method` or `params`, or name a method outside the NT4
    /// table are skipped rather than failing the frame that carried them, as
    /// NT4 §"Text Data Frames" requires.
    ///
    /// # Errors
    ///
    /// Returns [`CtMessageError`] when the frame itself is not valid JSON.
    pub fn from_json_batch(json: &str) -> Result<Vec<Self>, CtMessageError> {
        let root: Value = serde_json::from_str(json)
            .map_err(|e| CtMessageError::new(format!("invalid json: {e}")))?;
        match root {
            Value::Array(items) => Ok(items
                .iter()
                .filter_map(|i| Self::from_value(i).ok())
                .collect()),
            other => Ok(Self::from_value(&other).ok().into_iter().collect()),
        }
    }

    fn from_value(root: &Value) -> Result<Self, CtMessageError> {
        let obj = root.as_object().ok_or_else(CtMessageError::not_an_object)?;
        let method = obj
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| CtMessageError::missing("method"))?;
        let params = obj
            .get("params")
            .and_then(Value::as_object)
            .ok_or_else(|| CtMessageError::missing("params"))?;
        match method {
            "announce" => Ok(CtMessage::Announce {
                name: get_string(params, "name")?,
                id: get_u32(params, "id")?,
                data_type: get_string(params, "type")?,
                properties: get_map(params, "properties")?,
                pubuid: get_optional_u32(params, "pubuid")?,
            }),
            "unannounce" => Ok(CtMessage::Unannounce {
                name: get_string(params, "name")?,
                id: get_u32(params, "id")?,
            }),
            "properties" => Ok(CtMessage::PropertiesUpdate {
                name: get_string(params, "name")?,
                update: get_map(params, "update")?,
                ack: get_optional_bool(params, "ack")?,
            }),
            "setproperties" => Ok(CtMessage::SetProperties {
                name: get_string(params, "name")?,
                update: get_map(params, "update")?,
            }),
            "publish" => Ok(CtMessage::Publish {
                name: get_string(params, "name")?,
                pubuid: get_u32(params, "pubuid")?,
                data_type: get_string(params, "type")?,
                properties: get_map(params, "properties")?,
            }),
            "unpublish" => Ok(CtMessage::Unpublish {
                pubuid: get_u32(params, "pubuid")?,
            }),
            "subscribe" => Ok(CtMessage::Subscribe {
                topics: get_string_array(params, "topics")?,
                subuid: get_u32(params, "subuid")?,
                options: get_map(params, "options")?,
            }),
            "unsubscribe" => Ok(CtMessage::Unsubscribe {
                subuid: get_u32(params, "subuid")?,
            }),
            "controlvalue" => Ok(CtMessage::ControlValue {
                topic_id: get_u32(params, "topic_id")?,
                value: params
                    .get("value")
                    .cloned()
                    .ok_or_else(|| CtMessageError::missing("value"))?,
            }),
            "timestamp" => Ok(CtMessage::Timestamp {
                timestamp: get_u64(params, "timestamp")?,
                value: params
                    .get("value")
                    .cloned()
                    .ok_or_else(|| CtMessageError::missing("value"))?,
            }),
            "keepalive" => Ok(CtMessage::KeepAlive),
            other => Err(CtMessageError::unknown_method(other)),
        }
    }

    /// Serializes the control message to its NT4 text-frame form.
    ///
    /// An NT4 text frame is a JSON array of message objects, so the output is
    /// a one-element array.
    pub fn to_json(&self) -> String {
        let mut params = Map::new();
        let method = match self {
            CtMessage::Announce {
                name,
                id,
                data_type,
                properties,
                pubuid,
            } => {
                params.insert("name".into(), Value::String(name.clone()));
                params.insert("id".into(), Value::from(*id));
                params.insert("type".into(), Value::String(data_type.clone()));
                params.insert("properties".into(), Value::Object(properties.clone()));
                if let Some(pubuid) = pubuid {
                    params.insert("pubuid".into(), Value::from(*pubuid));
                }
                "announce"
            }
            CtMessage::Unannounce { name, id } => {
                params.insert("name".into(), Value::String(name.clone()));
                params.insert("id".into(), Value::from(*id));
                "unannounce"
            }
            CtMessage::PropertiesUpdate { name, update, ack } => {
                params.insert("name".into(), Value::String(name.clone()));
                params.insert("update".into(), Value::Object(update.clone()));
                if let Some(ack) = ack {
                    params.insert("ack".into(), Value::from(*ack));
                }
                "properties"
            }
            CtMessage::Publish {
                name,
                pubuid,
                data_type,
                properties,
            } => {
                params.insert("name".into(), Value::String(name.clone()));
                params.insert("pubuid".into(), Value::from(*pubuid));
                params.insert("type".into(), Value::String(data_type.clone()));
                params.insert("properties".into(), Value::Object(properties.clone()));
                "publish"
            }
            CtMessage::Unpublish { pubuid } => {
                params.insert("pubuid".into(), Value::from(*pubuid));
                "unpublish"
            }
            CtMessage::Subscribe {
                topics,
                subuid,
                options,
            } => {
                params.insert(
                    "topics".into(),
                    Value::Array(topics.iter().cloned().map(Value::String).collect()),
                );
                params.insert("subuid".into(), Value::from(*subuid));
                params.insert("options".into(), Value::Object(options.clone()));
                "subscribe"
            }
            CtMessage::Unsubscribe { subuid } => {
                params.insert("subuid".into(), Value::from(*subuid));
                "unsubscribe"
            }
            CtMessage::ControlValue { topic_id, value } => {
                params.insert("topic_id".into(), Value::from(*topic_id));
                params.insert("value".into(), value.clone());
                "controlvalue"
            }
            CtMessage::Timestamp { timestamp, value } => {
                params.insert("timestamp".into(), Value::from(*timestamp));
                params.insert("value".into(), value.clone());
                "timestamp"
            }
            CtMessage::SetProperties { name, update } => {
                params.insert("name".into(), Value::String(name.clone()));
                params.insert("update".into(), Value::Object(update.clone()));
                "setproperties"
            }
            CtMessage::KeepAlive => "keepalive",
        };
        let mut root = Map::new();
        root.insert("method".into(), Value::String(method.into()));
        root.insert("params".into(), Value::Object(params));
        Value::Array(vec![Value::Object(root)]).to_string()
    }
}

/// The reserved topic id for NT4 RTT/timestamp messages.
///
/// The wire form is the signed integer `-1`; this crate carries it as the
/// `u32` sentinel so topic ids stay unsigned everywhere else.
pub const RTT_TOPIC_ID: u32 = u32::MAX;

/// An NT4 value message: the MessagePack 4-tuple
/// `[topic_id, timestamp_micros, data_type, value]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueMessage {
    /// Topic (or publisher) ID.
    pub topic_id: u32,
    /// Timestamp in microseconds.
    pub timestamp_micros: u64,
    /// Numeric data type.
    pub data_type: u32,
    /// The value.
    pub value: XtValue,
}

impl ValueMessage {
    /// Encodes the message as a MessagePack fixarray(4).
    ///
    /// [`RTT_TOPIC_ID`] is written as the wire's reserved `-1`.
    pub fn encode(&self, buf: &mut Vec<u8>) {
        msgpack::encode_array_header(4, buf)
            .expect("encoding a 4-element array header is infallible");
        if self.topic_id == RTT_TOPIC_ID {
            msgpack::encode_int(-1, buf).expect("encoding an i64 is infallible");
        } else {
            msgpack::encode_uint(self.topic_id as u64, buf).expect("encoding a u64 is infallible");
        }
        msgpack::encode_uint(self.timestamp_micros, buf).expect("encoding a u64 is infallible");
        msgpack::encode_uint(self.data_type as u64, buf).expect("encoding a u64 is infallible");
        msgpack::encode_value(&self.value, buf).expect("encoding an XtValue is infallible");
    }

    /// Decodes a value message from its MessagePack form.
    ///
    /// The input must be exactly one 4-element array; trailing bytes are an
    /// error. Use [`ValueMessage::decode_all`] for a frame that carries more
    /// than one message.
    pub fn decode(buf: &[u8]) -> Result<Self, MsgpackError> {
        let (msg, consumed) = Self::decode_one(buf)?;
        if consumed != buf.len() {
            return Err(MsgpackError::trailing_bytes());
        }
        Ok(msg)
    }

    /// Decodes every value message in one binary frame.
    ///
    /// NT4 binary frames carry one or more complete MessagePack arrays, so a
    /// conforming client may batch several value updates into a single frame.
    ///
    /// # Errors
    ///
    /// Returns the first [`MsgpackError`] from decoding, and an error for an
    /// empty frame.
    pub fn decode_all(buf: &[u8]) -> Result<Vec<Self>, MsgpackError> {
        let mut rest = buf;
        let mut out = Vec::new();
        while !rest.is_empty() {
            let (msg, consumed) = Self::decode_one(rest)?;
            out.push(msg);
            rest = &rest[consumed..];
        }
        if out.is_empty() {
            return Err(MsgpackError::unexpected_eof());
        }
        Ok(out)
    }

    fn decode_one(buf: &[u8]) -> Result<(Self, usize), MsgpackError> {
        let (items, consumed) = msgpack::decode_array(buf)?;
        if items.len() != 4 {
            return Err(MsgpackError::wrong_array_len(4, items.len()));
        }
        let topic_id = match items[0].as_i64() {
            Some(-1) => RTT_TOPIC_ID,
            _ => u32::try_from(
                items[0]
                    .as_u64_any()
                    .ok_or_else(MsgpackError::not_an_integer)?,
            )
            .map_err(|_| MsgpackError::out_of_range("topic id"))?,
        };
        Ok((
            ValueMessage {
                topic_id,
                timestamp_micros: items[1]
                    .as_u64_any()
                    .ok_or_else(MsgpackError::not_an_integer)?,
                data_type: u32::try_from(
                    items[2]
                        .as_u64_any()
                        .ok_or_else(MsgpackError::not_an_integer)?,
                )
                .map_err(|_| MsgpackError::out_of_range("data type"))?,
                value: items[3].clone(),
            },
            consumed,
        ))
    }
}

fn get_string(params: &Map<String, Value>, key: &str) -> Result<String, CtMessageError> {
    match params.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(CtMessageError::wrong_type(key)),
        None => Err(CtMessageError::missing(key)),
    }
}

fn get_u32(params: &Map<String, Value>, key: &str) -> Result<u32, CtMessageError> {
    match params.get(key) {
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .ok_or_else(|| CtMessageError::wrong_type(key)),
        Some(_) => Err(CtMessageError::wrong_type(key)),
        None => Err(CtMessageError::missing(key)),
    }
}

fn get_u64(params: &Map<String, Value>, key: &str) -> Result<u64, CtMessageError> {
    match params.get(key) {
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| CtMessageError::wrong_type(key)),
        Some(_) => Err(CtMessageError::wrong_type(key)),
        None => Err(CtMessageError::missing(key)),
    }
}

fn get_optional_u32(params: &Map<String, Value>, key: &str) -> Result<Option<u32>, CtMessageError> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .map(Some)
            .ok_or_else(|| CtMessageError::wrong_type(key)),
        Some(_) => Err(CtMessageError::wrong_type(key)),
    }
}

fn get_optional_bool(
    params: &Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, CtMessageError> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(CtMessageError::wrong_type(key)),
    }
}

fn get_map(params: &Map<String, Value>, key: &str) -> Result<Map<String, Value>, CtMessageError> {
    match params.get(key) {
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(_) => Err(CtMessageError::wrong_type(key)),
        None => Err(CtMessageError::missing(key)),
    }
}

fn get_string_array(params: &Map<String, Value>, key: &str) -> Result<Vec<String>, CtMessageError> {
    match params.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| CtMessageError::wrong_type(key))
            })
            .collect(),
        Some(_) => Err(CtMessageError::wrong_type(key)),
        None => Err(CtMessageError::missing(key)),
    }
}

#[cfg(test)]
mod tests {
    use crate::value::XtValue;
    use crate::websocket::message::{CtMessage, ValueMessage};

    fn hex_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn golden_vector_nt4() {
        // The 4.1 symbolic golden. The wire bytes `40 16 2E 14 7A E1 47 AE` decode
        // to the double 5.545, not 4.344505251111111 as the brief prose claimed
        // (that value encodes as `40 11 60 C5 FC 0B 4A 3B`). The wire bytes are
        // authoritative; the assertion below matches them.
        let wire = hex_bytes("9432d207270e0001cb40162e147ae147ae");
        let m = ValueMessage::decode(&wire).unwrap();
        assert_eq!(m.topic_id, 50);
        assert_eq!(m.timestamp_micros, 0x07270E00);
        assert_eq!(m.data_type, 1);
        assert_eq!(m.value, XtValue::Double(5.545));
        let mut out = Vec::new();
        m.encode(&mut out);
        assert_eq!(wire, out.as_slice());
    }

    #[test]
    fn value_message_round_trip() {
        let m = ValueMessage {
            topic_id: 7,
            timestamp_micros: 123_456_789,
            data_type: 4,
            value: XtValue::DoubleArray(vec![1.5, -2.5]),
        };
        let mut buf = Vec::new();
        m.encode(&mut buf);
        assert_eq!(ValueMessage::decode(&buf).unwrap(), m);
    }

    #[test]
    fn value_message_rejects_oversized_topic_id() {
        // topic_id = 2^32 + 5 must not silently truncate to 5.
        let wire = hex_bytes("94d300000001000000050001cb3ff0000000000000");
        assert!(ValueMessage::decode(&wire).is_err());
    }

    #[test]
    fn value_message_rejects_non_array() {
        let mut buf = Vec::new();
        crate::websocket::msgpack::encode_value(&XtValue::Double(1.0), &mut buf).unwrap();
        assert!(ValueMessage::decode(&buf).is_err());
    }

    #[test]
    fn ct_message_json_round_trip() {
        let messages = vec![
            CtMessage::Announce {
                name: "x".into(),
                id: 1,
                data_type: "double".into(),
                properties: Default::default(),
                pubuid: None,
            },
            CtMessage::Unannounce {
                name: "x".into(),
                id: 1,
            },
            CtMessage::PropertiesUpdate {
                name: "x".into(),
                update: Default::default(),
                ack: None,
            },
            CtMessage::Publish {
                name: "x".into(),
                pubuid: 2,
                data_type: "double".into(),
                properties: Default::default(),
            },
            CtMessage::Unpublish { pubuid: 2 },
            CtMessage::Subscribe {
                topics: vec!["x".into()],
                subuid: 3,
                options: Default::default(),
            },
            CtMessage::Unsubscribe { subuid: 3 },
            CtMessage::ControlValue {
                topic_id: 1,
                value: serde_json::json!(5),
            },
            CtMessage::Timestamp {
                timestamp: 123,
                value: serde_json::json!(1.5),
            },
            CtMessage::KeepAlive,
        ];
        for m in messages {
            let json = m.to_json();
            assert_eq!(
                CtMessage::from_json(&json).unwrap(),
                m,
                "round trip of {json}"
            );
        }
    }

    #[test]
    fn ct_message_parses_timestamp_and_keepalive() {
        let ts = CtMessage::from_json(
            r#"{"method":"timestamp","params":{"timestamp":123,"value":1.5}}"#,
        )
        .unwrap();
        assert_eq!(
            ts,
            CtMessage::Timestamp {
                timestamp: 123,
                value: serde_json::json!(1.5)
            }
        );
        let ka = CtMessage::from_json(r#"{"method":"keepalive","params":{}}"#).unwrap();
        assert_eq!(ka, CtMessage::KeepAlive);
    }

    #[test]
    fn ct_message_rejects_unknown_method() {
        assert!(CtMessage::from_json(r#"{"method":"bogus","params":{}}"#).is_err());
    }
}
