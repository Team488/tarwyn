//! The TARWYN wire format, and the two things built directly on it.
//!
//! [`protobuf`] is generated from `proto/messages.proto` by
//! [`protox`](https://crates.io/crates/protox), a pure-Rust compiler, so a clean
//! build needs no `protoc`. [`telemetry`] is the UDP datagram format used by the
//! low-latency plane, and [`wpilog`] writes published values into a
//! [WPILOG](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
//! file that AdvantageScope, Elastic and the WPILib DataLogTool open directly.

#![warn(missing_docs)]

/// Message types generated from `proto/messages.proto`.
#[allow(missing_docs)]
pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/protobuf.rs"));
}

/// The UDP datagram format carrying the telemetry plane.
pub mod telemetry;
/// A WPILOG writer, and the background logger built on it.
pub mod wpilog;
