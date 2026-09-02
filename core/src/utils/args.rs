use std::sync::OnceLock;

use clap::Parser;

use crate::utils::ports;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Command-line arguments for the server binary.
pub struct TarwynArgs {
    /// Enable logging for the Tarwyn server
    #[arg(short, long, default_value_t = false)]
    pub log: bool,

    /// TCP port subscriptions are fanned out on
    #[arg(long, default_value_t = ports::DEFAULT_PUB_SUB_PORT)]
    pub pub_port: u16,

    /// TCP port publishes are received on
    #[arg(long, default_value_t = ports::DEFAULT_PUSH_PULL_PORT)]
    pub pull_port: u16,

    /// TCP port reads and the control plane are served on
    #[arg(long, default_value_t = ports::DEFAULT_WS_PORT)]
    pub rep_port: u16,

    /// UDP port the telemetry plane is relayed on
    #[arg(long, default_value_t = tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT)]
    pub telemetry_port: u16,
}

/// The parsed arguments, set once at startup and read from anywhere.
pub static CONFIG: OnceLock<TarwynArgs> = OnceLock::new();
