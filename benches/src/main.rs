mod harness;
mod subjects;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(about = "Latency harness for tarwyn-rust and its alternatives")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Subject {
    Udp,
    Tarwyn,
    TarwynUdp,
    ZmqDirect,
}

#[derive(Subcommand)]
enum Command {
    Publisher {
        #[arg(long, value_enum, default_value = "udp")]
        subject: Subject,
        #[arg(long, default_value_t = 16)]
        payload: usize,
        #[arg(long, default_value_t = 1000)]
        rate: u64,
        #[arg(long, default_value_t = 100_000)]
        count: u64,
        #[arg(long, default_value = subjects::udp::DEFAULT_ADDR)]
        addr: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    Subscriber {
        #[arg(long, value_enum, default_value = "udp")]
        subject: Subject,
        #[arg(long, default_value_t = 16)]
        payload: usize,
        #[arg(long, default_value_t = 100_000)]
        samples: u64,
        #[arg(long, default_value = subjects::udp::DEFAULT_ADDR)]
        addr: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

fn main() -> std::io::Result<()> {
    match Cli::parse().command {
        Command::Publisher {
            subject,
            payload,
            rate,
            count,
            addr,
            host,
        } => match subject {
            Subject::Udp => subjects::udp::publish(&addr, payload, rate, count),
            Subject::Tarwyn => subjects::tarwyn::publish(&host, payload, rate, count),
            Subject::TarwynUdp => subjects::tarwyn_udp::publish(&host, payload, rate, count),
            Subject::ZmqDirect => subjects::zmq_direct::publish(
                subjects::zmq_direct::DEFAULT_ENDPOINT, payload, rate, count),
        },
        Command::Subscriber {
            subject,
            payload,
            samples,
            addr,
            host,
        } => match subject {
            Subject::Udp => subjects::udp::subscribe(&addr, payload, samples),
            Subject::Tarwyn => subjects::tarwyn::subscribe(&host, payload, samples),
            Subject::TarwynUdp => subjects::tarwyn_udp::subscribe(&host, payload, samples),
            Subject::ZmqDirect => subjects::zmq_direct::subscribe(
                subjects::zmq_direct::DEFAULT_ENDPOINT, payload, samples),
        },
    }
}
