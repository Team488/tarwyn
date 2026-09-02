//! The NT4 WebSocket message layer.
//!
//! [`message`] holds the value and protocol message model; [`msgpack`] is the
//! hand-rolled MessagePack codec the value messages are carried in.

pub mod frame;
pub mod message;
pub mod msgpack;
pub mod protocol;
