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

    let config = CONFIG.get().expect("configuration was just set");
    eprintln!(
        "tarwyn: WS {}, telemetry UDP {}",
        config.rep_port, config.telemetry_port
    );

    let tarwyn_server = match TarwynServer::try_with_ports_and_telemetry(
        config.pub_port,
        config.pull_port,
        config.rep_port,
        config.telemetry_port,
    ) {
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
    eprintln!("tarwyn: ready");

    loop {
        std::thread::park();
    }
}
