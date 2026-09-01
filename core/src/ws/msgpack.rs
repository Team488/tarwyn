//! A minimal hand-rolled MessagePack codec for NT4 values.
//!
//! Follows the original MPack spec bytes
//! (https://github.com/msgpack/msgpack/blob/master/spec.md). Only the subset
//! NT4 needs is implemented: ints, floats, str, bin, bool, nil, and arrays.

// Rust guideline compliant 2026-02-21

use std::fmt;

use crate::ws::message::XtValue;

/// An error from encoding or decoding a MessagePack value.
///
/// Carries a human-readable message; no payload is needed beyond that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgpackError {
    message: String,
}

impl MsgpackError {
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

impl fmt::Display for MsgpackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MsgpackError {}

/// Encodes `v` into `buf` as MessagePack.
pub fn encode_value(v: &XtValue, buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
    match v {
        XtValue::Int8(x) => encode_i64(*x as i64, buf),
        XtValue::Int16(x) => encode_i64(*x as i64, buf),
        XtValue::Int32(x) => encode_i64(*x as i64, buf),
        XtValue::Int64(x) => encode_i64(*x, buf),
        XtValue::Uint8(x) => encode_uint(*x as u64, buf),
        XtValue::Uint16(x) => encode_uint(*x as u64, buf),
        XtValue::Uint32(x) => encode_uint(*x as u64, buf),
        XtValue::Uint64(x) => encode_uint(*x, buf),
        XtValue::Float(x) => {
            buf.push(0xca);
            buf.extend_from_slice(&x.to_bits().to_be_bytes());
            Ok(())
        }
        XtValue::Double(x) => {
            buf.push(0xcb);
            buf.extend_from_slice(&x.to_bits().to_be_bytes());
            Ok(())
        }
        XtValue::String(s) => encode_str(s, buf),
        XtValue::Bool(b) => {
            buf.push(if *b { 0xc3 } else { 0xc2 });
            Ok(())
        }
        XtValue::Bytes(b) => encode_bin(b, buf),
        XtValue::Int8Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Int8(*x)),
        XtValue::Int16Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Int16(*x)),
        XtValue::Int32Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Int32(*x)),
        XtValue::Int64Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Int64(*x)),
        XtValue::Uint8Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Uint8(*x)),
        XtValue::Uint16Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Uint16(*x)),
        XtValue::Uint32Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Uint32(*x)),
        XtValue::Uint64Array(xs) => encode_typed_array(xs, buf, |x| XtValue::Uint64(*x)),
        XtValue::FloatArray(xs) => encode_typed_array(xs, buf, |x| XtValue::Float(*x)),
        XtValue::DoubleArray(xs) => encode_typed_array(xs, buf, |x| XtValue::Double(*x)),
        XtValue::StringArray(xs) => encode_typed_array(xs, buf, |x| XtValue::String(x.clone())),
        XtValue::BoolArray(xs) => encode_typed_array(xs, buf, |x| XtValue::Bool(*x)),
        XtValue::BytesList(b) | XtValue::Coordinate(b) | XtValue::Bezier(b) => encode_bin(b, buf),
    }
}

/// Decodes one MessagePack value from `buf`, requiring the whole input be used.
pub fn decode_value(buf: &[u8]) -> Result<XtValue, MsgpackError> {
    let (value, consumed) = decode_one(buf)?;
    if consumed != buf.len() {
        return Err(MsgpackError::trailing_bytes());
    }
    Ok(value)
}

/// Decodes a MessagePack array header and its elements.
///
/// Returns the raw elements and the number of bytes consumed, so callers can
/// decode a value message's 4-tuple without classifying the array.
pub(crate) fn decode_array(buf: &[u8]) -> Result<(Vec<XtValue>, usize), MsgpackError> {
    let (&marker, rest) = buf.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
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
        _ => return Err(MsgpackError::not_an_array()),
    };
    let mut items = Vec::with_capacity(len);
    let mut rest = rest;
    for _ in 0..len {
        let (item, consumed) = decode_one(rest)?;
        items.push(item);
        rest = &rest[consumed..];
    }
    let consumed = buf.len() - rest.len();
    Ok((items, consumed))
}

/// Writes a MessagePack array header for `len` elements.
pub(crate) fn encode_array_header(len: usize, buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
    if len <= 0x0f {
        buf.push(0x90 | len as u8);
    } else if len <= 0xffff {
        buf.push(0xdc);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        buf.push(0xdd);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    } else {
        return Err(MsgpackError::out_of_range("array length"));
    }
    Ok(())
}

/// Encodes a `u64` as the smallest signed MessagePack int that holds it.
///
/// NT4 timestamps and ids are Java `long`s on the wire, so values that fit an
/// `i64` use the signed forms (int8/int16/int32/int64); only values above
/// `i64::MAX` fall back to uint64.
pub(crate) fn encode_uint(x: u64, buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
    if x <= i64::MAX as u64 {
        encode_i64(x as i64, buf)
    } else {
        buf.push(0xcf);
        buf.extend_from_slice(&x.to_be_bytes());
        Ok(())
    }
}

fn encode_i64(x: i64, buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
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

fn encode_str(s: &str, buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
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
        return Err(MsgpackError::out_of_range("string length"));
    }
    buf.extend_from_slice(s.as_bytes());
    Ok(())
}

fn encode_bin(b: &[u8], buf: &mut Vec<u8>) -> Result<(), MsgpackError> {
    let len = b.len();
    if len <= 0xff {
        buf.push(0xc4);
        buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0xc5);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        return Err(MsgpackError::out_of_range("bin length"));
    }
    buf.extend_from_slice(b);
    Ok(())
}

fn encode_typed_array<T>(
    xs: &[T],
    buf: &mut Vec<u8>,
    f: impl Fn(&T) -> XtValue,
) -> Result<(), MsgpackError> {
    encode_array_header(xs.len(), buf)?;
    for x in xs {
        encode_value(&f(x), buf)?;
    }
    Ok(())
}

fn decode_one(buf: &[u8]) -> Result<(XtValue, usize), MsgpackError> {
    let (&marker, rest) = buf.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
    match marker {
        0x00..=0x7f => Ok((XtValue::Uint8(marker), 1)),
        0xe0..=0xff => Ok((XtValue::Int8(marker as i8), 1)),
        0xc0 => Err(MsgpackError::unsupported("nil")),
        0xc2 => Ok((XtValue::Bool(false), 1)),
        0xc3 => Ok((XtValue::Bool(true), 1)),
        0xca => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((XtValue::Float(f32::from_bits(u32::from_be_bytes(bytes))), 5))
        }
        0xcb => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((XtValue::Double(f64::from_bits(u64::from_be_bytes(bytes))), 9))
        }
        0xcc => {
            let (&b, _) = rest.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
            Ok((XtValue::Uint8(b), 2))
        }
        0xcd => {
            let (bytes, _) = take::<2>(rest)?;
            Ok((XtValue::Uint16(u16::from_be_bytes(bytes)), 3))
        }
        0xce => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((XtValue::Uint32(u32::from_be_bytes(bytes)), 5))
        }
        0xcf => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((XtValue::Uint64(u64::from_be_bytes(bytes)), 9))
        }
        0xd0 => {
            let (&b, _) = rest.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
            Ok((XtValue::Int8(b as i8), 2))
        }
        0xd1 => {
            let (bytes, _) = take::<2>(rest)?;
            Ok((XtValue::Int16(i16::from_be_bytes(bytes)), 3))
        }
        0xd2 => {
            let (bytes, _) = take::<4>(rest)?;
            Ok((XtValue::Int32(i32::from_be_bytes(bytes)), 5))
        }
        0xd3 => {
            let (bytes, _) = take::<8>(rest)?;
            Ok((XtValue::Int64(i64::from_be_bytes(bytes)), 9))
        }
        0xa0..=0xbf => {
            let len = (marker & 0x1f) as usize;
            let bytes = rest.get(..len).ok_or_else(MsgpackError::unexpected_eof)?;
            let s = std::str::from_utf8(bytes)
                .map_err(|_| MsgpackError::new("invalid utf-8 in string"))?;
            Ok((XtValue::String(s.to_string()), 1 + len))
        }
        0xd9 => {
            let (&len, rest) = rest.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
            let bytes = rest.get(..len as usize).ok_or_else(MsgpackError::unexpected_eof)?;
            let s = std::str::from_utf8(bytes)
                .map_err(|_| MsgpackError::new("invalid utf-8 in string"))?;
            Ok((XtValue::String(s.to_string()), 2 + len as usize))
        }
        0xda => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or_else(MsgpackError::unexpected_eof)?;
            let s = std::str::from_utf8(bytes)
                .map_err(|_| MsgpackError::new("invalid utf-8 in string"))?;
            Ok((XtValue::String(s.to_string()), 3 + len))
        }
        0xc4 => {
            let (&len, rest) = rest.split_first().ok_or_else(MsgpackError::unexpected_eof)?;
            let bytes = rest.get(..len as usize).ok_or_else(MsgpackError::unexpected_eof)?;
            Ok((XtValue::Bytes(bytes.to_vec()), 2 + len as usize))
        }
        0xc5 => {
            let (bytes, rest) = take::<2>(rest)?;
            let len = u16::from_be_bytes(bytes) as usize;
            let bytes = rest.get(..len).ok_or_else(MsgpackError::unexpected_eof)?;
            Ok((XtValue::Bytes(bytes.to_vec()), 3 + len))
        }
        0x90..=0x9f | 0xdc | 0xdd => {
            let (items, consumed) = decode_array(buf)?;
            let value = classify_array(items)?;
            Ok((value, consumed))
        }
        _ => Err(MsgpackError::unsupported(format!("0x{marker:02x}"))),
    }
}

fn classify_array(items: Vec<XtValue>) -> Result<XtValue, MsgpackError> {
    if items.is_empty() {
        // An empty array carries no element type on the wire; double is the default.
        return Ok(XtValue::DoubleArray(Vec::new()));
    }
    let all_ints = items
        .iter()
        .all(|v| v.as_i64().is_some() || v.as_u64().is_some());
    let all_float32 = items.iter().all(|v| matches!(v, XtValue::Float(_)));
    let all_float64 = items.iter().all(|v| matches!(v, XtValue::Double(_)));
    let all_strings = items.iter().all(|v| v.as_string().is_some());
    let all_bools = items.iter().all(|v| v.as_bool().is_some());
    if all_ints {
        let any_big = items
            .iter()
            .any(|v| v.as_u64().is_some_and(|x| x > i64::MAX as u64));
        if any_big {
            let xs = items
                .iter()
                .map(|v| v.as_u64().unwrap_or_else(|| v.as_i64().unwrap() as u64))
                .collect();
            Ok(XtValue::Uint64Array(xs))
        } else {
            let xs = items
                .iter()
                .map(|v| v.as_i64().unwrap_or_else(|| v.as_u64().unwrap() as i64))
                .collect();
            Ok(XtValue::Int64Array(xs))
        }
    } else if all_float32 {
        let xs = items.iter().map(|v| v.as_f64().unwrap() as f32).collect();
        Ok(XtValue::FloatArray(xs))
    } else if all_float64 {
        let xs = items.iter().map(|v| v.as_f64().unwrap()).collect();
        Ok(XtValue::DoubleArray(xs))
    } else if all_strings {
        let xs = items
            .iter()
            .map(|v| v.as_string().unwrap().to_string())
            .collect();
        Ok(XtValue::StringArray(xs))
    } else if all_bools {
        let xs = items.iter().map(|v| v.as_bool().unwrap()).collect();
        Ok(XtValue::BoolArray(xs))
    } else {
        Err(MsgpackError::invalid_array("mixed element types"))
    }
}

fn take<const N: usize>(buf: &[u8]) -> Result<([u8; N], &[u8]), MsgpackError> {
    let bytes = buf.get(..N).ok_or_else(MsgpackError::unexpected_eof)?;
    let mut arr = [0u8; N];
    arr.copy_from_slice(bytes);
    Ok((arr, &buf[N..]))
}

#[cfg(test)]
mod tests {
    use crate::ws::message::XtValue;
    use crate::ws::msgpack::{decode_value, encode_value, MsgpackError};

    #[test]
    fn double_round_trip() {
        let v = XtValue::Double(4.344505251111111);
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
            XtValue::Int64(5),
            XtValue::Uint64(5),
            XtValue::Float(1.5),
            XtValue::Double(2.5),
            XtValue::String("hello".to_string()),
            XtValue::Bool(true),
            XtValue::Bytes(vec![1, 2, 3]),
            XtValue::Int64Array(vec![1, 2, 3]),
            XtValue::StringArray(vec!["a".to_string(), "b".to_string()]),
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
        let v = XtValue::DoubleArray(vec![1.5, 2.5, -3.25]);
        let mut buf = Vec::new();
        encode_value(&v, &mut buf).unwrap();
        assert_eq!(decode_value(&buf).unwrap(), v);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        let mut buf = Vec::new();
        encode_value(&XtValue::Double(1.0), &mut buf).unwrap();
        assert!(matches!(
            decode_value(&buf[..buf.len() - 1]),
            Err(MsgpackError { .. })
        ));
    }

    #[test]
    fn decode_rejects_nil() {
        assert!(matches!(decode_value(&[0xc0]), Err(MsgpackError { .. })));
    }
}