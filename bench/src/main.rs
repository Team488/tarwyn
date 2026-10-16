//! Latency harness for tarwyn and the alternatives it is measured against.
//! See `bench/BENCHMARK.md`.
//!
//! Harnesses in other languages only stamp clocks. `bench row` turns their
//! lines into the same `ROW` this binary prints.

#![forbid(unsafe_code)]

mod catalog;

mod harness;

mod report;

mod run;

mod probes;

mod sim;

use clap::{Parser, Subcommand};
use harness::RowId;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Latency harness for tarwyn and its alternatives")]
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
        /// The measured implementation's version, which defaults to this crate's.
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
        /// The rate the publisher was paced at.
        #[arg(long)]
        rate: u64,
        #[arg(long)]
        version: String,
    },
    /// Run every case the catalog declares, at every rate and payload, for
    /// every rep.
    Sweep {
        /// Publish rates in Hz: 50 is a robot's main loop, and 500 a vision
        /// coprocessor.
        #[arg(long, value_delimiter = ' ', default_values_t = [50u64, 500])]
        rates: Vec<u64>,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value_t = 500)]
        warmup: u64,
        /// Messages the publisher sends. Defaults to samples plus warmup plus
        /// a thousand.
        #[arg(long)]
        count: Option<u64>,
        /// Wire sizes in bytes: a scalar, a pose, a pose array.
        #[arg(long, value_delimiter = ' ', default_values_t = [16usize, 96, 1024])]
        payloads: Vec<usize>,
        #[arg(long, default_value_t = 3)]
        reps: u32,
        /// Seconds before a probe is killed.
        #[arg(long, default_value_t = 90)]
        limit: u64,
        /// Seconds to wait after a subscriber says it is ready.
        #[arg(long, default_value_t = 2)]
        sub_settle: u64,
        /// Which cases to run. All of them when empty.
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
        /// The machine-readable record, kept beside RESULTS.md.
        #[arg(long, default_value = "bench/results.json")]
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
        /// Messages the publisher sends. Defaults to samples plus warmup plus
        /// a thousand.
        #[arg(long)]
        count: Option<u64>,
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
        /// Replay this `bench sim` profile instead of the one-topic probe.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// The rate multiplier the profile is replayed at.
        #[arg(long, default_value_t = 1)]
        multiplier: u32,
    },
    /// Replay a robot's NetworkTables load against each implementation, scaled
    /// until it breaks. See `bench/BENCHMARK.md`.
    Sim {
        #[arg(long, default_value = "bench/workloads/team488-2026.json")]
        profile: PathBuf,
        /// `rate`, `clients` or both.
        #[arg(long, value_delimiter = ' ', default_values_t = ["rate".to_string(), "clients".to_string()])]
        axes: Vec<String>,
        #[arg(long = "impls", value_delimiter = ' ', default_values_t = ["tarwyn".to_string(), "ntcore-2026".to_string(), "ntcore-2027".to_string()])]
        implementations: Vec<String>,
        /// CPU budgets the rate axis runs under: `none`, `1core`, `25pct`.
        #[arg(long, value_delimiter = ' ', default_values_t = ["none".to_string(), "1core".to_string(), "25pct".to_string()])]
        budgets: Vec<String>,
        #[arg(long, default_value_t = 3)]
        reps: u32,
        #[arg(long, default_value_t = 5.0)]
        warmup: f64,
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
        #[arg(long, default_value_t = 64)]
        max_multiplier: u32,
        /// Extra dashboards the clients axis adds.
        #[arg(long, value_delimiter = ' ', default_values_t = [0usize, 2, 4, 8, 16])]
        clients: Vec<usize>,
        #[arg(long, default_value_t = 5)]
        probes: u32,
        #[arg(long)]
        no_pin: bool,
        #[arg(long, default_value = "target/bench-sim")]
        logs: PathBuf,
        #[arg(long, default_value = "bench/results.json")]
        json: PathBuf,
        #[arg(long, default_value = "bench/RESULTS.md")]
        markdown: PathBuf,
    },
    /// Run one role of a `bench sim` step. `bench sim` starts these itself.
    #[command(hide = true)]
    SimRole {
        /// `robot`, `coprocessors` or `dashboards`.
        #[arg(long)]
        role: String,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long, default_value = "127.0.0.1:5810")]
        host: String,
        #[arg(long, default_value_t = 1)]
        multiplier: u32,
        #[arg(long, default_value_t = 5.0)]
        warmup: f64,
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
        #[arg(long, default_value_t = 0)]
        extra: usize,
        #[arg(long, default_value_t = 5)]
        probes: u32,
        /// When the warmup starts, in milliseconds since the Unix epoch.
        #[arg(long)]
        opens_at: Option<u64>,
    },
    /// Make or inspect a `bench sim` profile.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Read the `ROW` lines a run accumulated and write `results.json` and `RESULTS.md`.
    Report {
        #[arg(long)]
        rows: PathBuf,
        #[arg(long)]
        json: PathBuf,
        #[arg(long)]
        markdown: PathBuf,
        #[arg(long, value_delimiter = ' ', default_values_t = [50u64, 500])]
        rates: Vec<u64>,
        #[arg(long, default_value_t = 3000)]
        samples: u64,
        #[arg(long, default_value_t = 500)]
        warmup: u64,
        #[arg(long, default_value_t = 3)]
        reps: u32,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Turn a `capture.py` capture and the dashboards' layouts into a profile.
    FromCapture {
        /// The JSON `bench/python/src/capture.py` wrote.
        capture: PathBuf,
        /// An Elastic layout, whose widgets become the `elastic` dashboard.
        #[arg(long)]
        elastic: Option<PathBuf>,
        /// An AdvantageScope layout, whose fields become the `advantagescope` dashboard.
        #[arg(long)]
        advantagescope: Option<PathBuf>,
        /// The rate every camera publishes at.
        #[arg(long, default_value_t = 60.0)]
        camera_fps: f64,
        #[arg(long)]
        name: String,
        /// Recorded as the profile's source, beside the capture's path.
        #[arg(long, default_value = "")]
        source_commit: String,
        /// How the robot was running when it was captured.
        #[arg(long, default_value = "")]
        source_mode: String,
        #[arg(long)]
        out: PathBuf,
    },
}

/// Read a JSON file, naming it in the error.
fn read_json(path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    use anyhow::Context as _;
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    serde_json::from_str(&text).with_context(|| path.display().to_string())
}

fn main() -> anyhow::Result<()> {
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
                return Err(anyhow::anyhow!("{case} is not in the catalog"));
            };
            if !declared.implementations.contains(&implementation.as_str()) {
                return Err(anyhow::anyhow!("{case} does not declare {implementation}"));
            }
            let id = RowId::new(
                &case,
                &implementation,
                &version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
                rate,
            );
            probes::run_delivery(&id, &role, &host, payload, rate, count, samples)
        }
        Command::Row {
            samples,
            case,
            implementation,
            payload,
            rate,
            version,
        } => {
            if catalog::find(&case).is_none() {
                return Err(anyhow::anyhow!("{case} is not in the catalog"));
            }
            harness::report_samples(
                &samples,
                &RowId::new(&case, &implementation, &version, rate),
                payload,
            )
        }
        Command::Sweep {
            rates,
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
            rate_hz: rates.first().copied().unwrap_or(500),
            rates,
            samples,
            warmup,
            count: count.unwrap_or(samples + warmup + 1000),
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
                rates: vec![rate],
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
            profile,
            multiplier,
        } => {
            let settings = run::Settings {
                rate_hz: rate,
                rates: vec![rate],
                samples,
                warmup: 500,
                count: count.unwrap_or(samples + 500 + 1000),
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
            };
            match profile {
                Some(profile) => run::compare_profile(&settings, &servers, &profile, multiplier),
                None => run::compare(&settings, &servers),
            }
        }
        Command::Sim {
            profile,
            axes,
            implementations,
            budgets,
            reps,
            warmup,
            seconds,
            max_multiplier,
            clients,
            probes,
            no_pin,
            logs,
            json,
            markdown,
        } => {
            let settings = sim::sweep::SimSettings {
                profile: std::path::absolute(&profile)?,
                axes,
                implementations,
                budgets: budgets
                    .iter()
                    .map(|b| b.parse())
                    .collect::<anyhow::Result<_>>()?,
                reps,
                warmup,
                seconds,
                max_multiplier,
                clients,
                probes,
                logs,
                pin: !no_pin,
            };
            let (results, headroom) = sim::sweep::sim(&settings)?;
            let env = run::env::Env::discover()?;
            let machine = report::Conditions::from_machine(
                Vec::new(),
                0,
                0,
                settings.reps,
                Vec::new(),
                String::new(),
            );
            let profile = sim::profile::Profile::load(&settings.profile)?;
            let meta = sim::report::Meta {
                profile: profile.name,
                source: profile.source,
                warmup: settings.warmup,
                seconds: settings.seconds,
                reps: settings.reps,
                max_multiplier: settings.max_multiplier,
                versions: settings
                    .implementations
                    .iter()
                    .map(|name| (name.clone(), env.version_of(name)))
                    .collect(),
                machine: format!("{}, {} logical cpus", machine.cpu, machine.cores),
                date: machine.date,
                commit: machine.commit,
            };
            sim::report::write(&json, &markdown, &meta, &results, &headroom)?;
            eprintln!("updated {}", markdown.display());
            Ok(())
        }
        Command::SimRole {
            role,
            profile,
            host,
            multiplier,
            warmup,
            seconds,
            extra,
            probes,
            opens_at,
        } => {
            let args = sim::RoleArgs {
                profile,
                host,
                multiplier,
                warmup,
                seconds,
                extra,
                probes,
                opens_at_ms: opens_at,
            };
            match role.as_str() {
                "robot" => sim::roles::run_robot(&args),
                "coprocessors" => sim::roles::run_coprocessors(&args),
                "dashboards" => sim::dashboard::run(&args),
                other => Err(anyhow::anyhow!("no sim role is called {other}")),
            }
        }
        Command::Profile {
            command:
                ProfileCommand::FromCapture {
                    capture,
                    elastic,
                    advantagescope,
                    camera_fps,
                    name,
                    source_commit,
                    source_mode,
                    out,
                },
        } => {
            let layouts = sim::capture::Layouts {
                elastic: elastic.as_deref().map(read_json).transpose()?,
                advantagescope: advantagescope.as_deref().map(read_json).transpose()?,
            };
            let options = sim::capture::CaptureOptions {
                name,
                camera_fps,
                source: serde_json::json!({
                    "kind": "sim-capture",
                    "file": capture.file_name().map(|f| f.to_string_lossy().to_string()),
                    "commit": source_commit,
                    "mode": source_mode,
                    "camera_fps": camera_fps,
                }),
            };
            let profile = sim::capture::from_capture(&read_json(&capture)?, &layouts, &options)?;
            std::fs::write(&out, sim::capture::to_lines(&profile)?)?;
            eprintln!(
                "wrote {} with {} topics across {} clients",
                out.display(),
                profile.publishes().count(),
                profile.clients.len()
            );
            Ok(())
        }
        Command::Report {
            rows,
            json,
            markdown,
            rates,
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
                rates,
                samples,
                warmup,
                reps,
                implementations.into_iter().collect(),
                "unknown".to_string(),
            );
            report::write_json(&json, &conditions, &records)?;
            report::write_markdown(&markdown, &conditions, &records)?;
            report::check_achieved_rate(&records)
        }
    }
}
