//! The WebSocket transport and the NT4 protocol spoken over it.
//!
//! `message` holds the NT4 protocol message model; `msgpack` is the
//! hand-rolled MessagePack codec the value messages are carried in. The value
//! type itself lives in [`crate::value`], outside this module, so the core
//! stays independent of the wire format.

pub mod frame;
pub mod message;
pub mod msgpack;
pub mod protocol;
pub mod server;
pub mod transport;
