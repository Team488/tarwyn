//! Latency harness for tarwyn-rust and the alternatives it is measured against.
//!
//! Each subject is a publisher and a subscriber in separate processes on one
//! host. Warmup samples are discarded before anything is recorded, and the rate
//! must stay below saturation or the run measures the queue rather than the
//! transport. See `bench/BENCHMARK.md` for the subjects and how to run them.

mod catalog;

mod cases;

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
    Telemetry,
    Client,
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
    /// Print the catalog as `name<TAB>group<TAB>mode<TAB>implementations`.
    ListCases,
    /// Look a case up in the catalog and run it for one implementation.
    Run {
        #[arg(long)]
        case: String,
        #[arg(long = "impl")]
        implementation: String,
        #[arg(long, default_value_t = 96)]
        payload: usize,
        #[arg(long, default_value_t = 500)]
        rate: u64,
        #[arg(long, default_value_t = 12000)]
        count: u64,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "subscriber")]
        role: String,
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
            Subject::Telemetry => subjects::telemetry::publish(&host, payload, rate, count),
            Subject::Client => subjects::client::publish(&host, payload, rate, count),
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
            Subject::Telemetry => subjects::telemetry::subscribe(&host, payload, samples),
            // The client subject differs only in who publishes; the subscriber
            // is the ordinary NT4 one.
            Subject::Client => subjects::nt4::subscribe(&host, payload, samples),
        },
        Command::ListCases => {
            for case in catalog::CASES {
                println!(
                    "{}\t{}\t{}\t{}",
                    case.name,
                    case.group,
                    case.mode.as_str(),
                    case.implementations.join(",")
                );
            }
            Ok(())
        }
        Command::Run {
            case,
            implementation,
            payload,
            rate,
            count,
            samples,
            host,
            role,
        } => {
            let Some(declared) = catalog::find(&case) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{case} is not in the catalog"),
                ));
            };
            if !declared.implementations.contains(&implementation.as_str()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{case} does not declare {implementation}"),
                ));
            }
            match declared.mode {
                catalog::Mode::RoundTrip => {
                    let (latencies, elapsed) =
                        cases::read::run(&case, &host, rate, samples, samples.min(200))?;
                    cases::read::report(&case, &implementation, payload, &latencies, elapsed);
                    Ok(())
                }
                catalog::Mode::Delivery => subjects::run_delivery(
                    &case,
                    &implementation,
                    &role,
                    &host,
                    payload,
                    rate,
                    count,
                    samples,
                ),
            }
        }
    }
}
