use clap::Parser;
use log::info;
use tarwyn_server::{
    server::Server,
    utils::{args::Args, log::init_logger},
};

/// An error and every cause under it, as one line.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        line.push_str(": ");
        line.push_str(&inner.to_string());
        cause = inner.source();
    }
    line
}

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
                eprintln!("tarwyn: {}", error_chain(&error));
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
