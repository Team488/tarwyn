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

    /// Address both planes listen on. 127.0.0.1 keeps the server local
    #[arg(long, default_value_t = crate::websocket::server::DEFAULT_BIND_HOST.to_string())]
    pub bind: String,

    /// UDP port the telemetry plane is relayed on
    #[arg(long, default_value_t = tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT)]
    pub telemetry_port: u16,

    /// Microseconds a connection's reader spins before blocking
    ///
    /// 0 blocks right away. Windows ignores the setting.
    #[arg(long, default_value_t = 0, value_name = "MICROS")]
    pub busy_poll: u64,

    /// Microseconds around a predicted arrival a connection's reader spins
    ///
    /// 0 turns prediction off. Windows ignores the setting.
    #[arg(long, default_value_t = crate::websocket::pacing::DEFAULT_MARGIN.as_micros() as u64, value_name = "MICROS")]
    pub predict: u64,
}
