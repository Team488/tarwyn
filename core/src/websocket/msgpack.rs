//! A minimal hand-rolled MessagePack codec for NT4 values.
//!
//! Follows the original MPack spec bytes
//! (<https://github.com/msgpack/msgpack/blob/master/spec.md>). Only the subset
//! NT4 needs is implemented: ints, floats, str, bin, bool, nil, and arrays.

use std::fmt;

use serde_json::{Map, Value as Json};

use crate::value::Value;

/// How deep an inbound value may nest arrays before it is rejected.
///
/// Decoding recurses, so an unbounded depth is a stack overflow, and a stack
/// overflow aborts the process rather than dropping the connection. NT4 values
/// are one array of scalars, so anything past a couple of levels is malformed
/// either way.
const MAX_DEPTH: usize = 16;

/// An error from encoding or decoding a MessagePack value.
///
/// Carries a human-readable message; no payload is needed beyond that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    /// A generic error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The input ended before the value was complete.
    pub fn unexpected_eof() -> Self {
        Self::new("unexpected end of input")
    }

    /// The input had bytes left over after the value.
    pub fn trailing_bytes() -> Self {
        Self::new("trailing bytes after value")
    }

    /// The byte is a valid MessagePack marker this codec does not support.
    pub fn unsupported(what: impl Into<String>) -> Self {
        Self::new(format!("unsupported MessagePack marker: {}", what.into()))
    }

    /// An array mixed element kinds that cannot form a typed NT4 list.
    pub fn invalid_array(what: impl Into<String>) -> Self {
        Self::new(format!("invalid array: {}", what.into()))
    }

    /// The value was not an array.
    pub fn not_an_array() -> Self {
        Self::new("expected an array")
    }

    /// The value nested arrays deeper than the decoder's depth limit.
    pub fn too_deep() -> Self {
        Self::new("nested too deeply")
    }

    /// The array had a different length than expected.
    pub fn wrong_array_len(expected: usize, got: usize) -> Self {
        Self::new(format!("expected array of length {expected}, got {got}"))
    }

    /// The value was not an integer.
    pub fn not_an_integer() -> Self {
        Self::new("expected an integer")
    }

    /// A length or value did not fit the wire format.
    pub fn out_of_range(what: impl Into<String>) -> Self {
        Self::new(format!("value out of range: {}", what.into()))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

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
        return Err(Error::trailing_bytes());
    }
    Ok(value)
}

/// Decodes a MessagePack array header and its elements.
///
/// Returns the raw elements and the number of bytes consumed, so callers can
/// decode a value message's 4-tuple without classifying the array.
pub(crate) fn decode_array(buf: &[u8]) -> Result<(Vec<Value>, usize), Error> {
    decode_array_at(buf, 0)
}

fn decode_array_at(buf: &[u8], depth: usize) -> Result<(Vec<Value>, usize), Error> {
    if depth >= MAX_DEPTH {
        return Err(Error::too_deep());
    }
    let (&marker, rest) = buf.split_first().ok_or_else(Error::unexpected_eof)?;
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
        _ => return Err(Error::not_an_array()),
    };
    // Cap the preallocation at the remaining input: each element needs at
    // least one byte, so a hostile array32 length cannot force a huge
    // allocation. The loop still decodes exactly `len` elements and errors
    // with `unexpected_eof` when the input runs out.
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
        return Err(Error::out_of_range("array length"));
    }
    Ok(())
}

/// Encodes a `u64` as the smallest signed MessagePack int that holds it.
///
/// NT4 timestamps and ids are Java `long`s on the wire, so values that fit an
/// `i64` use the signed forms (int8/int16/int32/int64); only values above
/// `i64::MAX` fall back to uint64.
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
///
/// Needed for the NT4 RTT topic id of `-1`, which must go out as a negative
/// int rather than a large unsigned one.
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
    } else {
        return Err(Error::out_of_range("string length"));
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
    } else {
        return Err(Error::out_of_range("bin length"));
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

/// Encodes an NT4 meta-topic payload (array of maps) as raw MessagePack bytes.
///
/// `$`-prefixed meta topics carry msgpack-typed payloads whose value is an
/// array of maps with string keys.
pub(crate) fn encode_meta_payload(maps: &[Map<String, Json>]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_array_header(maps.len(), &mut buf)
        .expect("a meta payload never holds more than u32::MAX maps");
    for map in maps {
        encode_meta_map(map, &mut buf);
    }
    buf
}

fn encode_meta_map(map: &Map<String, Json>, buf: &mut Vec<u8>) {
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
        encode_str(key, buf).expect("a meta key is never longer than u32::MAX bytes");
        encode_meta_value(value, buf);
    }
}

fn encode_meta_value(v: &Json, buf: &mut Vec<u8>) {
    match v {
        Json::Null => buf.push(0xc0),
        Json::Bool(b) => {
            buf.push(if *b { 0xc3 } else { 0xc2 });
        }
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                encode_i64(i, buf).expect("encoding an i64 is infallible");
            } else {
                let f = n.as_f64().unwrap_or_default();
                buf.push(0xcb);
                buf.extend_from_slice(&f.to_bits().to_be_bytes());
            }
        }
        Json::String(s) => {
            encode_str(s, buf).expect("a meta string is never longer than u32::MAX bytes");
        }
        Json::Array(arr) => {
            encode_array_header(arr.len(), buf)
                .expect("a meta array never holds more than u32::MAX items");
            for item in arr {
                encode_meta_value(item, buf);
            }
        }
        Json::Object(map) => {
            encode_meta_map(map, buf);
        }
    }
}

fn decode_one(buf: &[u8], depth: usize) -> Result<(Value, usize), Error> {
    let (&marker, rest) = buf.split_first().ok_or_else(Error::unexpected_eof)?;
    match marker {
        0x00..=0x7f => Ok((Value::Uint8(marker), 1)),
        0xe0..=0xff => Ok((Value::Int8(marker as i8), 1)),
        0xc0 => Err(Error::unsupported("nil")),
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
            let (&b, _) = rest.split_first().ok_or_else(Error::unexpected_eof)?;
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
            let (&b, _) = rest.split_first().ok_or_else(Error::unexpected_eof)?;
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
            let bytes = rest.get(..len).ok_or_else(Error::unexpected_eof)?;
            let s =
                std::str::from_utf8(bytes).map_err(|_| Error::new("invalid utf-8 in string"))?;
            Ok((Value::String(s.to_string()), 1 + len))
        }
        0xd9 => {
            let (&len, rest) = rest.split_first().ok_or_else(Error::unexpected_eof)?;
            let bytes = rest.get(..len as usize).ok_or_else(Error::unexpected_eof)?;
            let s =
                std::str::from_utf8(bytes).map_err(|_| Error::new("invalid utf-8 in string"))?;
            Ok((Value::String(s.to_string()), 2 + len as usize))
        }
        0xda => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or_else(Error::unexpected_eof)?;
            let s =
                std::str::from_utf8(bytes).map_err(|_| Error::new("invalid utf-8 in string"))?;
            Ok((Value::String(s.to_string()), 3 + len))
        }
        0xc4 => {
            let (&len, rest) = rest.split_first().ok_or_else(Error::unexpected_eof)?;
            let bytes = rest.get(..len as usize).ok_or_else(Error::unexpected_eof)?;
            Ok((Value::Bytes(bytes.to_vec()), 2 + len as usize))
        }
        0xc5 => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or_else(Error::unexpected_eof)?;
            Ok((Value::Bytes(bytes.to_vec()), 3 + len))
        }
        0x90..=0x9f | 0xdc | 0xdd => {
            let (items, consumed) = decode_array_at(buf, depth)?;
            let value = classify_array(items)?;
            Ok((value, consumed))
        }
        _ => Err(Error::unsupported(format!("0x{marker:02x}"))),
    }
}

fn classify_array(items: Vec<Value>) -> Result<Value, Error> {
    if items.is_empty() {
        // An empty array carries no element type on the wire; double is the default.
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
        Err(Error::invalid_array("mixed element types"))
    }
}

fn take<const N: usize>(buf: &[u8]) -> Result<([u8; N], &[u8]), Error> {
    let bytes = buf.get(..N).ok_or_else(Error::unexpected_eof)?;
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
        assert!(matches!(
            decode_value(&buf[..buf.len() - 1]),
            Err(Error { .. })
        ));
    }

    #[test]
    fn decode_rejects_nil() {
        assert!(matches!(decode_value(&[0xc0]), Err(Error { .. })));
    }

    #[test]
    fn decode_rejects_arrays_nested_past_the_depth_limit() {
        // Every byte opens another array, so the input length is the nesting
        // depth. Without a limit this recurses until the stack is gone, which
        // aborts the process rather than closing the connection.
        let deep = vec![0x91u8; 1024 * 1024];
        assert!(matches!(decode_value(&deep), Err(Error { .. })));
    }

    #[test]
    fn decode_still_accepts_an_array_of_scalars() {
        let v = Value::Int64Array(vec![1, 2, 3]);
        let mut buf = Vec::new();
        encode_value(&v, &mut buf).unwrap();
        assert_eq!(decode_value(&buf).unwrap(), v);
    }

    #[test]
    fn decode_array_rejects_hostile_length() {
        // array32 header claiming 2^32-1 elements with no payload must error,
        // not attempt a ~128 GiB preallocation.
        assert!(matches!(
            decode_array(&[0xdd, 0xff, 0xff, 0xff, 0xff]),
            Err(Error { .. })
        ));
    }
}
