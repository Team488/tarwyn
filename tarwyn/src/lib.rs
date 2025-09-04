pub mod utils {
    pub mod ports;
    pub mod ring_buffer;
}

pub mod tarwyn_server;

pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/protobuf.rs"));
}
