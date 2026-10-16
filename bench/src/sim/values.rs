//! The values a simulated topic publishes.

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
