pub mod utils {
    pub mod ports;
    pub mod ring_buffer;
}

pub mod tarwyn_server;

pub mod tarwyn {
    include!(concat!(env!("OUT_DIR"), "/tarwyn.rs"));
}
