//! The NT4 numeric data-type table.

use crate::value::Value;

/// A numeric NT4 data type for a value, from the NT4 4.1 table: `0` bool,
/// `1` double, `2` int, `3` float, `4` string, `5` binary, and `+16` for arrays.
pub fn xt_data_type(v: &Value) -> u32 {
    match v {
        Value::Bool(_) => 0,
        Value::Double(_) => 1,
        Value::Float(_) => 3,
        Value::Int8(_) | Value::Uint8(_) => 2,
        Value::BoolArray(_) => 16,
        Value::DoubleArray(_) => 17,
        Value::FloatArray(_) => 19,
        Value::StringArray(_) => 20,
        Value::String(_) => 4,
        Value::Bytes(_) | Value::BytesList(_) | Value::Coordinate(_) | Value::Bezier(_) => 5,
        Value::Int16(_)
        | Value::Uint16(_)
        | Value::Int32(_)
        | Value::Uint32(_)
        | Value::Int64(_)
        | Value::Uint64(_) => 2,
        Value::Int8Array(_)
        | Value::Uint8Array(_)
        | Value::Int16Array(_)
        | Value::Uint16Array(_)
        | Value::Int32Array(_)
        | Value::Uint32Array(_)
        | Value::Int64Array(_)
        | Value::Uint64Array(_) => 18,
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

/// The numeric NT4 data type for a type string. Unknown strings (`msgpack`,
/// `struct:*` and so on) are binary, type 5, per NT4.
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
