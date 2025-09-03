#![allow(dead_code)]

pub mod tarwyn {
    include!(concat!(env!("OUT_DIR"), "/tarwyn.rs"));
}

mod ports;

mod tarwyn_client;
