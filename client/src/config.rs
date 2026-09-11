//! How a client is configured, and how connecting can fail.

use std::time::Duration;

use tarwyn_protobuf::telemetry;

use crate::ports;

/// Why a client could not be built.
///
/// Every variant is a failure to set up the connection before any traffic is
/// attempted. Once a client exists, a server that is absent or unreachable is
/// not an error - publishes drop and reads return `None`.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// The host could not be resolved to an address for the WebSocket.
    #[error("could not connect the {socket} socket to {endpoint}")]
    Connect {
        /// Which socket failed.
        socket: &'static str,
        /// The endpoint it was given.
        endpoint: String,
        /// The underlying resolver error.
        source: std::io::Error,
    },
    /// No UDP socket could be bound for the telemetry plane.
    #[error("could not bind a telemetry socket")]
    Telemetry(#[from] std::io::Error),
    /// The host could not be resolved to an address for the telemetry plane.
    #[error("could not resolve {host} for the telemetry plane")]
    Resolve {
        /// The host that was given.
        host: String,
        /// The underlying resolver error.
        source: std::io::Error,
    },
}

#[derive(Clone, Debug)]
/// Where the client dials and how patient it is.
///
/// [`Default`] points at `127.0.0.1` on the standard ports with a 500 ms
/// request timeout.
pub struct Config {
    /// Host running the server. An address, not a URL.
    pub host: String,
    /// PUSH/PULL port. Retained for compatibility; the WebSocket carries
    /// publishes, so this is unused.
    pub push_port: u16,
    /// REQ/REP port, used by [`get`](crate::Client::get), the control plane and
    /// every publish and subscription - the WebSocket binds here.
    pub req_port: u16,
    /// PUB/SUB port. Retained for compatibility; the WebSocket carries
    /// subscriptions, so this is unused.
    pub sub_port: u16,
    /// How long a request waits for its reply before giving up and returning `None`.
    pub request_timeout: Duration,
    /// High-water mark on the outbound queue. Publishes past it are dropped,
    /// not queued; [`dropped_publishes`](crate::Client::dropped_publishes) counts them.
    pub send_high_water_mark: i32,
    /// UDP port for the telemetry plane.
    pub telemetry_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "127.0.0.1".to_string(),
            push_port: ports::DEFAULT_PUSH_PULL_PORT,
            req_port: ports::DEFAULT_REQ_REP_PORT,
            sub_port: ports::DEFAULT_PUB_SUB_PORT,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
    }
}
