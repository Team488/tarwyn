//! A Rust client for the tarwyn key/value server.
//!
//! [`Client`] speaks the same method names
//! as the original: every public `put`/`get` on its `Requests` class exists here,
//! across scalars, the seven list types, poses, coordinates and bezier curves.
//! `send_float` is an addition, as are the control-plane calls and
//! [`compare_and_set`](client::Client::compare_and_set).
//!
//! Values move over two transports. Publishes and reads go over ZeroMQ, which is
//! reliable and framed; [`publish_telemetry`](client::Client::publish_telemetry)
//! goes over UDP, which is roughly 3.6x faster and makes no delivery guarantee.
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
//! # Reserved names
//!
//! Channels beginning with `TARWYN_INTERNAL` are reserved for the server's own
//! traffic and may conflict with it.

#![warn(missing_docs)]

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
