//! Measuring two server builds against each other.

use super::Settings;
use super::env::Env;
use super::process::{Cores, spawn, wait_for_marker, wait_for_port, wait_with_limit};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Measure two server builds against each other, alternating so drift lands
/// on both. Short runs are dropped, and each stat takes the middle rep.
///
/// # Errors
///
/// Returns an error when a process fails to spawn or no build produces a
/// measurement.
pub fn compare(settings: &Settings, servers: &[PathBuf]) -> anyhow::Result<()> {
    let env = Env::discover()?;
    std::fs::create_dir_all(&settings.rows_dir)?;
    let cores = Cores::pick(settings.pin);
    let server_core = cores.server();
    let (pub_core, sub_core) = cores.probe(true);
    let payload = settings.payloads.first().copied().unwrap_or(96);
    let mut measurements: Vec<(usize, [f64; 4])> = Vec::new();

    for rep in 1..=settings.reps {
        for (index, server) in servers.iter().enumerate() {
            let stem = format!("compare_{index}_r{rep}");
            let out = settings.rows_dir.join(format!("{stem}.out"));
            let running = spawn(
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
            let publisher = spawn(
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
            if let Some(child) = subscriber.take().as_mut() {
                wait_with_limit(child, settings.limit)?;
            }
            drop(publisher);
            drop(running);

            let row = std::fs::read_to_string(&out)?.lines().find_map(|line| {
                let fields: Vec<&str> = line.strip_prefix("ROW\t")?.split('\t').collect();
                let samples: u64 = fields.get(13)?.parse().ok()?;
                if (samples as f64) < settings.samples as f64 * 0.9 {
                    return None;
                }
                let at = |i: usize| fields.get(i)?.parse::<f64>().ok();
                Some([at(4)?, at(9)?, at(10)?, at(11)?])
            });
            match row {
                Some(row) => {
                    println!(
                        "rep{rep:<3} {:<32} median={:.2} p99={:.2} p99.9={:.2} max={:.2}",
                        name_of(server),
                        row[0],
                        row[1],
                        row[2],
                        row[3]
                    );
                    measurements.push((index, row));
                }
                None => println!("rep{rep:<3} {:<32} no full row", name_of(server)),
            }
        }
    }

    println!();
    let mut measured = false;
    for (index, server) in servers.iter().enumerate() {
        let rows: Vec<[f64; 4]> = measurements
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, v)| *v)
            .collect();
        if rows.is_empty() {
            println!("  {:<32} no measurements", name_of(server));
            continue;
        }
        measured = true;
        let middle = |column: usize| {
            let mut values: Vec<f64> = rows.iter().map(|row| row[column]).collect();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            values[(values.len() - 1) / 2]
        };
        let mut medians: Vec<f64> = rows.iter().map(|row| row[0]).collect();
        medians.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let spread = if medians[0] > 0.0 {
            100.0 * (medians[medians.len() - 1] - medians[0]) / medians[0]
        } else {
            0.0
        };
        println!(
            "  {:<32} over {} runs: median={:.2} (spread {spread:.1}%) p99={:.2} p99.9={:.2} max={:.2}",
            name_of(server),
            rows.len(),
            middle(0),
            middle(1),
            middle(2),
            rows.iter().map(|row| row[3]).fold(0.0, f64::max)
        );
    }
    if !measured {
        return Err(anyhow::anyhow!("no build produced a measurement"));
    }
    Ok(())
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}
