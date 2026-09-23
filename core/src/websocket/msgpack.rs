//! A minimal MessagePack codec: the subset NT4 uses.

use serde_json::{Map, Value as Json};

use crate::value::Value;

/// How deep arrays may nest, keeping the recursive decoder off the end of the
/// stack.
const MAX_DEPTH: usize = 16;

/// An error from encoding or decoding a MessagePack value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The input ended before the value was complete.
    #[error("the input ended before the value did")]
    UnexpectedEof,
    /// The input had bytes left over after the value.
    #[error("bytes were left over after the value")]
    TrailingBytes,
    /// A valid MessagePack marker this codec does not support.
    #[error("MessagePack marker {0:#04x} is not supported")]
    Unsupported(u8),
    /// An array mixed element kinds that cannot form a typed NT4 list.
    #[error("an array mixes element types")]
    MixedArray,
    /// The value was not an array.
    #[error("expected an array")]
    NotAnArray,
    /// The value nested arrays more than 16 deep.
    #[error("arrays nest deeper than {MAX_DEPTH}")]
    TooDeep,
    /// The array had a different length than expected.
    #[error("expected an array of {expected}, got {got}")]
    WrongArrayLen {
        /// The length the caller needed.
        expected: usize,
        /// The length that arrived.
        got: usize,
    },
    /// The value was not an integer.
    #[error("expected an integer")]
    NotAnInteger,
    /// A length or value, named in the field, did not fit the wire format.
    #[error("{0} does not fit the wire format")]
    OutOfRange(&'static str),
    /// A string's bytes were not UTF-8.
    #[error("a string is not valid UTF-8")]
    InvalidUtf8,
}

/// Encodes `v` into `buf` as MessagePack.
pub fn encode_value(v: &Value, buf: &mut Vec<u8>) -> Result<(), Error> {
    match v {
        Value::Int8(x) => encode_i64(*x as i64, buf),
        Value::Int16(x) => encode_i64(*x as i64, buf),
        Value::Int32(x) => encode_i64(*x as i64, buf),
        Value::Int64(x) => encode_i64(*x, buf),
        Value::Uint8(x) => encode_uint(*x as u64, buf),
        Value::Uint16(x) => encode_uint(*x as u64, buf),
        Value::Uint32(x) => encode_uint(*x as u64, buf),
        Value::Uint64(x) => encode_uint(*x, buf),
        Value::Float(x) => {
            buf.push(0xca);
            buf.extend_from_slice(&x.to_bits().to_be_bytes());
            Ok(())
        }
        Value::Double(x) => {
            buf.push(0xcb);
            buf.extend_from_slice(&x.to_bits().to_be_bytes());
            Ok(())
        }
        Value::String(s) => encode_str(s, buf),
        Value::Bool(b) => {
            buf.push(if *b { 0xc3 } else { 0xc2 });
            Ok(())
        }
        Value::Bytes(b) => encode_bin(b, buf),
        Value::Int8Array(xs) => encode_typed_array(xs, buf, |x| Value::Int8(*x)),
        Value::Int16Array(xs) => encode_typed_array(xs, buf, |x| Value::Int16(*x)),
        Value::Int32Array(xs) => encode_typed_array(xs, buf, |x| Value::Int32(*x)),
        Value::Int64Array(xs) => encode_typed_array(xs, buf, |x| Value::Int64(*x)),
        Value::Uint8Array(xs) => encode_typed_array(xs, buf, |x| Value::Uint8(*x)),
        Value::Uint16Array(xs) => encode_typed_array(xs, buf, |x| Value::Uint16(*x)),
        Value::Uint32Array(xs) => encode_typed_array(xs, buf, |x| Value::Uint32(*x)),
        Value::Uint64Array(xs) => encode_typed_array(xs, buf, |x| Value::Uint64(*x)),
        Value::FloatArray(xs) => encode_typed_array(xs, buf, |x| Value::Float(*x)),
        Value::DoubleArray(xs) => encode_typed_array(xs, buf, |x| Value::Double(*x)),
        Value::StringArray(xs) => encode_typed_array(xs, buf, |x| Value::String(x.clone())),
        Value::BoolArray(xs) => encode_typed_array(xs, buf, |x| Value::Bool(*x)),
        Value::BytesList(b) | Value::Coordinate(b) | Value::Bezier(b) => encode_bin(b, buf),
    }
}

/// Decodes one MessagePack value from `buf`, requiring the whole input be used.
pub fn decode_value(buf: &[u8]) -> Result<Value, Error> {
    let (value, consumed) = decode_one(buf, 0)?;
    if consumed != buf.len() {
        return Err(Error::TrailingBytes);
    }
    Ok(value)
}

/// Decodes an array header and its elements, returning the bytes consumed.
/// Preallocation never exceeds the remaining input.
pub(crate) fn decode_array(buf: &[u8]) -> Result<(Vec<Value>, usize), Error> {
    decode_array_at(buf, 0)
}

fn decode_array_at(buf: &[u8], depth: usize) -> Result<(Vec<Value>, usize), Error> {
    if depth >= MAX_DEPTH {
        return Err(Error::TooDeep);
    }
    let (&marker, rest) = buf.split_first().ok_or(Error::UnexpectedEof)?;
    let (len, rest) = match marker {
        0x90..=0x9f => ((marker & 0x0f) as usize, rest),
        0xdc => {
            let (bytes, rest) = take::<2>(rest)?;
            (u16::from_be_bytes(bytes) as usize, rest)
        }
        0xdd => {
            let (bytes, rest) = take::<4>(rest)?;
            (u32::from_be_bytes(bytes) as usize, rest)
        }
        _ => return Err(Error::NotAnArray),
    };
    let cap = len.min(rest.len());
    let mut items = Vec::with_capacity(cap);
    let mut rest = rest;
    for _ in 0..len {
        let (item, consumed) = decode_one(rest, depth + 1)?;
        items.push(item);
        rest = &rest[consumed..];
    }
    let consumed = buf.len() - rest.len();
    Ok((items, consumed))
}

/// Writes a MessagePack array header for `len` elements.
pub(crate) fn encode_array_header(len: usize, buf: &mut Vec<u8>) -> Result<(), Error> {
    if len <= 0x0f {
        buf.push(0x90 | len as u8);
    } else if len <= 0xffff {
        buf.push(0xdc);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        buf.push(0xdd);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    } else {
        return Err(Error::OutOfRange("array length"));
    }
    Ok(())
}

/// Encodes a `u64` as the smallest signed int that holds it, as NT4's Java
/// `long`s expect. Only values past `i64::MAX` use uint64.
pub(crate) fn encode_uint(x: u64, buf: &mut Vec<u8>) -> Result<(), Error> {
    if x <= i64::MAX as u64 {
        encode_i64(x as i64, buf)
    } else {
        buf.push(0xcf);
        buf.extend_from_slice(&x.to_be_bytes());
        Ok(())
    }
}

/// Encodes an `i64` as the smallest signed MessagePack int that holds it.
pub(crate) fn encode_int(x: i64, buf: &mut Vec<u8>) -> Result<(), Error> {
    encode_i64(x, buf)
}

fn encode_i64(x: i64, buf: &mut Vec<u8>) -> Result<(), Error> {
    if (0..=0x7f).contains(&x) || (-32..=-1).contains(&x) {
        buf.push(x as u8);
    } else if (-128..=127).contains(&x) {
        buf.push(0xd0);
        buf.push(x as u8);
    } else if (-32768..=32767).contains(&x) {
        buf.push(0xd1);
        buf.extend_from_slice(&(x as i16).to_be_bytes());
    } else if (-2147483648..=2147483647).contains(&x) {
        buf.push(0xd2);
        buf.extend_from_slice(&(x as i32).to_be_bytes());
    } else {
        buf.push(0xd3);
        buf.extend_from_slice(&x.to_be_bytes());
    }
    Ok(())
}

fn encode_str(s: &str, buf: &mut Vec<u8>) -> Result<(), Error> {
    let len = s.len();
    if len <= 0x1f {
        buf.push(0xa0 | len as u8);
    } else if len <= 0xff {
        buf.push(0xd9);
        buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0xda);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        buf.push(0xdb);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    } else {
        return Err(Error::OutOfRange("string length"));
    }
    buf.extend_from_slice(s.as_bytes());
    Ok(())
}

fn encode_bin(b: &[u8], buf: &mut Vec<u8>) -> Result<(), Error> {
    let len = b.len();
    if len <= 0xff {
        buf.push(0xc4);
        buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0xc5);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        buf.push(0xc6);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    } else {
        return Err(Error::OutOfRange("bin length"));
    }
    buf.extend_from_slice(b);
    Ok(())
}

fn encode_typed_array<T>(
    xs: &[T],
    buf: &mut Vec<u8>,
    f: impl Fn(&T) -> Value,
) -> Result<(), Error> {
    encode_array_header(xs.len(), buf)?;
    for x in xs {
        encode_value(&f(x), buf)?;
    }
    Ok(())
}

/// Encodes an NT4 meta-topic payload, an array of maps with string keys, as
/// MessagePack bytes.
pub(crate) fn encode_meta_payload(maps: &[Map<String, Json>]) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    encode_array_header(maps.len(), &mut buf)?;
    for map in maps {
        encode_meta_map(map, &mut buf)?;
    }
    Ok(buf)
}

fn encode_meta_map(map: &Map<String, Json>, buf: &mut Vec<u8>) -> Result<(), Error> {
    let len = map.len();
    if len <= 15 {
        buf.push(0x80 | len as u8);
    } else if len <= 0xffff {
        buf.push(0xde);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        buf.push(0xdf);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    }
    for (key, value) in map {
        encode_str(key, buf)?;
        encode_meta_value(value, buf)?;
    }
    Ok(())
}

fn encode_meta_value(v: &Json, buf: &mut Vec<u8>) -> Result<(), Error> {
    match v {
        Json::Null => buf.push(0xc0),
        Json::Bool(b) => {
            buf.push(if *b { 0xc3 } else { 0xc2 });
        }
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                encode_i64(i, buf)?;
            } else {
                let f = n.as_f64().unwrap_or_default();
                buf.push(0xcb);
                buf.extend_from_slice(&f.to_bits().to_be_bytes());
            }
        }
        Json::String(s) => encode_str(s, buf)?,
        Json::Array(arr) => {
            encode_array_header(arr.len(), buf)?;
            for item in arr {
                encode_meta_value(item, buf)?;
            }
        }
        Json::Object(map) => encode_meta_map(map, buf)?,
    }
    Ok(())
}

fn decode_one(buf: &[u8], depth: usize) -> Result<(Value, usize), Error> {
    let (&marker, rest) = buf.split_first().ok_or(Error::UnexpectedEof)?;
    match marker {
        0x00..=0x7f => Ok((Value::Uint8(marker), 1)),
        0xe0..=0xff => Ok((Value::Int8(marker as i8), 1)),
        0xc0 => Err(Error::Unsupported(0xc0)),
        0xc2 => Ok((Value::Bool(false), 1)),
        0xc3 => Ok((Value::Bool(true), 1)),
        0xca => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((Value::Float(f32::from_bits(u32::from_be_bytes(bytes))), 5))
        }
        0xcb => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((Value::Double(f64::from_bits(u64::from_be_bytes(bytes))), 9))
        }
        0xcc => {
            let (&b, _) = rest.split_first().ok_or(Error::UnexpectedEof)?;
            Ok((Value::Uint8(b), 2))
        }
        0xcd => {
            let (bytes, _) = take::<2>(rest)?;
            Ok((Value::Uint16(u16::from_be_bytes(bytes)), 3))
        }
        0xce => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((Value::Uint32(u32::from_be_bytes(bytes)), 5))
        }
        0xcf => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((Value::Uint64(u64::from_be_bytes(bytes)), 9))
        }
        0xd0 => {
            let (&b, _) = rest.split_first().ok_or(Error::UnexpectedEof)?;
            Ok((Value::Int8(b as i8), 2))
        }
        0xd1 => {
            let (bytes, _) = take::<2>(rest)?;
            Ok((Value::Int16(i16::from_be_bytes(bytes)), 3))
        }
        0xd2 => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((Value::Int32(i32::from_be_bytes(bytes)), 5))
        }
        0xd3 => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((Value::Int64(i64::from_be_bytes(bytes)), 9))
        }
        0xa0..=0xbf => {
            let len = (marker & 0x1f) as usize;
            let bytes = rest.get(..len).ok_or(Error::UnexpectedEof)?;
            let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
            Ok((Value::String(s.to_string()), 1 + len))
        }
        0xd9 => {
            let (&len, rest) = rest.split_first().ok_or(Error::UnexpectedEof)?;
            let bytes = rest.get(..len as usize).ok_or(Error::UnexpectedEof)?;
            let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
            Ok((Value::String(s.to_string()), 2 + len as usize))
        }
        0xda => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or(Error::UnexpectedEof)?;
            let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
            Ok((Value::String(s.to_string()), 3 + len))
        }
        0xdb => {
            let (bytes, rest) = take::<4>(rest)?;
            let len = u32::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or(Error::UnexpectedEof)?;
            let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
            Ok((Value::String(s.to_string()), 5 + len))
        }
        0xc4 => {
            let (&len, rest) = rest.split_first().ok_or(Error::UnexpectedEof)?;
            let bytes = rest.get(..len as usize).ok_or(Error::UnexpectedEof)?;
            Ok((Value::Bytes(bytes.to_vec()), 2 + len as usize))
        }
        0xc5 => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or(Error::UnexpectedEof)?;
            Ok((Value::Bytes(bytes.to_vec()), 3 + len))
        }
        0xc6 => {
            let (bytes, rest) = take::<4>(rest)?;
            let len = u32::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or(Error::UnexpectedEof)?;
            Ok((Value::Bytes(bytes.to_vec()), 5 + len))
        }
        0x90..=0x9f | 0xdc | 0xdd => {
            let (items, consumed) = decode_array_at(buf, depth)?;
            let value = classify_array(items)?;
            Ok((value, consumed))
        }
        _ => Err(Error::Unsupported(marker)),
    }
}

/// Turns decoded elements into the typed NT4 list they form. An empty array
/// becomes a double array.
fn classify_array(items: Vec<Value>) -> Result<Value, Error> {
    if items.is_empty() {
        return Ok(Value::DoubleArray(Vec::new()));
    }
    fn every<T>(items: &[Value], pick: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
        items.iter().map(pick).collect()
    }
    let any_big = items
        .iter()
        .any(|v| v.as_u64().is_some_and(|x| x > i64::MAX as u64));
    if any_big
        && let Some(xs) = every(&items, |v| {
            v.as_u64().or_else(|| v.as_i64().map(|x| x as u64))
        })
    {
        Ok(Value::Uint64Array(xs))
    } else if let Some(xs) = every(&items, |v| {
        v.as_i64().or_else(|| v.as_u64().map(|x| x as i64))
    }) {
        Ok(Value::Int64Array(xs))
    } else if items.iter().all(|v| matches!(v, Value::Float(_)))
        && let Some(xs) = every(&items, |v| v.as_f64().map(|x| x as f32))
    {
        Ok(Value::FloatArray(xs))
    } else if items.iter().all(|v| matches!(v, Value::Double(_)))
        && let Some(xs) = every(&items, Value::as_f64)
    {
        Ok(Value::DoubleArray(xs))
    } else if let Some(xs) = every(&items, |v| v.as_string().map(str::to_string)) {
        Ok(Value::StringArray(xs))
    } else if let Some(xs) = every(&items, Value::as_bool) {
        Ok(Value::BoolArray(xs))
    } else {
        Err(Error::MixedArray)
    }
}

fn take<const N: usize>(buf: &[u8]) -> Result<([u8; N], &[u8]), Error> {
    let bytes = buf.get(..N).ok_or(Error::UnexpectedEof)?;
    let mut arr = [0u8; N];
    arr.copy_from_slice(bytes);
    Ok((arr, &buf[N..]))
}

#[cfg(test)]
mod tests {
    use crate::value::Value;
    use crate::websocket::msgpack::{Error, decode_array, decode_value, encode_value};

    #[test]
    fn double_round_trip() {
        let v = Value::Double(4.344505251111111);
        let mut buf = Vec::new();
        encode_value(&v, &mut buf).unwrap();
        assert_eq!(
            buf,
            vec![0xcb, 0x40, 0x11, 0x60, 0xc5, 0xfc, 0x0b, 0x4a, 0x3b]
        );
        assert_eq!(decode_value(&buf).unwrap(), v);
    }

    #[test]
    fn scalar_encode_decode_encode_identity() {
        let values = vec![
            Value::Int64(5),
            Value::Uint64(5),
            Value::Float(1.5),
            Value::Double(2.5),
            Value::String("hello".to_string()),
            Value::Bool(true),
            Value::Bytes(vec![1, 2, 3]),
            Value::Int64Array(vec![1, 2, 3]),
            Value::StringArray(vec!["a".to_string(), "b".to_string()]),
        ];
        for v in values {
            let mut buf = Vec::new();
            encode_value(&v, &mut buf).unwrap();
            let decoded = decode_value(&buf).unwrap();
            let mut again = Vec::new();
            encode_value(&decoded, &mut again).unwrap();
            assert_eq!(again, buf, "re-encode of {v:?} drifted");
        }
    }

    /// A raw value past 64 KiB needs the 32-bit forms. A 16-bit length cannot
    /// hold it, and a camera frame or a long struct array is that size.
    #[test]
    fn values_past_64_kib_round_trip_through_the_32_bit_forms() {
        for v in [
            Value::Bytes(vec![7; 70_000]),
            Value::String("x".repeat(70_000)),
        ] {
            let mut buf = Vec::new();
            encode_value(&v, &mut buf).unwrap();
            assert!(
                matches!(buf[0], 0xc6 | 0xdb),
                "a value this size must use bin32 or str32"
            );
            assert_eq!(decode_value(&buf).unwrap(), v);
        }
    }

    #[test]
    fn typed_list_round_trip() {
        let v = Value::DoubleArray(vec![1.5, 2.5, -3.25]);
        let mut buf = Vec::new();
        encode_value(&v, &mut buf).unwrap();
        assert_eq!(decode_value(&buf).unwrap(), v);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        let mut buf = Vec::new();
        encode_value(&Value::Double(1.0), &mut buf).unwrap();
        assert_eq!(
            decode_value(&buf[..buf.len() - 1]),
            Err(Error::UnexpectedEof)
        );
    }

    #[test]
    fn decode_rejects_nil() {
        assert_eq!(decode_value(&[0xc0]), Err(Error::Unsupported(0xc0)));
    }

    /// Every byte of the input opens another array, so its length is the
    /// nesting depth. Without a limit the decoder recurses until the stack is
    /// gone, which aborts the process rather than closing the connection.
    #[test]
    fn decode_rejects_arrays_nested_past_the_depth_limit() {
        let deep = vec![0x91u8; 1024 * 1024];
        assert_eq!(decode_value(&deep), Err(Error::TooDeep));
    }

    #[test]
    fn decode_still_accepts_an_array_of_scalars() {
        let v = Value::Int64Array(vec![1, 2, 3]);
        let mut buf = Vec::new();
        encode_value(&v, &mut buf).unwrap();
        assert_eq!(decode_value(&buf).unwrap(), v);
    }

    /// An array32 header claiming 2^32-1 elements with no payload must error
    /// rather than attempt a 128 GiB preallocation.
    #[test]
    fn decode_array_rejects_hostile_length() {
        assert_eq!(
            decode_array(&[0xdd, 0xff, 0xff, 0xff, 0xff]),
            Err(Error::UnexpectedEof)
        );
    }
}
