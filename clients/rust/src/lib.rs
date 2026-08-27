//! A Rust client for [TARWYN](https://github.com/Team488/tarwyn).
//!
//! [`TarwynClient`](tarwyn_client::TarwynClient) speaks the same method names
//! as the original: every public `put`/`get` on its `Requests` class exists here,
//! across scalars, the seven list types, poses, coordinates and bezier curves.
//! `send_float` is an addition, as are the control-plane calls and
//! [`compare_and_set`](tarwyn_client::TarwynClient::compare_and_set).
//!
//! Values move over two transports. Publishes and reads go over ZeroMQ, which is
//! reliable and framed; [`publish_telemetry`](tarwyn_client::TarwynClient::publish_telemetry)
//! goes over UDP, which is roughly 3.6x faster and makes no delivery guarantee.
//!
//! ```no_run
//! use tarwyn_client::tarwyn_client::TarwynClient;
//!
//! let client = TarwynClient::new();
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
#![allow(dead_code)]

mod ports;

/// The client itself, its configuration, and the value types it carries.
pub mod tarwyn_client;
