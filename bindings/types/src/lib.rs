//! The value types every non-Rust tarwyn client shares.

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ServerStatistics {
    pub channels: u64,
    pub values: u64,
    pub telemetry_subscribers: u64,
    pub uptime_seconds: u64,
    pub dropped_publishes: u64,
    pub dropped_logs: u64,
    pub version: String,
}

#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq, Default)]
pub struct Coordinate {
    pub x: f64,
    pub y: f64,
}

#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub rotation_degrees: Option<f64>,
}

#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq, Default)]
pub struct Pose2d {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq, Default)]
pub struct Pose3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub qw: f64,
    pub qx: f64,
    pub qy: f64,
    pub qz: f64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct Update {
    pub channel: String,
    pub value: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct Telemetry {
    pub timestamp_micros: u64,
    pub payload: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct StructSchema {
    pub type_name: String,
    pub schema: String,
}

uniffi::setup_scaffolding!("tarwyn_types");
