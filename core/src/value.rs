//! The Tarwyn value model.
//!
//! `Value` is the value type the server stores and every transport
//! carries. It lives outside the websocket module so the core stays
//! independent of any wire format, as the spec requires.

/// One typed value: what a topic holds.
///
/// The variants cover the value space every transport shares: integers,
/// floats, strings, bools, raw bytes, typed lists, and the raw-byte geometry
/// types.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
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

impl Value {
    /// The value as an `i64`, if it is a signed integer variant.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int8(v) => Some(*v as i64),
            Value::Int16(v) => Some(*v as i64),
            Value::Int32(v) => Some(*v as i64),
            Value::Int64(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a `u64`, if it is an unsigned integer variant.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Uint8(v) => Some(*v as u64),
            Value::Uint16(v) => Some(*v as u64),
            Value::Uint32(v) => Some(*v as u64),
            Value::Uint64(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as an `f64`, if it is a float variant.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v as f64),
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a `&str`, if it is a string variant.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(v) => Some(v),
            _ => None,
        }
    }

    /// The value as a `bool`, if it is a bool variant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a `u64`, if it is any integer variant that is not negative.
    pub fn as_u64_any(&self) -> Option<u64> {
        self.as_u64()
            .or_else(|| self.as_i64().and_then(|x| u64::try_from(x).ok()))
    }

    /// The value with its 8- and 16-bit integer variants widened to 32 bits.
    ///
    /// MessagePack carries no integer width, so a decoder can only pick the
    /// smallest type a number fits, and `7` published as a `u32` arrives as
    /// [`Value::Uint8`]. The request plane answers in protobuf, whose
    /// narrowest integers are 32 bits wide, so a client that reports what it
    /// received unchanged would answer `get` and `subscribe` differently for
    /// the same channel. Widening on receipt makes them agree.
    #[must_use]
    pub fn widened(self) -> Value {
        match self {
            Value::Int8(v) => Value::Int32(v.into()),
            Value::Int16(v) => Value::Int32(v.into()),
            Value::Uint8(v) => Value::Uint32(v.into()),
            Value::Uint16(v) => Value::Uint32(v.into()),
            Value::Int8Array(v) => Value::Int32Array(v.into_iter().map(i32::from).collect()),
            Value::Int16Array(v) => Value::Int32Array(v.into_iter().map(i32::from).collect()),
            Value::Uint8Array(v) => Value::Uint32Array(v.into_iter().map(u32::from).collect()),
            Value::Uint16Array(v) => Value::Uint32Array(v.into_iter().map(u32::from).collect()),
            other => other,
        }
    }
}
