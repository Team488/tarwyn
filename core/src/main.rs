use clap::Parser;
use log::info;
use tarwyn_server::{
    server::Server,
    utils::{args::Args, log::init_logger},
};

fn main() {
    let config = Args::parse();
    init_logger(config.log);
    eprintln!(
        "tarwyn: WebSocket {}:{}, telemetry UDP {}",
        config.bind, config.port, config.telemetry_port
    );

    let tarwyn_server =
        match Server::try_with_bind(&config.bind, config.port, config.telemetry_port) {
            Ok(server) => server,
            Err(error) => {
                eprintln!("tarwyn: {:#}", anyhow::Error::from(error));
                std::process::exit(1);
            }
        };
    tarwyn_server.set_busy_poll(std::time::Duration::from_micros(config.busy_poll));
    tarwyn_server.set_predict(std::time::Duration::from_micros(config.predict));
    tarwyn_server.start();

    info!("tarwyn server started successfully.");
    eprintln!("tarwyn: ready");

    loop {
        std::thread::park();
    }
}
