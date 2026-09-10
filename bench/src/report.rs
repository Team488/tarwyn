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

/// What the machine was doing while the run happened.
#[derive(Debug, Clone)]
pub struct Conditions {
    /// Kernel release, as `uname -r`.
    pub kernel: String,
    pub cpu: String,
    pub governor: String,
    pub boost: bool,
    /// The one-minute load average when the run started.
    pub loadavg: f64,
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
            kernel: "7.2.3-arch1-2".to_string(),
            cpu: "AMD Ryzen 5 5600X".to_string(),
            governor: "powersave".to_string(),
            boost: true,
            loadavg: 0.54,
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
            rate_hz,
            samples,
            warmup,
            reps,
            implementations,
        }
    }
}

fn read_kernel() -> String {
    std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// One measured case, for one implementation, at one payload size.
#[derive(Debug, Clone)]
pub struct Record {
    pub case: String,
    /// The table this case belongs in.
    pub group: String,
    /// How it was timed.
    pub mode: String,
    pub implementation: String,
    pub payload_bytes: usize,
    /// How many runs the median was picked from.
    pub runs: u32,
    pub samples: u64,
    pub median_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub loss_pct: f64,
    /// How far the median moved between runs, as a percentage.
    pub spread_pct: f64,
    pub achieved_hz: Option<f64>,
    /// The measured implementation's version, when the row carried one.
    pub implementation_version: Option<String>,
}

/// One parsed `ROW` line, before grouping across repeated runs.
#[derive(Debug)]
struct RawRow {
    subject: String,
    payload_bytes: usize,
    median_us: f64,
    p99_us: f64,
    max_us: f64,
    loss_pct: f64,
    samples: u64,
    achieved_hz: Option<f64>,
    version: Option<String>,
}

/// Parse one `ROW` line's tab-separated fields.
///
/// # Errors
///
/// Returns an error naming `line_no` if the line has fewer than 13 fields,
/// so a truncated line fails loudly instead of parsing into zeros.
/// Fields beyond 13 are optional: fields 14-16 are corrected median,
/// corrected p99 and achieved rate; a missing achieved rate is absent
/// rather than zero.
fn parse_row_line(line: &str, line_no: usize) -> std::io::Result<RawRow> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 13 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "line {line_no}: ROW row has {} fields, expected at least 13",
                fields.len()
            ),
        ));
    }
    let bad = |what: &str| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("line {line_no}: could not parse {what}"),
        )
    };
    let parse_f64 = |field: &str, what: &str| field.parse::<f64>().map_err(|_| bad(what));
    Ok(RawRow {
        subject: fields[1].to_string(),
        payload_bytes: fields[2].parse().map_err(|_| bad("payload_bytes"))?,
        median_us: parse_f64(fields[3], "median")?,
        p99_us: parse_f64(fields[8], "p99")?,
        max_us: parse_f64(fields[10], "p100")?,
        loss_pct: parse_f64(fields[11], "loss")?,
        samples: fields[12].parse().map_err(|_| bad("samples"))?,
        achieved_hz: fields.get(15).and_then(|f| f.parse::<f64>().ok()),
        version: fields
            .get(16)
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .map(str::to_string),
    })
}

/// Split a `ROW` subject into its case and implementation.
///
/// A round-trip subject is `"<case> <implementation>"`; a delivery subject
/// today is just the implementation's own label. `catalog::find` decides
/// which one a subject is, rather than guessing from its shape: only a
/// first token that names a real case is treated as one.
fn split_subject(subject: &str) -> (String, String) {
    if let Some((first, rest)) = subject.split_once(' ')
        && catalog::find(first).is_some()
    {
        return (first.to_string(), rest.to_string());
    }
    (subject.to_string(), subject.to_string())
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
        let (case, implementation) = split_subject(&row.subject);
        if catalog::find(&case).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: {case} is not in the catalog", index + 1),
            ));
        }
        groups
            .entry((case, implementation, row.payload_bytes))
            .or_default()
            .push(row);
    }

    let mut records = Vec::with_capacity(groups.len());
    for ((case, implementation), rows) in groups
        .into_iter()
        .map(|((case, implementation, payload), rows)| ((case, implementation), (payload, rows)))
    {
        let (payload_bytes, rows) = rows;
        let declared = catalog::find(&case).expect("checked above");
        let group = declared.group.to_string();
        let mode = declared.mode.as_str().to_string();
        records.push(fold(
            case,
            group,
            mode,
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
    group: String,
    mode: String,
    implementation: String,
    payload_bytes: usize,
    rows: &[RawRow],
) -> Record {
    let runs = rows.len() as u32;
    let mut medians: Vec<f64> = rows.iter().map(|r| r.median_us).collect();
    let median_us = median(&mut medians);
    let min_median = medians.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_median = medians.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let spread_pct = if median_us > 0.0 {
        100.0 * (max_median - min_median) / median_us
    } else {
        0.0
    };
    let mut p99s: Vec<f64> = rows.iter().map(|r| r.p99_us).collect();
    let p99_us = median(&mut p99s);
    let max_us = rows
        .iter()
        .map(|r| r.max_us)
        .fold(f64::NEG_INFINITY, f64::max);
    let loss_pct = rows.iter().map(|r| r.loss_pct).sum::<f64>() / runs as f64;
    let mut hzs: Vec<f64> = rows.iter().filter_map(|r| r.achieved_hz).collect();
    let achieved_hz = if hzs.is_empty() {
        None
    } else {
        Some(median(&mut hzs))
    };
    let samples = rows.last().map(|r| r.samples).unwrap_or(0);
    let implementation_version = rows.iter().find_map(|r| r.version.clone());
    Record {
        case,
        group,
        mode,
        implementation,
        payload_bytes,
        runs,
        samples,
        median_us,
        p99_us,
        max_us,
        loss_pct,
        spread_pct,
        achieved_hz,
        implementation_version,
    }
}

/// Render the whole report.
pub fn markdown(conditions: &Conditions, records: &[Record]) -> String {
    let mut out = String::new();
    out.push_str("# Benchmark results\n\n");
    out.push_str("Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).\n\n");

    out.push_str("## Run conditions\n\n");
    let _ = writeln!(out, "|  |  |\n|---|---|");
    let _ = writeln!(
        out,
        "|machine|{}, kernel {}|",
        conditions.cpu, conditions.kernel
    );
    let _ = writeln!(
        out,
        "|scaling|{}, boost {}|",
        conditions.governor,
        if conditions.boost { "on" } else { "off" }
    );
    let _ = writeln!(out, "|load average|{:.2} at start|", conditions.loadavg);
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

    let groups: BTreeSet<&str> = records.iter().map(|r| r.group.as_str()).collect();
    for group in groups {
        let _ = writeln!(out, "\n## {group}\n");
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
            let _ = writeln!(out, "|Operation|{}|", implementations.join("|"));
            let _ = writeln!(
                out,
                "|---|{}|",
                vec!["---"; implementations.len()].join("|")
            );

            let cases: BTreeSet<&str> = records
                .iter()
                .filter(|r| r.group == group && r.payload_bytes == payload)
                .map(|r| r.case.as_str())
                .collect();
            for case in cases {
                let mut row = format!("|{case}|");
                for implementation in &implementations {
                    let cell = records
                        .iter()
                        .find(|r| {
                            r.group == group
                                && r.case == case
                                && r.payload_bytes == payload
                                && r.implementation == *implementation
                        })
                        .map(|r| {
                            format!(
                                "{:.2} us (p99 {:.2} us, loss {:.2}%)",
                                r.median_us, r.p99_us, r.loss_pct
                            )
                        })
                        .unwrap_or_else(|| "-".to_string());
                    let _ = write!(row, "{cell}|");
                }
                let _ = writeln!(out, "{row}");
            }
        }
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
                "mode": r.mode,
                "impl": r.implementation,
                "payload_bytes": r.payload_bytes,
                "runs": r.runs,
                "samples": r.samples,
                "median_us": r.median_us,
                "p99_us": r.p99_us,
                "max_us": r.max_us,
                "loss_pct": r.loss_pct,
                "spread_pct": r.spread_pct,
                "achieved_hz": r.achieved_hz,
                "implementation_version": r.implementation_version,
            })
        })
        .collect();
    let document = serde_json::json!({
        "schema": 1,
        "conditions": {
            "kernel": conditions.kernel,
            "cpu": conditions.cpu,
            "governor": conditions.governor,
            "boost": conditions.boost,
            "loadavg": conditions.loadavg,
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
    use super::{Conditions, Record, markdown, parse_row_line};

    fn record(case: &str, implementation: &str, median: f64) -> Record {
        Record {
            case: case.to_string(),
            group: "delivery".to_string(),
            mode: "delivery".to_string(),
            implementation: implementation.to_string(),
            payload_bytes: 96,
            runs: 3,
            samples: 3000,
            median_us: median,
            p99_us: median * 4.0,
            max_us: median * 40.0,
            loss_pct: 0.0,
            spread_pct: 4.2,
            achieved_hz: Some(500.0),
            implementation_version: None,
        }
    }

    #[test]
    fn the_matrix_has_a_cell_for_every_case_and_implementation() {
        let records = vec![
            record("publish", "tarwyn-rust", 34.1),
            record("publish", "ntcore", 49.5),
            record("get", "tarwyn-rust-client", 71.2),
        ];
        let out = markdown(&Conditions::sample(), &records);
        assert!(out.contains("publish"), "the matrix must list publish");
        assert!(out.contains("34.10"), "cells carry the median");
        assert!(out.contains("49.50"));
        assert!(out.contains("71.20"));
    }

    #[test]
    fn an_implementation_without_a_record_reads_as_absent() {
        let records = vec![record("get", "tarwyn-rust-client", 71.2)];
        let out = markdown(&Conditions::sample(), &records);
        assert!(
            out.contains('-'),
            "an operation an implementation lacks must show a dash, not a zero"
        );
        assert!(
            !out.contains("0.00 us"),
            "a missing cell must never read as a zero median"
        );
    }

    #[test]
    fn the_conditions_block_names_what_was_measured() {
        let out = markdown(
            &Conditions::sample(),
            &[record("publish", "tarwyn-rust", 34.1)],
        );
        assert!(out.contains("Run conditions"));
        assert!(out.contains("500 Hz"), "the rate belongs in the conditions");
    }

    #[test]
    fn a_malformed_row_is_rejected_with_its_line_number() {
        let short = "ROW\ttarwyn-rust\t96\t34.10\t10.0\t20.0\t25.0\t30.0";
        let err = parse_row_line(short, 42).expect_err("too few fields must error");
        assert!(
            err.to_string().contains("42"),
            "the error must name the line number: {err}"
        );
    }

    #[test]
    fn a_13_field_row_parses_with_fallback_achieved_hz() {
        let row_13 = "ROW\tjtable-java\t96\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00\t3000";
        let row = parse_row_line(row_13, 5).expect("13-field row must parse");
        assert_eq!(row.subject, "jtable-java");
        assert_eq!(row.payload_bytes, 96);
        assert_eq!(row.median_us, 51.25);
        assert_eq!(row.p99_us, 867.93);
        assert_eq!(
            row.achieved_hz, None,
            "missing achieved_hz must be absent, not 0.0"
        );
    }

    #[test]
    fn a_12_field_row_is_rejected_with_line_number() {
        let row_12 = "ROW\tjtable-java\t96\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00";
        let err = parse_row_line(row_12, 99).expect_err("12-field row must error");
        assert!(
            err.to_string().contains("99"),
            "the error must name line 99: {err}"
        );
    }
}
