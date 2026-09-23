//! NT4 messages: [`ControlMessage`] as JSON, [`ValueMessage`] as MessagePack.

use serde_json::{Map, Value as Json};

use crate::value::Value;
use crate::websocket::msgpack;

/// An error from parsing a [`ControlMessage`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ControlMessageError {
    /// The frame was not JSON.
    #[error("the frame is not valid JSON")]
    Json(#[from] serde_json::Error),
    /// The JSON document was not an object.
    #[error("the control message is not a JSON object")]
    NotAnObject,
    /// A required parameter, named in the field, was absent.
    #[error("the parameter `{0}` is missing")]
    Missing(String),
    /// A parameter, named in the field, had the wrong type.
    #[error("the parameter `{0}` has the wrong type")]
    WrongType(String),
    /// The `method` value is not a known control message.
    #[error("`{0}` is not a control message method")]
    UnknownMethod(String),
    /// A frame held some other number of messages where one was expected.
    #[error("expected one control message, got {0}")]
    NotOne(usize),
}

/// An NT4 control message, carried as JSON.
///
/// `ControlValue`, `Timestamp` and `KeepAlive` are this crate's own JSON
/// methods, outside NT4.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlMessage {
    /// Topic announcement (server to client).
    Announce {
        /// Topic name.
        name: String,
        /// Topic ID used in MessagePack messages.
        id: u32,
        /// Data type as a string (e.g. `"double"`).
        data_type: String,
        /// Topic properties.
        properties: Map<String, Json>,
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
    SetProperties {
        /// Topic name.
        name: String,
        /// Properties to set. A null value removes the property.
        update: Map<String, Json>,
    },
    /// Topic properties changed (server to client).
    PropertiesUpdate {
        /// Topic name.
        name: String,
        /// Properties to update.
        update: Map<String, Json>,
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
        properties: Map<String, Json>,
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
        options: Map<String, Json>,
    },
    /// Unsubscribe request (client to server).
    Unsubscribe {
        /// Subscription UID from the matching `subscribe`.
        subuid: u32,
    },
    /// A control value for a topic (`"controlvalue"`, this crate's own).
    ControlValue {
        /// Topic ID.
        topic_id: u32,
        /// The value.
        value: Json,
    },
    /// A timestamp exchange (`"timestamp"`, this crate's own).
    Timestamp {
        /// Timestamp in microseconds.
        timestamp: u64,
        /// The value.
        value: Json,
    },
    /// A keepalive (`"keepalive"`, this crate's own).
    KeepAlive,
}

impl ControlMessage {
    /// Parses one control message, bare or as a one-element NT4 array.
    ///
    /// # Errors
    ///
    /// Returns [`ControlMessageError`] when the JSON is malformed, holds some
    /// other number of messages, or names an unknown method.
    pub fn from_json(json: &str) -> Result<Self, ControlMessageError> {
        let mut batch = Self::from_json_batch(json)?;
        if batch.len() != 1 {
            return Err(ControlMessageError::NotOne(batch.len()));
        }
        Ok(batch.remove(0))
    }

    /// Parses every control message in one NT4 text frame, skipping messages
    /// that do not parse.
    ///
    /// # Errors
    ///
    /// Returns [`ControlMessageError::Json`] when the frame is not JSON.
    pub fn from_json_batch(json: &str) -> Result<Vec<Self>, ControlMessageError> {
        let root: Json = serde_json::from_str(json)?;
        match root {
            Json::Array(items) => Ok(items
                .iter()
                .filter_map(|i| Self::from_value(i).ok())
                .collect()),
            other => Ok(Self::from_value(&other).ok().into_iter().collect()),
        }
    }

    fn from_value(root: &Json) -> Result<Self, ControlMessageError> {
        let obj = root.as_object().ok_or(ControlMessageError::NotAnObject)?;
        let method = obj
            .get("method")
            .and_then(Json::as_str)
            .ok_or_else(|| ControlMessageError::Missing("method".to_owned()))?;
        let params = obj
            .get("params")
            .and_then(Json::as_object)
            .ok_or_else(|| ControlMessageError::Missing("params".to_owned()))?;
        match method {
            "announce" => Ok(ControlMessage::Announce {
                name: get_string(params, "name")?,
                id: get_u32(params, "id")?,
                data_type: get_string(params, "type")?,
                properties: get_map(params, "properties")?,
                pubuid: get_optional_u32(params, "pubuid")?,
            }),
            "unannounce" => Ok(ControlMessage::Unannounce {
                name: get_string(params, "name")?,
                id: get_u32(params, "id")?,
            }),
            "properties" => Ok(ControlMessage::PropertiesUpdate {
                name: get_string(params, "name")?,
                update: get_map(params, "update")?,
                ack: get_optional_bool(params, "ack")?,
            }),
            "setproperties" => Ok(ControlMessage::SetProperties {
                name: get_string(params, "name")?,
                update: get_map(params, "update")?,
            }),
            "publish" => Ok(ControlMessage::Publish {
                name: get_string(params, "name")?,
                pubuid: get_u32(params, "pubuid")?,
                data_type: get_string(params, "type")?,
                properties: get_map(params, "properties")?,
            }),
            "unpublish" => Ok(ControlMessage::Unpublish {
                pubuid: get_u32(params, "pubuid")?,
            }),
            "subscribe" => Ok(ControlMessage::Subscribe {
                topics: get_string_array(params, "topics")?,
                subuid: get_u32(params, "subuid")?,
                options: get_map(params, "options")?,
            }),
            "unsubscribe" => Ok(ControlMessage::Unsubscribe {
                subuid: get_u32(params, "subuid")?,
            }),
            "controlvalue" => Ok(ControlMessage::ControlValue {
                topic_id: get_u32(params, "topic_id")?,
                value: params
                    .get("value")
                    .cloned()
                    .ok_or_else(|| ControlMessageError::Missing("value".to_owned()))?,
            }),
            "timestamp" => Ok(ControlMessage::Timestamp {
                timestamp: get_u64(params, "timestamp")?,
                value: params
                    .get("value")
                    .cloned()
                    .ok_or_else(|| ControlMessageError::Missing("value".to_owned()))?,
            }),
            "keepalive" => Ok(ControlMessage::KeepAlive),
            other => Err(ControlMessageError::UnknownMethod(other.to_owned())),
        }
    }

    /// Serializes the message as an NT4 text frame: a one-element JSON array.
    pub fn to_json(&self) -> String {
        let mut params = Map::new();
        let method = match self {
            ControlMessage::Announce {
                name,
                id,
                data_type,
                properties,
                pubuid,
            } => {
                params.insert("name".into(), Json::String(name.clone()));
                params.insert("id".into(), Json::from(*id));
                params.insert("type".into(), Json::String(data_type.clone()));
                params.insert("properties".into(), Json::Object(properties.clone()));
                if let Some(pubuid) = pubuid {
                    params.insert("pubuid".into(), Json::from(*pubuid));
                }
                "announce"
            }
            ControlMessage::Unannounce { name, id } => {
                params.insert("name".into(), Json::String(name.clone()));
                params.insert("id".into(), Json::from(*id));
                "unannounce"
            }
            ControlMessage::PropertiesUpdate { name, update, ack } => {
                params.insert("name".into(), Json::String(name.clone()));
                params.insert("update".into(), Json::Object(update.clone()));
                if let Some(ack) = ack {
                    params.insert("ack".into(), Json::from(*ack));
                }
                "properties"
            }
            ControlMessage::Publish {
                name,
                pubuid,
                data_type,
                properties,
            } => {
                params.insert("name".into(), Json::String(name.clone()));
                params.insert("pubuid".into(), Json::from(*pubuid));
                params.insert("type".into(), Json::String(data_type.clone()));
                params.insert("properties".into(), Json::Object(properties.clone()));
                "publish"
            }
            ControlMessage::Unpublish { pubuid } => {
                params.insert("pubuid".into(), Json::from(*pubuid));
                "unpublish"
            }
            ControlMessage::Subscribe {
                topics,
                subuid,
                options,
            } => {
                params.insert(
                    "topics".into(),
                    Json::Array(topics.iter().cloned().map(Json::String).collect()),
                );
                params.insert("subuid".into(), Json::from(*subuid));
                params.insert("options".into(), Json::Object(options.clone()));
                "subscribe"
            }
            ControlMessage::Unsubscribe { subuid } => {
                params.insert("subuid".into(), Json::from(*subuid));
                "unsubscribe"
            }
            ControlMessage::ControlValue { topic_id, value } => {
                params.insert("topic_id".into(), Json::from(*topic_id));
                params.insert("value".into(), value.clone());
                "controlvalue"
            }
            ControlMessage::Timestamp { timestamp, value } => {
                params.insert("timestamp".into(), Json::from(*timestamp));
                params.insert("value".into(), value.clone());
                "timestamp"
            }
            ControlMessage::SetProperties { name, update } => {
                params.insert("name".into(), Json::String(name.clone()));
                params.insert("update".into(), Json::Object(update.clone()));
                "setproperties"
            }
            ControlMessage::KeepAlive => "keepalive",
        };
        let mut root = Map::new();
        root.insert("method".into(), Json::String(method.into()));
        root.insert("params".into(), Json::Object(params));
        Json::Array(vec![Json::Object(root)]).to_string()
    }
}

/// The reserved topic id for NT4 timestamp messages. The wire carries `-1`,
/// and this crate uses the `u32` sentinel.
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
    pub value: Value,
}

impl ValueMessage {
    /// Encodes the message as a MessagePack fixarray(4), writing
    /// [`RTT_TOPIC_ID`] as `-1`.
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_value_message(
            self.topic_id,
            self.timestamp_micros,
            self.data_type,
            &self.value,
            buf,
        );
    }

    /// Decodes exactly one value message. See
    /// [`ValueMessage::decode_all`] for a batched frame.
    pub fn decode(buf: &[u8]) -> Result<Self, msgpack::Error> {
        let (msg, consumed) = Self::decode_one(buf)?;
        if consumed != buf.len() {
            return Err(msgpack::Error::TrailingBytes);
        }
        Ok(msg)
    }

    /// Decodes every value message in one binary frame.
    ///
    /// # Errors
    ///
    /// Returns the first [`msgpack::Error`] from decoding, or
    /// [`msgpack::Error::UnexpectedEof`] when the frame is empty.
    pub fn decode_all(buf: &[u8]) -> Result<Vec<Self>, msgpack::Error> {
        let mut rest = buf;
        let mut out = Vec::new();
        while !rest.is_empty() {
            let (msg, consumed) = Self::decode_one(rest)?;
            out.push(msg);
            rest = &rest[consumed..];
        }
        if out.is_empty() {
            return Err(msgpack::Error::UnexpectedEof);
        }
        Ok(out)
    }

    fn decode_one(buf: &[u8]) -> Result<(Self, usize), msgpack::Error> {
        let (mut items, consumed) = msgpack::decode_array(buf)?;
        if items.len() != 4 {
            return Err(msgpack::Error::WrongArrayLen {
                expected: 4,
                got: items.len(),
            });
        }
        let value = items.pop().expect("length checked");
        let topic_id = match items[0].as_i64() {
            Some(-1) => RTT_TOPIC_ID,
            _ => u32::try_from(items[0].as_u64_any().ok_or(msgpack::Error::NotAnInteger)?)
                .map_err(|_| msgpack::Error::OutOfRange("topic id"))?,
        };
        Ok((
            ValueMessage {
                topic_id,
                timestamp_micros: items[1].as_u64_any().ok_or(msgpack::Error::NotAnInteger)?,
                data_type: u32::try_from(
                    items[2].as_u64_any().ok_or(msgpack::Error::NotAnInteger)?,
                )
                .map_err(|_| msgpack::Error::OutOfRange("data type"))?,
                value,
            },
            consumed,
        ))
    }
}

/// Encodes one value message from borrowed parts, writing [`RTT_TOPIC_ID`]
/// as `-1`.
///
/// # Panics
///
/// Panics on a string or byte array longer than `u32::MAX` bytes.
pub fn encode_value_message(
    topic_id: u32,
    timestamp_micros: u64,
    data_type: u32,
    value: &Value,
    buf: &mut Vec<u8>,
) {
    msgpack::encode_array_header(4, buf).expect("encoding a 4-element array header is infallible");
    if topic_id == RTT_TOPIC_ID {
        msgpack::encode_int(-1, buf).expect("encoding an i64 is infallible");
    } else {
        msgpack::encode_uint(topic_id as u64, buf).expect("encoding a u64 is infallible");
    }
    msgpack::encode_uint(timestamp_micros, buf).expect("encoding a u64 is infallible");
    msgpack::encode_uint(data_type as u64, buf).expect("encoding a u64 is infallible");
    msgpack::encode_value(value, buf).expect("a value under 4 GiB always encodes");
}

fn get_string(params: &Map<String, Json>, key: &str) -> Result<String, ControlMessageError> {
    match params.get(key) {
        Some(Json::String(s)) => Ok(s.clone()),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
        None => Err(ControlMessageError::Missing(key.to_owned())),
    }
}

fn get_u32(params: &Map<String, Json>, key: &str) -> Result<u32, ControlMessageError> {
    match params.get(key) {
        Some(Json::Number(n)) => n
            .as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .ok_or_else(|| ControlMessageError::WrongType(key.to_owned())),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
        None => Err(ControlMessageError::Missing(key.to_owned())),
    }
}

fn get_u64(params: &Map<String, Json>, key: &str) -> Result<u64, ControlMessageError> {
    match params.get(key) {
        Some(Json::Number(n)) => n
            .as_u64()
            .ok_or_else(|| ControlMessageError::WrongType(key.to_owned())),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
        None => Err(ControlMessageError::Missing(key.to_owned())),
    }
}

fn get_optional_u32(
    params: &Map<String, Json>,
    key: &str,
) -> Result<Option<u32>, ControlMessageError> {
    match params.get(key) {
        None => Ok(None),
        Some(Json::Number(n)) => n
            .as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .map(Some)
            .ok_or_else(|| ControlMessageError::WrongType(key.to_owned())),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
    }
}

fn get_optional_bool(
    params: &Map<String, Json>,
    key: &str,
) -> Result<Option<bool>, ControlMessageError> {
    match params.get(key) {
        None => Ok(None),
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
    }
}

fn get_map(
    params: &Map<String, Json>,
    key: &str,
) -> Result<Map<String, Json>, ControlMessageError> {
    match params.get(key) {
        Some(Json::Object(m)) => Ok(m.clone()),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
        None => Err(ControlMessageError::Missing(key.to_owned())),
    }
}

fn get_string_array(
    params: &Map<String, Json>,
    key: &str,
) -> Result<Vec<String>, ControlMessageError> {
    match params.get(key) {
        Some(Json::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ControlMessageError::WrongType(key.to_owned()))
            })
            .collect(),
        Some(_) => Err(ControlMessageError::WrongType(key.to_owned())),
        None => Err(ControlMessageError::Missing(key.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use crate::value::Value;
    use crate::websocket::message::{ControlMessage, ValueMessage};

    fn hex_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn golden_vector_nt4() {
        let wire = hex_bytes("9432d207270e0001cb40162e147ae147ae");
        let m = ValueMessage::decode(&wire).unwrap();
        assert_eq!(m.topic_id, 50);
        assert_eq!(m.timestamp_micros, 0x07270E00);
        assert_eq!(m.data_type, 1);
        assert_eq!(m.value, Value::Double(5.545));
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
            value: Value::DoubleArray(vec![1.5, -2.5]),
        };
        let mut buf = Vec::new();
        m.encode(&mut buf);
        assert_eq!(ValueMessage::decode(&buf).unwrap(), m);
    }

    #[test]
    fn value_message_rejects_oversized_topic_id() {
        let wire = hex_bytes("94d300000001000000050001cb3ff0000000000000");
        assert!(ValueMessage::decode(&wire).is_err());
    }

    #[test]
    fn value_message_rejects_non_array() {
        let mut buf = Vec::new();
        crate::websocket::msgpack::encode_value(&Value::Double(1.0), &mut buf).unwrap();
        assert!(ValueMessage::decode(&buf).is_err());
    }

    #[test]
    fn ct_message_json_round_trip() {
        let messages = vec![
            ControlMessage::Announce {
                name: "x".into(),
                id: 1,
                data_type: "double".into(),
                properties: Default::default(),
                pubuid: None,
            },
            ControlMessage::Unannounce {
                name: "x".into(),
                id: 1,
            },
            ControlMessage::PropertiesUpdate {
                name: "x".into(),
                update: Default::default(),
                ack: None,
            },
            ControlMessage::Publish {
                name: "x".into(),
                pubuid: 2,
                data_type: "double".into(),
                properties: Default::default(),
            },
            ControlMessage::Unpublish { pubuid: 2 },
            ControlMessage::Subscribe {
                topics: vec!["x".into()],
                subuid: 3,
                options: Default::default(),
            },
            ControlMessage::Unsubscribe { subuid: 3 },
            ControlMessage::ControlValue {
                topic_id: 1,
                value: serde_json::json!(5),
            },
            ControlMessage::Timestamp {
                timestamp: 123,
                value: serde_json::json!(1.5),
            },
            ControlMessage::KeepAlive,
        ];
        for m in messages {
            let json = m.to_json();
            assert_eq!(
                ControlMessage::from_json(&json).unwrap(),
                m,
                "round trip of {json}"
            );
        }
    }

    #[test]
    fn ct_message_parses_timestamp_and_keepalive() {
        let ts = ControlMessage::from_json(
            r#"{"method":"timestamp","params":{"timestamp":123,"value":1.5}}"#,
        )
        .unwrap();
        assert_eq!(
            ts,
            ControlMessage::Timestamp {
                timestamp: 123,
                value: serde_json::json!(1.5)
            }
        );
        let ka = ControlMessage::from_json(r#"{"method":"keepalive","params":{}}"#).unwrap();
        assert_eq!(ka, ControlMessage::KeepAlive);
    }

    #[test]
    fn ct_message_rejects_unknown_method() {
        assert!(ControlMessage::from_json(r#"{"method":"bogus","params":{}}"#).is_err());
    }
}
