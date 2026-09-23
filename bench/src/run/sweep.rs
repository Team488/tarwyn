//! Running every case the catalog declares.

use super::env::Env;
use super::plan::{Probe, plan, probe_command, server_command};
use super::process::{Cores, cpu_seconds, spawn, wait_for_marker, wait_for_port, wait_with_limit};
use super::{Settings, noise_check};
use crate::harness::{RowId, row_from_samples};
use crate::{catalog, report};
use std::io::Write as _;
use std::time::{Duration, Instant};

/// One cell of the sweep: case, implementation, payload, rate and rep.
struct Cell<'a> {
    case: &'a str,
    implementation: &'a str,
    payload: usize,
    rate_hz: u64,
    rep: u32,
}

/// Run one cell and return its `ROW`, with a `CPU` line when measured. The
/// publisher starts once the subscriber waits, and is killed once it reports.
fn run_case(
    env: &Env,
    settings: &Settings,
    cores: &Cores,
    cell: &Cell<'_>,
) -> anyhow::Result<Option<String>> {
    let Cell {
        case,
        implementation,
        payload,
        rate_hz,
        rep,
    } = *cell;
    let plan = plan(case, implementation);
    let stem = format!("{case}_{implementation}_{payload}_{rate_hz}hz_r{rep}");
    let out = settings.rows_dir.join(format!("{stem}.out"));
    let server_log = settings.rows_dir.join(format!("{stem}.log"));
    let pub_log = settings.rows_dir.join(format!("{stem}_pub.log"));

    let server_core = cores.server();
    let (pub_core, sub_core) = cores.probe(plan.probe.pinnable());
    // Fan-out runs a receive thread per subscriber, which one core would throttle.
    let sub_core = if case == "fanout" { None } else { sub_core };

    let Some((program, args)) = server_command(env, plan.server, plan.port) else {
        return Ok(None);
    };
    let server = spawn(&program, &args, server_core, &server_log, settings)?;
    if !wait_for_port(plan.port, Instant::now() + Duration::from_secs(20)) {
        return Ok(None);
    }
    std::thread::sleep(plan.settle);

    let Some((program, args)) = probe_command(
        env,
        settings,
        plan.probe,
        "subscriber",
        case,
        implementation,
        payload,
        rate_hz,
        plan.port,
        plan.server,
    ) else {
        return Ok(None);
    };
    let mut subscriber = spawn(&program, &args, sub_core, &out, settings)?;

    let ready = Instant::now() + Duration::from_secs(30);
    wait_for_marker(&out, "waiting for", ready);
    std::thread::sleep(settings.sub_settle);
    let cpu_before = server.id().and_then(cpu_seconds);
    let measuring_from = Instant::now();

    let publisher = probe_command(
        env,
        settings,
        plan.probe,
        "publisher",
        case,
        implementation,
        payload,
        rate_hz,
        plan.port,
        plan.server,
    )
    .map(|(program, args)| spawn(&program, &args, pub_core, &pub_log, settings))
    .transpose()?;

    if let Some(child) = subscriber.take().as_mut() {
        wait_with_limit(child, settings.limit)?;
    }
    let cpu_after = server.id().and_then(cpu_seconds);
    let elapsed = measuring_from.elapsed().as_secs_f64();
    drop(publisher);
    drop(server);
    let server_cpu_pct = match (cpu_before, cpu_after) {
        (Some(before), Some(after)) if elapsed > 0.0 => Some(100.0 * (after - before) / elapsed),
        _ => None,
    };

    let id = RowId::new(
        case,
        implementation,
        &env.version_of(implementation),
        rate_hz,
    );
    let row = match plan.probe {
        Probe::Rust => std::fs::read_to_string(&out)?
            .lines()
            .find(|line| line.starts_with("ROW"))
            .map(str::to_string),
        _ => match row_from_samples(&out, &id, payload, Some(settings.warmup)) {
            Ok(row) => Some(row),
            Err(error) => {
                eprintln!("  {case}/{implementation}: {error}");
                None
            }
        },
    };
    if let Some(row) = &row {
        let samples: u64 = row
            .split('\t')
            .nth(15)
            .and_then(|field| field.trim().parse().ok())
            .unwrap_or(0);
        if samples < settings.samples {
            eprintln!(
                "  {case}/{implementation}: {samples} of {} samples, the row is short",
                settings.samples
            );
        }
    }
    Ok(row.map(|row| match server_cpu_pct {
        Some(pct) => {
            format!("{row}\nCPU\t{case}\t{implementation}\t{payload}\t{rate_hz}\t{pct:.1}")
        }
        None => row,
    }))
}

/// Sweep every case at every rate and payload, for every rep.
///
/// # Errors
///
/// Returns an error when a process fails to spawn, a file cannot be read or
/// written, or a case reports nothing on both of its attempts.
pub fn sweep(settings: &Settings) -> anyhow::Result<()> {
    let env = Env::discover()?;
    let rows_path = settings.rows_dir.join("all.tsv");
    std::fs::create_dir_all(&settings.rows_dir)?;
    let cores = Cores::pick(settings.pin);
    let wanted = |case: &str| settings.cases.is_empty() || settings.cases.iter().any(|c| c == case);
    let mut failed = Vec::new();

    if !settings.only_report {
        noise_check();
        let mut rows = std::fs::File::create(&rows_path)?;
        for rep in 1..=settings.reps {
            for &rate_hz in &settings.rates {
                for &payload in &settings.payloads {
                    for case in catalog::CASES {
                        if !wanted(case.name) {
                            continue;
                        }
                        for implementation in case.implementations {
                            eprintln!(
                                "rep {rep} {rate_hz} Hz {payload} B: {}/{implementation}",
                                case.name
                            );
                            let mut captured = None;
                            for attempt in 0..2 {
                                captured = run_case(
                                    &env,
                                    settings,
                                    &cores,
                                    &Cell {
                                        case: case.name,
                                        implementation,
                                        payload,
                                        rate_hz,
                                        rep,
                                    },
                                )?;
                                if captured.is_some() {
                                    break;
                                }
                                eprintln!(
                                    "  {}/{implementation} reported nothing, retrying",
                                    case.name
                                );
                                if attempt == 0 {
                                    writeln!(
                                        rows,
                                        "RETRY\t{}\t{implementation}\t{payload}\t{rate_hz}",
                                        case.name
                                    )?;
                                }
                            }
                            match captured {
                                Some(row) => writeln!(rows, "{row}")?,
                                None => failed.push(format!("{}/{implementation}", case.name)),
                            }
                        }
                    }
                }
            }
        }
    }

    let records = report::parse_rows(&rows_path)?;
    let implementations: std::collections::BTreeSet<String> = records
        .iter()
        .map(|r| format!("{}={}", r.implementation, r.implementation_version))
        .collect();
    let conditions = report::Conditions::from_machine(
        settings.rates.clone(),
        settings.samples,
        settings.warmup,
        settings.reps,
        implementations.into_iter().collect(),
        cores.describe(),
    );
    if let Some(parent) = settings.json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    report::write_json(&settings.json, &conditions, &records)?;
    report::write_markdown(&settings.markdown, &conditions, &records)?;
    eprintln!("updated {}", settings.markdown.display());

    report::check_achieved_rate(&records)?;
    if !failed.is_empty() {
        return Err(anyhow::anyhow!(
            "failed every attempt: {}",
            failed.join(", ")
        ));
    }
    Ok(())
}
