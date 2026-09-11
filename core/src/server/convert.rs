//! Conversions between the wire's protobuf value and the server's `Value`.

use crate::value::Value;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurves, BoolList, BytesList, CoordinateList, DoubleList, FloatList, IntegerList,
    LongList, StringList, supported_values,
};

impl From<supported_values::Kind> for Value {
    fn from(kind: supported_values::Kind) -> Self {
        use supported_values::Kind;
        match kind {
            Kind::String(v) => Value::String(v),
            Kind::Int32(v) => Value::Int32(v),
            Kind::Int64(v) => Value::Int64(v),
            Kind::Uint32(v) => Value::Uint32(v),
            Kind::Uint64(v) => Value::Uint64(v),
            Kind::Bool(v) => Value::Bool(v),
            Kind::Double(v) => Value::Double(v),
            Kind::Float(v) => Value::Float(v),
            Kind::Bytes(v) => Value::Bytes(v),
            Kind::StringList(list) => Value::StringArray(list.values),
            Kind::FloatList(list) => Value::FloatArray(list.values),
            Kind::BytesList(list) => Value::BytesList(list.encode_to_vec()),
            Kind::BoolList(list) => Value::BoolArray(list.values),
            Kind::DoubleList(list) => Value::DoubleArray(list.values),
            Kind::IntegerList(list) => Value::Int32Array(list.values),
            Kind::LongList(list) => Value::Int64Array(list.values),
            Kind::CoordinateList(list) => Value::Coordinate(list.encode_to_vec()),
            Kind::BezierCurve(curve) => Value::Bezier(curve.encode_to_vec()),
            Kind::BezierCurves(curves) => Value::Bezier(curves.encode_to_vec()),
            Kind::BezierCurvesList(list) => Value::Bezier(list.encode_to_vec()),
        }
    }
}

impl From<Value> for supported_values::Kind {
    fn from(value: Value) -> Self {
        use supported_values::Kind;
        match value {
            Value::Int8(v) => Kind::Int32(v as i32),
            Value::Int16(v) => Kind::Int32(v as i32),
            Value::Int32(v) => Kind::Int32(v),
            Value::Int64(v) => Kind::Int64(v),
            Value::Uint8(v) => Kind::Uint32(v as u32),
            Value::Uint16(v) => Kind::Uint32(v as u32),
            Value::Uint32(v) => Kind::Uint32(v),
            Value::Uint64(v) => Kind::Uint64(v),
            Value::Float(v) => Kind::Float(v),
            Value::Double(v) => Kind::Double(v),
            Value::String(v) => Kind::String(v),
            Value::Bool(v) => Kind::Bool(v),
            Value::Bytes(v) => Kind::Bytes(v),
            Value::Int8Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            Value::Int16Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            Value::Int32Array(v) => Kind::IntegerList(IntegerList { values: v }),
            Value::Int64Array(v) => Kind::LongList(LongList { values: v }),
            Value::Uint8Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            Value::Uint16Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            Value::Uint32Array(v) => Kind::IntegerList(IntegerList {
                values: v.into_iter().map(|x| x as i32).collect(),
            }),
            Value::Uint64Array(v) => Kind::LongList(LongList {
                values: v.into_iter().map(|x| x as i64).collect(),
            }),
            Value::FloatArray(v) => Kind::FloatList(FloatList { values: v }),
            Value::DoubleArray(v) => Kind::DoubleList(DoubleList { values: v }),
            Value::StringArray(v) => Kind::StringList(StringList { values: v }),
            Value::BoolArray(v) => Kind::BoolList(BoolList { values: v }),
            Value::BytesList(v) => {
                Kind::BytesList(BytesList::decode(v.as_slice()).unwrap_or_default())
            }
            Value::Coordinate(v) => {
                Kind::CoordinateList(CoordinateList::decode(v.as_slice()).unwrap_or_default())
            }
            Value::Bezier(v) => {
                Kind::BezierCurves(BezierCurves::decode(v.as_slice()).unwrap_or_default())
            }
        }
    }
}
