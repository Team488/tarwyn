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

            let row = std::fs::read_to_string(&out)?
                .lines()
                .find_map(compare_row)
                .filter(|(samples, _)| *samples as f64 >= settings.samples as f64 * 0.9)
                .map(|(_, stats)| stats);
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

/// The sample count and the median, p99, p99.9 and max a `ROW` line
/// carries. `None` for any other line.
fn compare_row(line: &str) -> Option<(u64, [f64; 4])> {
    let fields: Vec<&str> = line.strip_prefix("ROW\t")?.split('\t').collect();
    let at = |i: usize| fields.get(i)?.trim().parse::<f64>().ok();
    let samples = fields.get(14)?.trim().parse().ok()?;
    Some((samples, [at(5)?, at(10)?, at(11)?, at(12)?]))
}

/// Replay `profile` at `multiplier` against each server build, alternating,
/// and print each run's per-class latency and verdict.
///
/// # Errors
///
/// Returns an error when a step cannot run.
pub fn compare_profile(
    settings: &Settings,
    servers: &[PathBuf],
    profile: &Path,
    multiplier: u32,
) -> anyhow::Result<()> {
    use crate::sim::sweep::{Budget, SimSettings, Step, run_step};
    let env = Env::discover()?;
    std::fs::create_dir_all(&settings.rows_dir)?;
    let cores = Cores::pick(settings.pin);
    let sim = SimSettings {
        profile: std::path::absolute(profile)?,
        axes: Vec::new(),
        implementations: vec!["tarwyn".into()],
        budgets: vec![Budget::None],
        reps: settings.reps,
        warmup: 5.0,
        seconds: 30.0,
        max_multiplier: multiplier,
        clients: Vec::new(),
        probes: 5,
        logs: settings.rows_dir.clone(),
        pin: settings.pin,
    };
    for rep in 1..=settings.reps {
        for (index, server) in servers.iter().enumerate() {
            let step = Step {
                axis: format!("compare{index}"),
                implementation: "tarwyn".into(),
                budget: Budget::None,
                multiplier,
                extra: 0,
                rep,
            };
            let result = run_step(&env, &sim, &cores, &step, Some(server))?;
            let classes: Vec<String> = result
                .classes
                .iter()
                .map(|(class, stats)| format!("{class} {:.1}/{:.1} us", stats.p50_us, stats.p99_us))
                .collect();
            println!(
                "rep{rep:<3} {:<32} {} cpu={:.1}% {}",
                name_of(server),
                classes.join("  "),
                result.cpu_pct.unwrap_or(0.0),
                result.verdict.text()
            );
        }
    }
    Ok(())
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::compare_row;

    #[test]
    fn compare_reads_the_median_and_samples_where_the_row_puts_them() {
        let row = "ROW\tpublish\ttarwyn\t0.1.1\t96\t500\t25.50\t10.00\t30.00\t31.00\t32.00\t46.00\t60.00\t90.00\t0.00\t3000\t500.0";
        assert_eq!(compare_row(row), Some((3000, [25.5, 46.0, 60.0, 90.0])));
    }

    #[test]
    fn a_line_that_is_not_a_row_is_skipped() {
        assert_eq!(compare_row("sent 1000 messages"), None);
    }
}
