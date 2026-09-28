//! The value type the server stores and every transport carries.

/// One typed value: what a topic holds.
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

    /// The value with 8- and 16-bit integers widened to 32 bits, the narrowest
    /// width the protobuf replies use.
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

    /// The value reshaped to NT4 data type `data_type`, as ntcore reads it.
    ///
    /// An empty array takes the declared element type, and integers widen to
    /// a declared float type. Anything else is returned unchanged.
    #[must_use]
    pub fn conformed(self, data_type: u32) -> Value {
        use crate::websocket::protocol::xt_data_type;

        if xt_data_type(&self) == data_type {
            return self;
        }
        let integers = |value: &Value| -> Option<Vec<f64>> {
            match value {
                Value::Int8Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Int16Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Int32Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Int64Array(v) => Some(v.iter().map(|x| *x as f64).collect()),
                Value::Uint8Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Uint16Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Uint32Array(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::Uint64Array(v) => Some(v.iter().map(|x| *x as f64).collect()),
                Value::FloatArray(v) => Some(v.iter().map(|x| f64::from(*x)).collect()),
                Value::DoubleArray(v) => Some(v.clone()),
                _ => None,
            }
        };
        let empty = matches!(&self, Value::DoubleArray(v) if v.is_empty());
        let number = self
            .as_f64()
            .or_else(|| self.as_i64().map(|x| x as f64))
            .or_else(|| self.as_u64().map(|x| x as f64));
        match data_type {
            1 => number.map_or(self, Value::Double),
            3 => number.map_or(self, |x| Value::Float(x as f32)),
            16 if empty => Value::BoolArray(Vec::new()),
            18 if empty => Value::Int64Array(Vec::new()),
            20 if empty => Value::StringArray(Vec::new()),
            17 => integers(&self).map_or(self, Value::DoubleArray),
            19 => integers(&self).map_or(self, |xs| {
                Value::FloatArray(xs.into_iter().map(|x| x as f32).collect())
            }),
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn an_empty_array_takes_the_declared_element_type() {
        let empty = Value::DoubleArray(Vec::new());
        assert_eq!(empty.clone().conformed(20), Value::StringArray(Vec::new()));
        assert_eq!(empty.clone().conformed(16), Value::BoolArray(Vec::new()));
        assert_eq!(empty.conformed(18), Value::Int64Array(Vec::new()));
    }

    #[test]
    fn integers_widen_to_a_declared_float_type() {
        assert_eq!(Value::Uint8(3).conformed(1), Value::Double(3.0));
        assert_eq!(Value::Int8(-2).conformed(3), Value::Float(-2.0));
        assert_eq!(
            Value::Int64Array(vec![1, 2]).conformed(17),
            Value::DoubleArray(vec![1.0, 2.0])
        );
    }

    #[test]
    fn a_value_of_another_kind_is_left_for_the_type_check() {
        let text = Value::String("x".into());
        assert_eq!(text.clone().conformed(1), text);
        let ints = Value::Int64Array(vec![1]);
        assert_eq!(ints.clone().conformed(20), ints);
    }
}
