//! The values a simulated topic publishes.

use crate::harness;
use crate::sim::profile::{PublishSpec, has_header};
use tarwyn_server::Value;

/// How a type string's value is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    Binary,
    Text,
    Bool,
    Double,
    Int,
    Float,
    BoolArray,
    DoubleArray,
    IntArray,
    FloatArray,
    StringArray,
}

/// The body of an NT4 type string. Anything outside the NT4 table is binary,
/// as NT4 carries it.
pub fn body(type_str: &str) -> Body {
    match type_str {
        "boolean" => Body::Bool,
        "double" => Body::Double,
        "int" => Body::Int,
        "float" => Body::Float,
        "string" | "json" => Body::Text,
        "boolean[]" => Body::BoolArray,
        "double[]" => Body::DoubleArray,
        "int[]" => Body::IntArray,
        "float[]" => Body::FloatArray,
        "string[]" => Body::StringArray,
        _ => Body::Binary,
    }
}

/// The smallest captured size that holds the header, which the capture
/// counted with 3 bytes of msgpack framing. `None` for a body with no room.
pub fn header_bytes(body: Body) -> Option<usize> {
    match body {
        Body::Binary => Some(crate::harness::HEADER_LEN + 3),
        Body::Text => Some(2 * crate::harness::HEADER_LEN + 3),
        _ => None,
    }
}

/// Bytes of msgpack framing the capture counted around a value.
const FRAMING: usize = 3;

/// The captured size of the `seq`th publish, cycling through the range.
pub fn size_for(spec: &PublishSpec, seq: u64) -> usize {
    let [low, high] = spec.bytes;
    low + (seq % (high - low + 1) as u64) as usize
}

/// The header as text: `seq` then the due time, 16 lowercase hex digits each.
pub fn header_text(seq: u64, due_ns: u64) -> String {
    format!("{seq:016x}{due_ns:016x}")
}

/// The `seq`th value of a topic, due at `due_ns`. It differs from the one
/// before, since ntcore drops a repeated value, and carries the header when
/// the topic has room for it.
pub fn make(spec: &PublishSpec, seq: u64, due_ns: u64) -> Value {
    let size = size_for(spec, seq);
    let len = size.saturating_sub(FRAMING);
    let header = has_header(&spec.type_str, size);
    let count = |each: usize| (len / each).max(1);
    match body(&spec.type_str) {
        Body::Binary => {
            let mut bytes = vec![0u8; len];
            if header {
                harness::encode(&mut bytes, seq, due_ns);
            } else if let Some(first) = bytes.first_mut() {
                *first = seq as u8;
            }
            Value::Bytes(bytes)
        }
        Body::Text => {
            let mut text = if header {
                header_text(seq, due_ns)
            } else {
                format!("{seq:x}")
            };
            while text.len() < len {
                text.push('x');
            }
            Value::String(text)
        }
        Body::Bool => Value::Bool(seq % 2 == 1),
        Body::Double => Value::Double(seq as f64 * 0.001),
        Body::Int => Value::Int64(seq as i64),
        Body::Float => Value::Float(seq as f32),
        Body::BoolArray => {
            Value::BoolArray((0..count(1)).map(|i| (seq + i as u64) % 2 == 1).collect())
        }
        Body::DoubleArray => {
            Value::DoubleArray((0..count(9)).map(|i| (seq + i as u64) as f64).collect())
        }
        Body::IntArray => {
            Value::Int64Array((0..count(9)).map(|i| (seq + i as u64) as i64).collect())
        }
        Body::FloatArray => {
            Value::FloatArray((0..count(5)).map(|i| (seq + i as u64) as f32).collect())
        }
        Body::StringArray => {
            let mut text = format!("{seq:016x}");
            while text.len() + FRAMING < len {
                text.push('x');
            }
            Value::StringArray(vec![text])
        }
    }
}

/// The sequence number and due time a value carries, if it carries them.
pub fn read_header(value: &Value) -> Option<(u64, u64)> {
    match value {
        Value::Bytes(bytes) => harness::decode(bytes),
        Value::String(text) => {
            let digits = text.get(..2 * harness::HEADER_LEN)?;
            let seq = u64::from_str_radix(&digits[..16], 16).ok()?;
            let due = u64::from_str_radix(&digits[16..], 16).ok()?;
            Some((seq, due))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::profile::Class;

    fn spec(type_str: &str, bytes: [usize; 2], class: Class) -> PublishSpec {
        PublishSpec {
            topic: "/t".into(),
            type_str: type_str.into(),
            rate_hz: 25.0,
            bytes,
            class,
            properties: Default::default(),
        }
    }

    #[test]
    fn a_binary_value_carries_the_header_and_its_size() {
        let spec = spec("struct:PoseObservation[]", [451, 451], Class::Struct);
        let value = make(&spec, 7, 1_000);
        let Value::Bytes(bytes) = &value else {
            panic!("{value:?}")
        };
        assert_eq!(
            bytes.len(),
            448,
            "the capture counted 3 bytes of msgpack framing"
        );
        assert_eq!(read_header(&value), Some((7, 1_000)));
    }

    #[test]
    fn a_string_carries_the_header_as_hex() {
        let value = make(&spec("string", [40, 40], Class::Slow), 3, 99);
        assert_eq!(read_header(&value), Some((3, 99)));
    }

    #[test]
    fn a_topic_too_small_for_a_header_is_load_only() {
        let value = make(&spec("structschema", [15, 15], Class::Scalar), 1, 2);
        let Value::Bytes(bytes) = &value else {
            panic!("{value:?}")
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(read_header(&value), None);
    }

    #[test]
    fn every_publish_changes_the_value() {
        for type_str in ["double", "int", "float", "boolean", "double[]", "string[]"] {
            let spec = spec(type_str, [20, 20], Class::Scalar);
            assert_ne!(make(&spec, 1, 0), make(&spec, 2, 0), "{type_str}");
        }
    }

    #[test]
    fn sizes_cycle_through_the_range() {
        let spec = spec("raw", [30, 32], Class::Camera);
        let sizes: Vec<usize> = (0..4).map(|seq| size_for(&spec, seq)).collect();
        assert_eq!(sizes, [30, 31, 32, 30]);
    }
}
