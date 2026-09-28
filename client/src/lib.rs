//! A Rust client for the tarwyn server.
//!
//! Publishes, subscriptions and reads go over the WebSocket.
//! [`publish_telemetry`](client::Client::publish_telemetry) goes over UDP,
//! with no delivery guarantee.
//!
//! ```no_run
//! use tarwyn_client::client::Client;
//!
//! let client = Client::new();
//! let _unsubscribe = client.subscribe("test", |value| println!("{value:?}"));
//! client.start();
//! client.send_bool("test", true);
//! ```
//!
//! Channels starting with `TARWYN_INTERNAL` are reserved for the server.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod ports;

pub mod client;
pub mod config;
mod connection;
pub mod ffi;
mod listeners;
mod reader;
pub mod subscriber;
pub mod telemetry;
mod typed;

pub use client::Client;
pub use config::{Config, ConnectError};
pub use subscriber::CachedSubscriber;
pub use tarwyn_server::Value;
pub use tarwyn_server::websocket::pacing::DEFAULT_MARGIN as DEFAULT_PREDICT;
