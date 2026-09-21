use clap::Parser;

use crate::utils::ports;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Command-line arguments for the server binary.
pub struct Args {
    /// Enable logging for the tarwyn server
    #[arg(short, long, default_value_t = false)]
    pub log: bool,

    /// TCP port the WebSocket plane is served on: values, reads and control
    #[arg(long, default_value_t = ports::DEFAULT_WEBSOCKET_PORT)]
    pub port: u16,

    /// Address the WebSocket plane listens on
    #[arg(long, default_value_t = crate::websocket::server::DEFAULT_BIND_HOST.to_string())]
    pub bind: String,

    /// UDP port the telemetry plane is relayed on
    #[arg(long, default_value_t = tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT)]
    pub telemetry_port: u16,

    /// Microseconds a connection's reader spins on its socket before blocking
    ///
    /// Keeps a core busy for that long after every message in exchange for
    /// taking the wakeup off the latency path; 0 blocks at once.
    #[arg(long, default_value_t = 0, value_name = "MICROS")]
    pub busy_poll: u64,

    /// Microseconds around a predicted arrival a connection's reader spins
    ///
    /// A reader that has seen a periodic stream sleeps until this long before
    /// the next message is due and spins until this long after it; 0 turns
    /// prediction off.
    #[arg(long, default_value_t = crate::websocket::pacing::DEFAULT_MARGIN.as_micros() as u64, value_name = "MICROS")]
    pub predict: u64,
}
