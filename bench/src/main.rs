//! Latency harness for tarwyn-rust and the alternatives it is measured against.
//!
//! Each case is a publisher and a subscriber in separate processes on one
//! host. Warmup samples are discarded before anything is recorded, and the rate
//! must stay below saturation or the run measures the queue rather than the
//! transport. See `bench/BENCHMARK.md` for the cases and how to run them.
//!
//! Harnesses in other languages do not measure anything. They move bytes and
//! stamp two clocks; `bench row` turns their sample lines into the same `ROW`
//! line this binary emits for its own probes, through the same histogram.

mod catalog;

mod harness;

mod report;

mod run;

mod probes;

use clap::{Parser, Subcommand};
use harness::RowId;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Latency harness for tarwyn-rust and its alternatives")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the catalog as `name<TAB>group<TAB>implementations`.
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
        #[arg(long)]
        role: String,
        /// The measured implementation's version; defaults to this crate's.
        #[arg(long)]
        version: Option<String>,
    },
    /// Turn a foreign harness's `S` sample lines into one `ROW` line.
    Row {
        /// The file the foreign subscriber wrote its sample lines to.
        #[arg(long)]
        samples: PathBuf,
        #[arg(long)]
        case: String,
        #[arg(long = "impl")]
        implementation: String,
        #[arg(long)]
        payload: usize,
        #[arg(long)]
        version: String,
    },
    /// Run every case the catalog declares, at every payload, for every rep.
    Sweep {
        #[arg(long, default_value_t = 500)]
        rate: u64,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value_t = 500)]
        warmup: u64,
        #[arg(long, default_value_t = 12000)]
        count: u64,
        /// Wire sizes in bytes.
        #[arg(long, value_delimiter = ' ', default_values_t = [16usize, 96])]
        payloads: Vec<usize>,
        #[arg(long, default_value_t = 3)]
        reps: u32,
        /// Seconds before a probe is killed.
        #[arg(long, default_value_t = 90)]
        limit: u64,
        /// Seconds to wait after a subscriber says it is ready.
        #[arg(long, default_value_t = 5)]
        sub_settle: u64,
        /// Which cases to run; all of them when empty.
        #[arg(long, value_delimiter = ' ')]
        cases: Vec<String>,
        /// Do not pin each process to a physical core.
        #[arg(long)]
        no_pin: bool,
        /// Rebuild the report from the rows a previous run left on disk.
        #[arg(long)]
        only_report: bool,
        #[arg(long, default_value = "target/bench-rows")]
        rows: PathBuf,
        #[arg(long, default_value = "target/bench/results.json")]
        json: PathBuf,
        #[arg(long, default_value = "bench/RESULTS.md")]
        markdown: PathBuf,
    },
    /// Run one pair for a long time and report latency per window.
    Soak {
        /// Seconds to run.
        #[arg(long, default_value_t = 3600)]
        duration: u64,
        /// Seconds per reported row.
        #[arg(long, default_value_t = 60)]
        window: u64,
        #[arg(long, default_value_t = 500)]
        rate: u64,
        #[arg(long, default_value_t = 96)]
        payload: usize,
        #[arg(long, default_value_t = 500)]
        warmup: u64,
        #[arg(long)]
        no_pin: bool,
        #[arg(long, default_value = "target/soak")]
        out: PathBuf,
    },
    /// Measure two server builds against each other, alternating between them.
    Compare {
        /// The server binaries to compare.
        #[arg(required = true, num_args = 2..)]
        servers: Vec<PathBuf>,
        #[arg(long, default_value_t = 500)]
        rate: u64,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value_t = 12000)]
        count: u64,
        #[arg(long, default_value_t = 96)]
        payload: usize,
        #[arg(long, default_value_t = 3)]
        reps: u32,
        #[arg(long, default_value_t = 90)]
        limit: u64,
        #[arg(long)]
        no_pin: bool,
        #[arg(long, default_value = "target/bench-compare")]
        out: PathBuf,
    },
    /// Read the `ROW` lines a run accumulated and write `results.json` and `RESULTS.md`.
    Report {
        #[arg(long)]
        rows: PathBuf,
        #[arg(long)]
        json: PathBuf,
        #[arg(long)]
        markdown: PathBuf,
        #[arg(long, default_value_t = 500)]
        rate: u64,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value_t = 500)]
        warmup: u64,
        #[arg(long, default_value_t = 3)]
        reps: u32,
    },
}

fn main() -> std::io::Result<()> {
    match Cli::parse().command {
        Command::ListCases => {
            for case in catalog::CASES {
                println!(
                    "{}\t{}\t{}",
                    case.name,
                    case.group,
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
            version,
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
            let id = RowId::new(
                &case,
                &implementation,
                &version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            );
            probes::run_delivery(&id, &role, &host, payload, rate, count, samples)
        }
        Command::Row {
            samples,
            case,
            implementation,
            payload,
            version,
        } => {
            if catalog::find(&case).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{case} is not in the catalog"),
                ));
            }
            harness::report_samples(
                &samples,
                &RowId::new(&case, &implementation, &version),
                payload,
            )
        }
        Command::Sweep {
            rate,
            samples,
            warmup,
            count,
            payloads,
            reps,
            limit,
            sub_settle,
            cases,
            no_pin,
            only_report,
            rows,
            json,
            markdown,
        } => run::sweep(&run::Settings {
            rate_hz: rate,
            samples,
            warmup,
            count,
            payloads,
            reps,
            limit: std::time::Duration::from_secs(limit),
            sub_settle: std::time::Duration::from_secs(sub_settle),
            cases,
            pin: !no_pin,
            only_report,
            rows_dir: rows,
            json,
            markdown,
        }),
        Command::Soak {
            duration,
            window,
            rate,
            payload,
            warmup,
            no_pin,
            out,
        } => run::soak(
            &run::Settings {
                rate_hz: rate,
                samples: 0,
                warmup,
                count: 0,
                payloads: vec![payload],
                reps: 1,
                limit: std::time::Duration::from_secs(duration + 120),
                sub_settle: std::time::Duration::from_secs(1),
                cases: Vec::new(),
                pin: !no_pin,
                only_report: false,
                rows_dir: out,
                json: PathBuf::new(),
                markdown: PathBuf::new(),
            },
            std::time::Duration::from_secs(duration),
            std::time::Duration::from_secs(window),
        ),
        Command::Compare {
            servers,
            rate,
            samples,
            count,
            payload,
            reps,
            limit,
            no_pin,
            out,
        } => run::compare(
            &run::Settings {
                rate_hz: rate,
                samples,
                warmup: 500,
                count,
                payloads: vec![payload],
                reps,
                limit: std::time::Duration::from_secs(limit),
                sub_settle: std::time::Duration::from_secs(1),
                cases: Vec::new(),
                pin: !no_pin,
                only_report: false,
                rows_dir: out,
                json: PathBuf::new(),
                markdown: PathBuf::new(),
            },
            &servers,
        ),
        Command::Report {
            rows,
            json,
            markdown,
            rate,
            samples,
            warmup,
            reps,
        } => {
            let records = report::parse_rows(&rows)?;
            let implementations: std::collections::BTreeSet<String> = records
                .iter()
                .map(|r| format!("{}={}", r.implementation, r.implementation_version))
                .collect();
            let conditions = report::Conditions::from_machine(
                rate,
                samples,
                warmup,
                reps,
                implementations.into_iter().collect(),
            );
            report::write_json(&json, &conditions, &records)?;
            report::write_markdown(&markdown, &conditions, &records)?;
            report::check_achieved_rate(&conditions, &records)
        }
    }
}
