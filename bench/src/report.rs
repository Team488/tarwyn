//! The record of one benchmark run, as JSON and as markdown.
//!
//! JSON is what a program reads to answer whether a commit regressed anything.
//! The markdown is generated from the same records, so the report can never
//! disagree with the record it came from.

use crate::catalog;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

/// How many fields a `ROW` line has, including the leading `ROW`.
const ROW_FIELDS: usize = 16;

/// What the machine was doing while the run happened.
#[derive(Debug, Clone)]
pub struct Conditions {
    /// The operating system the run happened on.
    pub os: String,
    /// Kernel release, where the platform reports one.
    pub kernel: String,
    pub cpu: String,
    pub governor: String,
    pub boost: bool,
    /// The one-minute load average when the run started.
    pub loadavg: f64,
    /// The commit the run measured, when the tree was a git checkout.
    pub commit: String,
    pub rate_hz: u64,
    pub samples: u64,
    /// Samples discarded before recording.
    pub warmup: u64,
    pub reps: u32,
    /// The version of every implementation measured, as `name=version`.
    pub implementations: Vec<String>,
}

impl Conditions {
    /// Conditions with every field filled, for tests and examples.
    #[cfg(test)]
    pub fn sample() -> Self {
        Conditions {
            os: "linux".to_string(),
            kernel: "7.2.3-arch1-2".to_string(),
            cpu: "AMD Ryzen 5 5600X".to_string(),
            governor: "powersave".to_string(),
            boost: true,
            loadavg: 0.54,
            commit: "abc1234".to_string(),
            rate_hz: 500,
            samples: 3000,
            warmup: 500,
            reps: 3,
            implementations: vec!["tarwyn-rust=0.1.0".to_string()],
        }
    }

    /// Read what can be read from `/proc` and `/sys`; anything unreadable
    /// becomes `unknown` (or `false`/`0.0`) rather than failing the run.
    pub fn from_machine(
        rate_hz: u64,
        samples: u64,
        warmup: u64,
        reps: u32,
        implementations: Vec<String>,
    ) -> Self {
        Conditions {
            os: std::env::consts::OS.to_string(),
            kernel: read_kernel(),
            cpu: read_cpu_model(),
            governor: std::fs::read_to_string(
                "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
            )
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
            boost: std::fs::read_to_string("/sys/devices/system/cpu/cpufreq/boost")
                .map(|s| s.trim() == "1")
                .unwrap_or(false),
            loadavg: std::fs::read_to_string("/proc/loadavg")
                .ok()
                .and_then(|s| s.split_whitespace().next().map(str::to_string))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            commit: read_commit(),
            rate_hz,
            samples,
            warmup,
            reps,
            implementations,
        }
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|out| !out.is_empty())
}

/// The OS release a reader would recognise: the marketed version on macOS and
/// Windows, the kernel release elsewhere.
fn read_kernel() -> String {
    let release = match std::env::consts::OS {
        "macos" => command_output("sw_vers", &["-productVersion"]),
        "windows" => command_output("cmd", &["/c", "ver"]).and_then(|v| {
            v.split("[Version ")
                .nth(1)?
                .trim_end_matches(']')
                .trim()
                .to_string()
                .into()
        }),
        _ => command_output("uname", &["-r"]),
    };
    release.unwrap_or_else(|| "unknown".to_string())
}

fn read_commit() -> String {
    command_output("git", &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string())
}

/// The CPU model, from whichever of these the platform has.
///
/// `/proc/cpuinfo` on Linux, `machdep.cpu.brand_string` on macOS, the registry
/// on Windows; anything else reports `unknown` rather than failing the run.
fn read_cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .or_else(|| command_output("sysctl", &["-n", "machdep.cpu.brand_string"]))
        .or_else(|| {
            command_output(
                "reg",
                &[
                    "query",
                    r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
                    "/v",
                    "ProcessorNameString",
                ],
            )
            .and_then(|out| {
                out.lines()
                    .find(|l| l.contains("ProcessorNameString"))?
                    .split("REG_SZ")
                    .nth(1)
                    .map(|s| s.trim().to_string())
            })
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// One measured case, for one implementation, at one payload size.
#[derive(Debug, Clone)]
pub struct Record {
    pub case: String,
    /// The name the report renders, from `catalog::Case::display`.
    pub display: String,
    /// The table this case belongs in.
    pub group: String,
    pub implementation: String,
    /// The measured implementation's version. Never empty: a row that cannot
    /// say what it measured is not comparable with anything.
    pub implementation_version: String,
    pub payload_bytes: usize,
    /// How many runs the median was picked from.
    pub runs: u32,
    pub samples: u64,
    pub p0_us: f64,
    pub median_us: f64,
    pub p80_us: f64,
    pub p90_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub p999_us: f64,
    pub max_us: f64,
    pub loss_pct: f64,
    /// How far the median moved between runs, as a percentage.
    pub spread_pct: f64,
    /// The lowest and highest per-run median, in microseconds.
    pub min_median_us: f64,
    pub max_median_us: f64,
    pub achieved_hz: f64,
}

/// One parsed `ROW` line, before grouping across repeated runs.
#[derive(Debug)]
struct RawRow {
    case: String,
    implementation: String,
    version: String,
    payload_bytes: usize,
    p0_us: f64,
    median_us: f64,
    p80_us: f64,
    p90_us: f64,
    p95_us: f64,
    p99_us: f64,
    p999_us: f64,
    max_us: f64,
    loss_pct: f64,
    samples: u64,
    achieved_hz: f64,
}

/// Parse one `ROW` line's tab-separated fields.
///
/// The schema is fixed at [`ROW_FIELDS`] and every field is required. There is
/// one emitter for it in the whole benchmark, so a row of any other width is a
/// bug rather than a dialect to be tolerated.
///
/// # Errors
///
/// Returns an error naming `line_no` if the line is not exactly [`ROW_FIELDS`]
/// fields or if any field does not parse.
fn parse_row_line(line: &str, line_no: usize) -> std::io::Result<RawRow> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != ROW_FIELDS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "line {line_no}: ROW row has {} fields, expected exactly {ROW_FIELDS}",
                fields.len()
            ),
        ));
    }
    let bad = |what: &str| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("line {line_no}: {what} is not a number"),
        )
    };
    let parse_f64 = |field: &str, what: &str| -> std::io::Result<f64> {
        field.trim().parse::<f64>().map_err(|_| bad(what))
    };
    let version = fields[3].trim();
    if version.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("line {line_no}: the row carries no implementation version"),
        ));
    }
    Ok(RawRow {
        case: fields[1].trim().to_string(),
        implementation: fields[2].trim().to_string(),
        version: version.to_string(),
        payload_bytes: fields[4].parse().map_err(|_| bad("payload_bytes"))?,
        median_us: parse_f64(fields[5], "median")?,
        p0_us: parse_f64(fields[6], "p0")?,
        p80_us: parse_f64(fields[7], "p80")?,
        p90_us: parse_f64(fields[8], "p90")?,
        p95_us: parse_f64(fields[9], "p95")?,
        p99_us: parse_f64(fields[10], "p99")?,
        p999_us: parse_f64(fields[11], "p999")?,
        max_us: parse_f64(fields[12], "p100")?,
        loss_pct: parse_f64(fields[13], "loss")?,
        samples: fields[14].trim().parse().map_err(|_| bad("samples"))?,
        achieved_hz: parse_f64(fields[15], "achieved_hz")?,
    })
}

/// Parse the `ROW` lines a run accumulated into grouped [`Record`]s.
///
/// Lines that do not begin with `ROW` are ignored. Repeated runs of the same
/// case, implementation and payload are folded into one record.
///
/// # Errors
///
/// Returns an error naming the line number of any malformed `ROW` line, and
/// any error reading `path`.
pub fn parse_rows(path: &Path) -> std::io::Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)?;
    let mut groups: BTreeMap<(String, String, usize), Vec<RawRow>> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if !line.starts_with("ROW") {
            continue;
        }
        let row = parse_row_line(line, index + 1)?;
        if catalog::find(&row.case).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: {} is not in the catalog", index + 1, row.case),
            ));
        }
        groups
            .entry((
                row.case.clone(),
                row.implementation.clone(),
                row.payload_bytes,
            ))
            .or_default()
            .push(row);
    }

    let mut records = Vec::with_capacity(groups.len());
    for ((case, implementation, payload_bytes), rows) in groups {
        let declared = catalog::find(&case).expect("checked above");
        records.push(fold(
            case,
            declared.display.to_string(),
            declared.group.to_string(),
            implementation,
            payload_bytes,
            &rows,
        ));
    }
    if records.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}: no ROW records were parsed", path.display()),
        ));
    }
    Ok(records)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

fn fold(
    case: String,
    display: String,
    group: String,
    implementation: String,
    payload_bytes: usize,
    rows: &[RawRow],
) -> Record {
    let runs = rows.len() as u32;
    let of = |pick: fn(&RawRow) -> f64| {
        let mut values: Vec<f64> = rows.iter().map(pick).collect();
        median(&mut values)
    };
    let median_us = of(|r| r.median_us);
    let medians: Vec<f64> = rows.iter().map(|r| r.median_us).collect();
    let min_median = medians.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_median = medians.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let spread_pct = if median_us > 0.0 {
        100.0 * (max_median - min_median) / median_us
    } else {
        0.0
    };
    Record {
        case,
        display,
        group,
        implementation,
        implementation_version: rows
            .iter()
            .map(|r| r.version.clone())
            .next()
            .unwrap_or_default(),
        payload_bytes,
        runs,
        samples: rows.last().map(|r| r.samples).unwrap_or(0),
        p0_us: of(|r| r.p0_us),
        median_us,
        p80_us: of(|r| r.p80_us),
        p90_us: of(|r| r.p90_us),
        p95_us: of(|r| r.p95_us),
        p99_us: of(|r| r.p99_us),
        p999_us: of(|r| r.p999_us),
        max_us: rows
            .iter()
            .map(|r| r.max_us)
            .fold(f64::NEG_INFINITY, f64::max),
        loss_pct: rows.iter().map(|r| r.loss_pct).sum::<f64>() / runs as f64,
        spread_pct,
        min_median_us: min_median,
        max_median_us: max_median,
        achieved_hz: of(|r| r.achieved_hz),
    }
}

/// Fail the run when a row did not achieve the rate it was asked for.
///
/// A row that received well under the rate it was paced at measured a
/// backlog, not a transport, and the number it reports is not the number the
/// run set out to take. The report is written first, so the row that failed
/// can be read.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::InvalidData`] naming every row that came in
/// below nine tenths of the asked rate.
pub fn check_achieved_rate(conditions: &Conditions, records: &[Record]) -> std::io::Result<()> {
    let floor = conditions.rate_hz as f64 * 0.9;
    let short: Vec<String> = records
        .iter()
        .filter(|r| r.achieved_hz < floor)
        .map(|r| {
            format!(
                "{}/{} at {} B achieved {:.1} Hz of {} asked",
                r.case, r.implementation, r.payload_bytes, r.achieved_hz, conditions.rate_hz
            )
        })
        .collect();
    if short.is_empty() {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("rows below the asked rate:\n  {}", short.join("\n  ")),
    ))
}

/// Where a group's table sits in the report, headline first.
fn rank(group: &str) -> u8 {
    match group {
        "clients" => 0,
        "servers" => 1,
        _ => 2,
    }
}

fn heading(group: &str) -> &'static str {
    match group {
        "servers" => "Servers",
        "clients" => "Clients",
        _ => "Other",
    }
}

fn blurb(group: &str) -> &'static str {
    match group {
        "servers" => {
            "Each server with its client library taken out of the path. `tarwyn-rust` and `ntcore` \
are driven by the same raw NT4 publisher from this repo; `tarwyn` speaks ZeroMQ, so it is driven \
by a raw JeroMQ publisher sending the bytes its own client would. That row carries a JVM where the \
other two carry a Rust process, so it is a ceiling on the TARWYN server; read it against \
`tarwyn` in the clients table, the same JVM and wire with `TarwynClient` added back."
        }
        "clients" => {
            "What a robot's own code gets, and the comparison that decides anything: every row \
publishes through its project's own client library."
        }
        _ => "",
    }
}

/// Which implementation won a row, and whether the win clears the noise.
///
/// Two rows whose run-to-run ranges overlap did not measure a difference, they
/// measured the machine. Saying so in the cell is the only way a reader who
/// stops at the table gets the same answer as one who reads the spread table.
fn verdict(cells: &[&Record]) -> String {
    if cells.len() < 2 {
        return "-".to_string();
    }
    let mut ordered: Vec<&&Record> = cells.iter().collect();
    ordered.sort_by(|a, b| a.median_us.partial_cmp(&b.median_us).unwrap());
    let (best, next) = (ordered[0], ordered[1]);
    if best.max_median_us >= next.min_median_us {
        return format!(
            "within noise ({} vs {})",
            best.implementation, next.implementation
        );
    }
    format!(
        "{}, {:.1}x",
        best.implementation,
        next.median_us / best.median_us
    )
}

pub fn markdown(conditions: &Conditions, records: &[Record]) -> String {
    let mut out = String::new();
    out.push_str("# Benchmark Results\n\n");
    out.push_str("Regenerate with `bench sweep`; see [BENCHMARK.md](BENCHMARK.md).\n\n");

    out.push_str("## Testbed\n\n");
    out.push_str(
        "The conditions of this run. They change with the machine, so figures from two testbeds \
say nothing about each other; rerun the benchmark on yours rather than reading these.\n\n",
    );
    let _ = writeln!(out, "|  |  |\n|---|---|");
    let mut machine = conditions.os.clone();
    if conditions.kernel != "unknown" {
        machine.push(' ');
        machine.push_str(&conditions.kernel);
    }
    if conditions.cpu != "unknown" {
        machine.push_str(", ");
        machine.push_str(&conditions.cpu);
    }
    let _ = writeln!(out, "|machine|{machine}|");
    let _ = writeln!(out, "|commit|{}|", conditions.commit);
    let _ = writeln!(
        out,
        "|measured|{} Hz, {} samples, {} warmup, {} reps|",
        conditions.rate_hz, conditions.samples, conditions.warmup, conditions.reps
    );
    let _ = writeln!(
        out,
        "|implementations|{}|",
        conditions.implementations.join(", ")
    );

    let mut groups: Vec<&str> = records
        .iter()
        .map(|r| r.group.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    groups.sort_by_key(|g| (rank(g), *g));

    out.push_str(
        "\nCells are medians in microseconds, with the lowest and highest run in brackets, then \
the p99 and the loss. A row whose two best run-to-run ranges overlap is marked `within noise` \
and did not measure a difference.\n",
    );

    for group in groups {
        let _ = writeln!(out, "\n## {}\n", heading(group));
        let _ = writeln!(out, "{}\n", blurb(group));
        let payloads: BTreeSet<usize> = records
            .iter()
            .filter(|r| r.group == group)
            .map(|r| r.payload_bytes)
            .collect();
        for payload in payloads {
            let _ = writeln!(out, "\n### {payload} B\n");
            let implementations: Vec<&str> = records
                .iter()
                .filter(|r| r.group == group && r.payload_bytes == payload)
                .map(|r| r.implementation.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let _ = writeln!(out, "|Operation|{}|Verdict|", implementations.join("|"));
            let _ = writeln!(
                out,
                "|---|{}|---|",
                vec!["---"; implementations.len()].join("|")
            );

            let cases: BTreeSet<&str> = records
                .iter()
                .filter(|r| r.group == group && r.payload_bytes == payload)
                .map(|r| r.case.as_str())
                .collect();
            for case in cases {
                let present: Vec<&Record> = records
                    .iter()
                    .filter(|r| r.case == case && r.payload_bytes == payload)
                    .collect();
                let display = present.first().map(|r| r.display.as_str()).unwrap_or(case);
                let mut row = format!("|{display}|");
                for implementation in &implementations {
                    let cell = present
                        .iter()
                        .find(|r| r.implementation == *implementation)
                        .map(|r| {
                            format!(
                                "{:.2} ({:.2}–{:.2} over {}) p99 {:.2}, loss {:.2}%",
                                r.median_us,
                                r.min_median_us,
                                r.max_median_us,
                                r.runs,
                                r.p99_us,
                                r.loss_pct
                            )
                        })
                        .unwrap_or_else(|| "-".to_string());
                    let _ = write!(row, "{cell}|");
                }
                let _ = writeln!(out, "{row}{}|", verdict(&present));
            }
        }
    }

    let mut sorted: Vec<&Record> = records.iter().collect();
    sorted.sort_by(|a, b| {
        rank(&a.group)
            .cmp(&rank(&b.group))
            .then(a.display.cmp(&b.display))
            .then(a.payload_bytes.cmp(&b.payload_bytes))
            .then(a.implementation.cmp(&b.implementation))
    });

    out.push_str("\n## Detail\n\n");
    out.push_str("Every percentile the run recorded.\n\n");
    let _ = writeln!(
        out,
        "|Section|Operation|Implementation|Version|Payload|P0|Median|P80|P90|P95|P99|P99.9|P100|Loss (%)|Samples|Achieved (Hz)|"
    );
    let _ = writeln!(
        out,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for r in &sorted {
        let _ = writeln!(
            out,
            "|{}|{}|{}|{}|{} B|{:.2}|{:.2}|{:.2}|{:.2}|{:.2}|{:.2}|{:.2}|{:.2}|{:.2}|{}|{:.1}|",
            heading(&r.group),
            r.display,
            r.implementation,
            r.implementation_version,
            r.payload_bytes,
            r.p0_us,
            r.median_us,
            r.p80_us,
            r.p90_us,
            r.p95_us,
            r.p99_us,
            r.p999_us,
            r.max_us,
            r.loss_pct,
            r.samples,
            r.achieved_hz
        );
    }

    out.push_str("\n## Run-to-Run Spread\n\n");
    out.push_str("How far the median moved between runs of the same row.\n\n");
    let _ = writeln!(
        out,
        "|Section|Operation|Implementation|Payload|Runs|Lowest median|Highest median|Spread (%)|"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    for r in &sorted {
        let _ = writeln!(
            out,
            "|{}|{}|{}|{} B|{}|{:.2}|{:.2}|{:.1}|",
            heading(&r.group),
            r.display,
            r.implementation,
            r.payload_bytes,
            r.runs,
            r.min_median_us,
            r.max_median_us,
            r.spread_pct
        );
    }
    out
}

/// # Errors
///
/// Returns any error from writing `path`.
pub fn write_markdown(
    path: &Path,
    conditions: &Conditions,
    records: &[Record],
) -> std::io::Result<()> {
    std::fs::write(path, markdown(conditions, records))
}

/// Write the machine-readable record of the run.
///
/// # Errors
///
/// Returns any error from writing `path`.
pub fn write_json(path: &Path, conditions: &Conditions, records: &[Record]) -> std::io::Result<()> {
    let cases: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "case": r.case,
                "group": r.group,
                "impl": r.implementation,
                "implementation_version": r.implementation_version,
                "payload_bytes": r.payload_bytes,
                "runs": r.runs,
                "samples": r.samples,
                "median_us": r.median_us,
                "p99_us": r.p99_us,
                "max_us": r.max_us,
                "loss_pct": r.loss_pct,
                "spread_pct": r.spread_pct,
                "min_median_us": r.min_median_us,
                "max_median_us": r.max_median_us,
                "achieved_hz": r.achieved_hz,
            })
        })
        .collect();
    let document = serde_json::json!({
        "schema": 3,
        "conditions": {
            "os": conditions.os,
            "kernel": conditions.kernel,
            "cpu": conditions.cpu,
            "governor": conditions.governor,
            "boost": conditions.boost,
            "loadavg": conditions.loadavg,
            "commit": conditions.commit,
            "rate_hz": conditions.rate_hz,
            "samples": conditions.samples,
            "warmup": conditions.warmup,
            "reps": conditions.reps,
        },
        "implementations": conditions.implementations,
        "cases": cases,
    });
    std::fs::write(path, serde_json::to_string_pretty(&document)?)
}

#[cfg(test)]
mod tests {
    use super::{Conditions, Record, check_achieved_rate, markdown, parse_row_line};

    fn record(case: &str, implementation: &str, median: f64) -> Record {
        Record {
            case: case.to_string(),
            display: case.to_string(),
            group: "clients".to_string(),
            implementation: implementation.to_string(),
            implementation_version: "1.2.3".to_string(),
            payload_bytes: 96,
            runs: 3,
            samples: 3000,
            p0_us: median * 0.7,
            median_us: median,
            p80_us: median * 1.2,
            p90_us: median * 1.4,
            p95_us: median * 1.6,
            p99_us: median * 4.0,
            p999_us: median * 20.0,
            max_us: median * 40.0,
            loss_pct: 0.0,
            spread_pct: 4.2,
            min_median_us: median * 0.98,
            max_median_us: median * 1.02,
            achieved_hz: 500.0,
        }
    }

    #[test]
    fn the_matrix_has_a_cell_for_every_case_and_implementation() {
        let records = vec![
            record("publish", "tarwyn-rust", 34.1),
            record("publish", "ntcore", 49.5),
        ];
        let out = markdown(&Conditions::sample(), &records);
        assert!(out.contains("publish"), "the matrix must list publish");
        assert!(out.contains("34.10"), "cells carry the median");
        assert!(out.contains("49.50"));
    }

    #[test]
    fn an_implementation_without_a_record_reads_as_absent() {
        let records = vec![record("publish", "tarwyn-rust", 71.2)];
        let out = markdown(&Conditions::sample(), &records);
        assert!(
            !out.contains("0.00 us"),
            "a missing cell must never read as a zero median"
        );
    }

    #[test]
    fn a_win_inside_the_run_to_run_range_is_called_noise() {
        let mut close = record("publish", "ntcore", 38.53);
        close.min_median_us = 38.46;
        close.max_median_us = 41.18;
        let mut ours = record("publish", "tarwyn-rust", 36.03);
        ours.min_median_us = 34.72;
        ours.max_median_us = 39.39;
        let out = markdown(&Conditions::sample(), &[close, ours]);
        assert!(
            out.contains("within noise"),
            "overlapping ranges are not a result: {out}"
        );
    }

    #[test]
    fn a_win_clear_of_the_ranges_names_the_winner_and_the_ratio() {
        let mut slow = record("publish", "ntcore", 53.44);
        slow.min_median_us = 52.18;
        slow.max_median_us = 54.35;
        let mut fast = record("publish", "tarwyn-rust", 35.33);
        fast.min_median_us = 34.21;
        fast.max_median_us = 35.33;
        let out = markdown(&Conditions::sample(), &[slow, fast]);
        assert!(out.contains("tarwyn-rust, 1.5x"), "{out}");
    }

    #[test]
    fn an_unknown_release_or_cpu_is_left_out_rather_than_printed() {
        let mut bare = Conditions::sample();
        bare.kernel = "unknown".to_string();
        bare.cpu = "unknown".to_string();
        let out = markdown(&bare, &[record("publish", "tarwyn-rust", 34.1)]);
        assert!(out.contains("|machine|linux|"), "{out}");
        assert!(
            !out.contains("unknown"),
            "the report never prints unknown: {out}"
        );
    }

    #[test]
    fn the_testbed_block_names_the_machine_the_numbers_came_from() {
        let out = markdown(
            &Conditions::sample(),
            &[record("publish", "tarwyn-rust", 34.1)],
        );
        assert!(out.contains("## Testbed"));
        assert!(
            out.contains("AMD Ryzen 5 5600X"),
            "the machine belongs in the report, not only the json"
        );
        assert!(out.contains("500 Hz"), "the rate belongs in the conditions");
    }

    #[test]
    fn a_row_that_missed_its_rate_fails_the_run() {
        let mut slow = record("publish", "ntcore", 34.1);
        slow.achieved_hz = 430.0;
        let err = check_achieved_rate(&Conditions::sample(), &[slow])
            .expect_err("a row at 430 of 500 Hz measured a backlog");
        assert!(err.to_string().contains("430"), "{err}");
    }

    #[test]
    fn a_row_at_its_rate_passes() {
        check_achieved_rate(&Conditions::sample(), &[record("publish", "ntcore", 34.1)])
            .expect("500 Hz of 500 asked is the whole rate");
    }

    #[test]
    fn a_malformed_row_is_rejected_with_its_line_number() {
        let short = "ROW\tpublish\ttarwyn-rust\t0.1.0\t96\t34.10\t10.0\t20.0";
        let err = parse_row_line(short, 42).expect_err("too few fields must error");
        assert!(
            err.to_string().contains("42"),
            "the error must name the line number: {err}"
        );
    }

    #[test]
    fn a_row_without_a_version_is_rejected() {
        let row = "ROW\tpublish\tntcore\t\t96\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00\t3000\t500.0";
        let err = parse_row_line(row, 7).expect_err("an unversioned row is not comparable");
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn a_full_row_parses_every_field() {
        let row = "ROW\tpublish\tntcore\t2027.0.0\t96\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00\t3000\t499.4";
        let parsed = parse_row_line(row, 5).expect("a full row must parse");
        assert_eq!(parsed.case, "publish");
        assert_eq!(parsed.implementation, "ntcore");
        assert_eq!(parsed.version, "2027.0.0");
        assert_eq!(parsed.median_us, 51.25);
        assert_eq!(parsed.p99_us, 867.93);
        assert_eq!(parsed.achieved_hz, 499.4);
    }
}
