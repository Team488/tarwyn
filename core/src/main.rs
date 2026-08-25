use clap::Parser;
use log::info;
use tarwyn_server::{
    utils::{
        args::{CONFIG, TarwynArgs},
        log::init_logger,
    },
    tarwyn_server::TarwynServer,
};

fn main() {
    CONFIG
        .set(TarwynArgs::parse())
        .expect("Failed to set configuration");

    init_logger();

    let tarwyn_server = TarwynServer::new();
    tarwyn_server.start();

    info!("Tarwyn server started successfully.");

    std::thread::park();
}
