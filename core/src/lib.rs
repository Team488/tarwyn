//! The TARWYN server.
//!
//! [`TarwynServer`](tarwyn_server::TarwynServer) holds the value map and
//! serves publishes, reads, the control plane (get/delete/tables/ping/stats/json/CAS),
//! and log relay over a single WebSocket port (5810). A UDP telemetry plane (5809)
//! is retained for callers that want latency over delivery guarantees.
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

/// The value model every transport carries.
pub mod value;

/// The server itself.
pub mod tarwyn_server;

/// The WebSocket transport and the NT4 protocol spoken over it.
pub mod websocket;
