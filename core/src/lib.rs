//! The TARWYN server.
//!
//! [`TarwynServer`](tarwyn_server::TarwynServer) holds the value map and
//! serves it over ZeroMQ — PULL for publishes, PUB for subscriptions, REP for
//! reads and the control plane — alongside a UDP telemetry plane for callers that
//! want latency over delivery guarantees.
//!
//! ```no_run
//! use tarwyn_server::tarwyn_server::TarwynServer;
//!
//! let server = TarwynServer::new();
//! server.start();
//! std::thread::park();
//! ```
//!
//! # Reserved names
//!
//! Channels beginning with `TARWYN_INTERNAL` are reserved for the server's own
//! traffic and may conflict with it.

#![warn(missing_docs)]

/// Logging, argument parsing, and the buffers the server keeps.
pub mod utils {
    /// Command-line arguments for the server binary.
    pub mod args;
    /// The server's logger, and the history it retains for clients.
    pub mod log;
    /// The ports the server binds by default.
    pub mod ports;
    /// A fixed-capacity queue that evicts rather than grows.
    pub mod ring_buffer;
}

/// The server itself.
pub mod tarwyn_server;
