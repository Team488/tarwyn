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
        config.bind, config.rep_port, config.telemetry_port
    );

    let tarwyn_server = match Server::try_with_bind(
        &config.bind,
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

    info!("tarwyn server started successfully.");
    eprintln!("tarwyn: ready");

    loop {
        std::thread::park();
    }
}
