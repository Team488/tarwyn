pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/protobuf.rs"));
}

pub mod telemetry;
pub mod wpilog;
