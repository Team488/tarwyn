//! Measuring two server builds against each other.

use super::Settings;
use super::env::Env;
use super::process::{Cores, spawn, wait_for_marker, wait_for_port, wait_with_limit};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Measure two server builds against each other, alternating between them so
/// that drift on a noisy machine lands on both rather than on whichever ran
/// last. A run that came up short of its sample budget measured something
/// other than what it was asked to and is dropped rather than averaged in.
///
/// # Errors
///
/// Returns any error from spawning a process, and an error if no build
/// produced a measurement at all.
pub fn compare(settings: &Settings, servers: &[PathBuf]) -> io::Result<()> {
    let env = Env::discover()?;
    std::fs::create_dir_all(&settings.rows_dir)?;
    let cores = Cores::pick(settings.pin);
    let server_core = cores.server();
    let (pub_core, sub_core) = cores.probe(true);
    let payload = settings.payloads.first().copied().unwrap_or(96);
    let mut measurements: Vec<(usize, f64)> = Vec::new();

    for rep in 1..=settings.reps {
        for (index, server) in servers.iter().enumerate() {
            let stem = format!("compare_{index}_r{rep}");
            let out = settings.rows_dir.join(format!("{stem}.out"));
            let mut running = spawn(
                &server.display().to_string(),
                &[],
                server_core,
                &settings.rows_dir.join(format!("{stem}_server.log")),
                settings,
            )?;
            if !wait_for_port(5810, Instant::now() + Duration::from_secs(20)) {
                eprintln!("rep{rep} {}: never listened", server.display());
                continue;
            }
            let side = |role: &str, extra: Vec<String>| {
                let mut args = vec![
                    "run".to_string(),
                    "--case".into(),
                    "publish".into(),
                    "--impl".into(),
                    "tarwyn".into(),
                    "--role".into(),
                    role.into(),
                    "--payload".into(),
                    payload.to_string(),
                ];
                args.extend(extra);
                args
            };
            let mut subscriber = spawn(
                &env.exe.display().to_string(),
                &side(
                    "subscriber",
                    vec!["--samples".into(), settings.samples.to_string()],
                ),
                sub_core,
                &out,
                settings,
            )?;
            wait_for_marker(
                &out,
                "waiting for",
                Instant::now() + Duration::from_secs(30),
            );
            let mut publisher = spawn(
                &env.exe.display().to_string(),
                &side(
                    "publisher",
                    vec![
                        "--rate".into(),
                        settings.rate_hz.to_string(),
                        "--count".into(),
                        settings.count.to_string(),
                    ],
                ),
                pub_core,
                &settings.rows_dir.join(format!("{stem}_pub.log")),
                settings,
            )?;
            if let Some(child) = publisher.take().as_mut() {
                wait_with_limit(child, settings.limit)?;
            }
            if let Some(child) = subscriber.take().as_mut() {
                wait_with_limit(child, settings.limit)?;
            }
            drop(running.take());

            let median = std::fs::read_to_string(&out)?.lines().find_map(|line| {
                let fields: Vec<&str> = line.strip_prefix("ROW\t")?.split('\t').collect();
                let samples: u64 = fields.get(13)?.parse().ok()?;
                (samples as f64 >= settings.samples as f64 * 0.9)
                    .then(|| fields.get(4)?.parse::<f64>().ok())
                    .flatten()
            });
            match median {
                Some(median) => {
                    println!("rep{rep:<3} {:<40} median={median:.2}", name_of(server));
                    measurements.push((index, median));
                }
                None => println!("rep{rep:<3} {:<40} median=none", name_of(server)),
            }
        }
    }

    println!();
    let mut measured = false;
    for (index, server) in servers.iter().enumerate() {
        let mut values: Vec<f64> = measurements
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, v)| *v)
            .collect();
        if values.is_empty() {
            println!("  {:<40} no measurements", name_of(server));
            continue;
        }
        measured = true;
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let spread = if values[0] > 0.0 {
            100.0 * (values[values.len() - 1] - values[0]) / values[0]
        } else {
            0.0
        };
        println!(
            "  {:<40} median of {} runs = {:.2} us, spread {spread:.1}%",
            name_of(server),
            values.len(),
            values[(values.len() - 1) / 2]
        );
    }
    if !measured {
        return Err(io::Error::other("no build produced a measurement"));
    }
    Ok(())
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}
