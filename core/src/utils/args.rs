use clap::Parser;

use crate::utils::ports;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Command-line arguments for the server binary.
pub struct Args {
    /// Enable logging for the tarwyn server
    #[arg(short, long, default_value_t = false)]
    pub log: bool,

    /// TCP port subscriptions are fanned out on
    #[arg(long, default_value_t = ports::DEFAULT_PUB_SUB_PORT)]
    pub pub_port: u16,

    /// TCP port publishes are received on
    #[arg(long, default_value_t = ports::DEFAULT_PUSH_PULL_PORT)]
    pub pull_port: u16,

    /// TCP port reads and the control plane are served on
    #[arg(long, default_value_t = ports::DEFAULT_WEBSOCKET_PORT)]
    pub rep_port: u16,

    /// Address the WebSocket plane listens on
    #[arg(long, default_value_t = crate::websocket::server::DEFAULT_BIND_HOST.to_string())]
    pub bind: String,

    /// UDP port the telemetry plane is relayed on
    #[arg(long, default_value_t = tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT)]
    pub telemetry_port: u16,
}
