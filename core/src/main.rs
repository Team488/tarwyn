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

    let tarwyn_server = match TarwynServer::try_new() {
        Ok(server) => server,
        Err(error) => {
            let mut message = error.to_string();
            let mut cause = std::error::Error::source(&error);
            while let Some(source) = cause {
                message.push_str(&format!(": {source}"));
                cause = source.source();
            }
            eprintln!("tarwyn: {message}");
            std::process::exit(1);
        }
    };
    tarwyn_server.start();

    info!("Tarwyn server started successfully.");

    std::thread::park();
}
