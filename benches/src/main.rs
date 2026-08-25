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
    /// Raw UDP datagrams: the latency floor everything else is measured against.
    Udp,
}

#[derive(Subcommand)]
enum Command {
    /// Send messages at a fixed rate.
    Publisher {
        #[arg(long, value_enum, default_value = "udp")]
        subject: Subject,
        /// Wire payload in bytes. Values under 16 are rounded up to fit the header.
        #[arg(long, default_value_t = 16)]
        payload: usize,
        #[arg(long, default_value_t = 1000)]
        rate: u64,
        #[arg(long, default_value_t = 100_000)]
        count: u64,
        #[arg(long, default_value = subjects::udp::DEFAULT_ADDR)]
        addr: String,
    },
    /// Receive messages and report latency percentiles.
    Subscriber {
        #[arg(long, value_enum, default_value = "udp")]
        subject: Subject,
        #[arg(long, default_value_t = 16)]
        payload: usize,
        #[arg(long, default_value_t = 100_000)]
        samples: u64,
        #[arg(long, default_value = subjects::udp::DEFAULT_ADDR)]
        addr: String,
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
        } => match subject {
            Subject::Udp => subjects::udp::publish(&addr, payload, rate, count),
        },
        Command::Subscriber {
            subject,
            payload,
            samples,
            addr,
        } => match subject {
            Subject::Udp => subjects::udp::subscribe(&addr, payload, samples),
        },
    }
}
