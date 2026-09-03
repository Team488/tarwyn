//! Latency harness for tarwyn-rust and the alternatives it is measured against.
//!
//! Each subject is a publisher and a subscriber in separate processes on one
//! host. Warmup samples are discarded before anything is recorded, and the rate
//! must stay below saturation or the run measures the queue rather than the
//! transport. See `bench/BENCHMARK.md` for the subjects and how to run them.

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
    Nt4,
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
            Subject::Nt4 => subjects::nt4::publish(&host, payload, rate, count),
        },
        Command::Subscriber {
            subject,
            payload,
            samples,
            addr,
            host,
        } => match subject {
            Subject::Udp => subjects::udp::subscribe(&addr, payload, samples),
            Subject::Nt4 => subjects::nt4::subscribe(&host, payload, samples),
        },
    }
}
