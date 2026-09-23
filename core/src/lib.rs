//! The tarwyn server: NT4 values and the control plane on WebSocket 5810,
//! telemetry on UDP 5809.
//!
//! ```no_run
//! use tarwyn_server::server::Server;
//!
//! let server = Server::new();
//! server.start();
//! std::thread::park();
//! ```
//!
//! Channels starting with `TARWYN_INTERNAL` are reserved for the server.

#![warn(missing_docs)]

/// Logging, argument parsing, and the buffers the server keeps.
pub mod utils {
    /// Command-line arguments for the server binary.
    pub mod args;
    /// The server's logger, and the history it retains for clients.
    pub mod log;
    /// The ports the server binds by default.
    pub mod ports;
    /// A fixed-capacity queue that drops its oldest item when full.
    pub mod ring_buffer;
}

/// The value model every transport carries.
pub mod value;

pub use value::Value;

/// The server itself.
pub mod server;

pub use server::Server;

/// The WebSocket transport and the NT4 protocol spoken over it.
pub mod websocket;
