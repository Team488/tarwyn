//! Running every case the catalog declares.

use super::env::Env;
use super::plan::{Probe, plan, probe_command, server_command};
use super::process::{Cores, spawn, wait_for_marker, wait_for_port, wait_with_limit};
use super::{Settings, noise_check};
use crate::harness::{RowId, row_from_samples};
use crate::{catalog, report};
use std::io::{self, Write as _};
use std::time::{Duration, Instant};

/// Run one case for one implementation at one payload, returning its `ROW`.
///
/// The publisher starts once the subscriber says it is waiting.
fn run_case(
    env: &Env,
    settings: &Settings,
    cores: &Cores,
    case: &str,
    implementation: &str,
    payload: usize,
    rep: u32,
) -> io::Result<Option<String>> {
    let plan = plan(case, implementation);
    let stem = format!("{case}_{implementation}_{payload}_r{rep}");
    let out = settings.rows_dir.join(format!("{stem}.out"));
    let server_log = settings.rows_dir.join(format!("{stem}.log"));
    let pub_log = settings.rows_dir.join(format!("{stem}_pub.log"));

    let server_core = cores.server();
    let (pub_core, sub_core) = cores.probe(plan.probe.pinnable());

    let Some((program, args)) = server_command(env, plan.server, plan.port, settings.rate_hz)
    else {
        return Ok(None);
    };
    let mut server = spawn(&program, &args, server_core, &server_log, settings)?;
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
        plan.port,
        plan.server,
    ) else {
        return Ok(None);
    };
    let mut subscriber = spawn(&program, &args, sub_core, &out, settings)?;

    let ready = Instant::now() + Duration::from_secs(30);
    wait_for_marker(&out, "waiting for", ready);
    std::thread::sleep(settings.sub_settle);

    if let Some((program, args)) = probe_command(
        env,
        settings,
        plan.probe,
        "publisher",
        case,
        implementation,
        payload,
        plan.port,
        plan.server,
    ) {
        let mut publisher = spawn(&program, &args, pub_core, &pub_log, settings)?;
        if let Some(child) = publisher.take().as_mut() {
            wait_with_limit(child, settings.limit)?;
        }
    }

    if let Some(child) = subscriber.take().as_mut() {
        wait_with_limit(child, settings.limit)?;
    }
    drop(server.take().map(|mut c| {
        let _ = c.kill();
        c.wait()
    }));

    let id = RowId::new(case, implementation, &env.version_of(implementation));
    Ok(match plan.probe {
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
    })
}

/// Sweep every case the catalog declares, at every payload, for every rep.
///
/// # Errors
///
/// Returns any error from spawning a process, reading a row file, or writing
/// the report, and an error if any case reported nothing on both of its
/// attempts.
pub fn sweep(settings: &Settings) -> io::Result<()> {
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
            for &payload in &settings.payloads {
                for case in catalog::CASES {
                    if !wanted(case.name) {
                        continue;
                    }
                    for implementation in case.implementations {
                        eprintln!(
                            "rep {rep} payload {payload}B: {}/{implementation}",
                            case.name
                        );
                        let mut captured = None;
                        for _ in 0..2 {
                            captured = run_case(
                                &env,
                                settings,
                                &cores,
                                case.name,
                                implementation,
                                payload,
                                rep,
                            )?;
                            if captured.is_some() {
                                break;
                            }
                            eprintln!(
                                "  {}/{implementation} reported nothing, retrying",
                                case.name
                            );
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

    let records = report::parse_rows(&rows_path)?;
    let implementations: std::collections::BTreeSet<String> = records
        .iter()
        .map(|r| format!("{}={}", r.implementation, r.implementation_version))
        .collect();
    let conditions = report::Conditions::from_machine(
        settings.rate_hz,
        settings.samples,
        settings.warmup,
        settings.reps,
        implementations.into_iter().collect(),
    );
    if let Some(parent) = settings.json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    report::write_json(&settings.json, &conditions, &records)?;
    report::write_markdown(&settings.markdown, &conditions, &records)?;
    eprintln!("updated {}", settings.markdown.display());

    report::check_achieved_rate(&conditions, &records)?;
    if !failed.is_empty() {
        return Err(io::Error::other(format!(
            "failed every attempt: {}",
            failed.join(", ")
        )));
    }
    Ok(())
}
