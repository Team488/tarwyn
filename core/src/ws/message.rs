//! The NT4 value and protocol message model.
//!
//! [`XtValue`] is a pure value enum covering the NT4 value space. The control
//! ([`CtMessage`]) and value ([`ValueMessage`]) message forms are added in
//! Task 2; the MessagePack codec lives in [`crate::ws::msgpack`].

// Rust guideline compliant 2026-02-21

/// A pure NT4 value, with no topic metadata.
///
/// The variants mirror the NT4 value space: integers, floats, strings, bools,
/// raw bytes, typed lists, and the type-5 raw-byte geometry types.
#[derive(Debug, Clone, PartialEq)]
pub enum XtValue {
    /// 8-bit signed integer.
    Int8(i8),
    /// 16-bit signed integer.
    Int16(i16),
    /// 32-bit signed integer.
    Int32(i32),
    /// 64-bit signed integer.
    Int64(i64),
    /// 8-bit unsigned integer.
    Uint8(u8),
    /// 16-bit unsigned integer.
    Uint16(u16),
    /// 32-bit unsigned integer.
    Uint32(u32),
    /// 64-bit unsigned integer.
    Uint64(u64),
    /// 32-bit float.
    Float(f32),
    /// 64-bit float.
    Double(f64),
    /// UTF-8 string.
    String(String),
    /// Boolean.
    Bool(bool),
    /// Raw bytes (`Bytes[Kind]`).
    Bytes(Vec<u8>),
    /// List of 8-bit signed integers.
    Int8Array(Vec<i8>),
    /// List of 16-bit signed integers.
    Int16Array(Vec<i16>),
    /// List of 32-bit signed integers.
    Int32Array(Vec<i32>),
    /// List of 64-bit signed integers.
    Int64Array(Vec<i64>),
    /// List of 8-bit unsigned integers.
    Uint8Array(Vec<u8>),
    /// List of 16-bit unsigned integers.
    Uint16Array(Vec<u16>),
    /// List of 32-bit unsigned integers.
    Uint32Array(Vec<u32>),
    /// List of 64-bit unsigned integers.
    Uint64Array(Vec<u64>),
    /// List of 32-bit floats.
    FloatArray(Vec<f32>),
    /// List of 64-bit floats.
    DoubleArray(Vec<f64>),
    /// List of strings.
    StringArray(Vec<String>),
    /// List of booleans.
    BoolArray(Vec<bool>),
    /// A list of raw byte arrays, encoded as type-5 raw bytes.
    BytesList(Vec<u8>),
    /// A coordinate, encoded as type-5 raw bytes.
    Coordinate(Vec<u8>),
    /// A bezier curve, encoded as type-5 raw bytes.
    Bezier(Vec<u8>),
}

impl XtValue {
    /// The value as an `i64`, if it is a signed integer variant.
    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            XtValue::Int8(v) => Some(*v as i64),
            XtValue::Int16(v) => Some(*v as i64),
            XtValue::Int32(v) => Some(*v as i64),
            XtValue::Int64(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a `u64`, if it is an unsigned integer variant.
    pub(crate) fn as_u64(&self) -> Option<u64> {
        match self {
            XtValue::Uint8(v) => Some(*v as u64),
            XtValue::Uint16(v) => Some(*v as u64),
            XtValue::Uint32(v) => Some(*v as u64),
            XtValue::Uint64(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as an `f64`, if it is a float variant.
    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            XtValue::Float(v) => Some(*v as f64),
            XtValue::Double(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a `&str`, if it is a string variant.
    pub(crate) fn as_string(&self) -> Option<&str> {
        match self {
            XtValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// The value as a `bool`, if it is a bool variant.
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            XtValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
}