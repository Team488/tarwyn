//! The record of one benchmark run, as JSON and as markdown generated from the
//! same records.

use crate::catalog;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

/// How many fields a `ROW` line has, including the leading `ROW`.
const ROW_FIELDS: usize = 17;

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
    /// The day the run started, as `YYYY-MM-DD` in UTC.
    pub date: String,
    /// Logical CPUs the machine offered.
    pub cores: usize,
    /// Which cpu each process was held to, in words. `unpinned` otherwise.
    pub pinning: String,
    /// The rates the run paced at, in Hz.
    pub rates: Vec<u64>,
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
            date: "2026-09-21".to_string(),
            cores: 12,
            pinning: "publisher on cpu 3, subscriber on cpu 4, server on cpus 1,2,5".to_string(),
            rates: vec![500],
            samples: 3000,
            warmup: 500,
            reps: 3,
            implementations: vec!["tarwyn=0.1.0".to_string()],
        }
    }

    /// Read what can be read from `/proc` and `/sys`. Anything unreadable
    /// becomes `unknown` (or `false`/`0.0`) so the run continues.
    pub fn from_machine(
        rates: Vec<u64>,
        samples: u64,
        warmup: u64,
        reps: u32,
        implementations: Vec<String>,
        pinning: String,
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
            date: today_utc(),
            cores: std::thread::available_parallelism().map_or(0, |n| n.get()),
            pinning,
            rates,
            samples,
            warmup,
            reps,
            implementations,
        }
    }
}

/// Today as `YYYY-MM-DD` in UTC, from the system clock. Uses Howard Hinnant's
/// `civil_from_days`.
fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    date_from_unix_days((secs / 86_400) as i64)
}

fn date_from_unix_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
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

/// The commit measured, with `-dirty` when the tree had uncommitted changes,
/// since a report from such a tree cannot be rebuilt from the hash alone.
fn read_commit() -> String {
    let Some(commit) = command_output("git", &["rev-parse", "--short", "HEAD"]) else {
        return "unknown".to_string();
    };
    match command_output("git", &["status", "--porcelain"]) {
        Some(_) => format!("{commit}-dirty"),
        None => commit,
    }
}

/// The CPU model, from whichever of these the platform has.
///
/// `/proc/cpuinfo` on Linux, `machdep.cpu.brand_string` on macOS, the registry
/// on Windows. Anything else reports `unknown` so the run continues.
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
    /// The measured implementation's version. Never empty.
    pub implementation_version: String,
    pub payload_bytes: usize,
    /// The publish rate the row was paced at.
    pub rate_hz: u64,
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
    /// Attempts that reported nothing before one did, out of the sweep's two.
    pub retries: u32,
    /// The server's CPU use while samples were recorded, as the middle run's
    /// percent of one core. `None` where the harness could not read it.
    pub server_cpu_pct: Option<f64>,
}

/// One parsed `ROW` line, before grouping across repeated runs.
#[derive(Debug)]
struct RawRow {
    case: String,
    implementation: String,
    version: String,
    payload_bytes: usize,
    rate_hz: u64,
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
/// # Errors
///
/// Returns an error naming `line_no` when the line has another field count,
/// or a field does not parse.
fn parse_row_line(line: &str, line_no: usize) -> anyhow::Result<RawRow> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != ROW_FIELDS {
        return Err(anyhow::anyhow!(
            "line {line_no}: ROW row has {} fields, expected exactly {ROW_FIELDS}",
            fields.len()
        ));
    }
    let bad = |what: &str| anyhow::anyhow!("line {line_no}: {what} is not a number");
    let parse_f64 = |field: &str, what: &str| -> anyhow::Result<f64> {
        field.trim().parse::<f64>().map_err(|_| bad(what))
    };
    let version = fields[3].trim();
    if version.is_empty() {
        return Err(anyhow::anyhow!(
            "line {line_no}: the row carries no implementation version"
        ));
    }
    Ok(RawRow {
        case: fields[1].trim().to_string(),
        implementation: fields[2].trim().to_string(),
        version: version.to_string(),
        payload_bytes: fields[4].parse().map_err(|_| bad("payload_bytes"))?,
        rate_hz: fields[5].trim().parse().map_err(|_| bad("rate_hz"))?,
        median_us: parse_f64(fields[6], "median")?,
        p0_us: parse_f64(fields[7], "p0")?,
        p80_us: parse_f64(fields[8], "p80")?,
        p90_us: parse_f64(fields[9], "p90")?,
        p95_us: parse_f64(fields[10], "p95")?,
        p99_us: parse_f64(fields[11], "p99")?,
        p999_us: parse_f64(fields[12], "p999")?,
        max_us: parse_f64(fields[13], "p100")?,
        loss_pct: parse_f64(fields[14], "loss")?,
        samples: fields[15].trim().parse().map_err(|_| bad("samples"))?,
        achieved_hz: parse_f64(fields[16], "achieved_hz")?,
    })
}

/// Parse a run's `ROW` lines into [`Record`]s, folding repeated runs together.
/// Other lines are ignored.
///
/// # Errors
///
/// Returns an error naming the line of any malformed `ROW`, or the error from
/// reading `path`.
pub fn parse_rows(path: &Path) -> anyhow::Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)?;
    let mut groups: BTreeMap<(String, String, usize, u64), Vec<RawRow>> = BTreeMap::new();
    let mut retries: BTreeMap<(String, String, usize, u64), u32> = BTreeMap::new();
    let mut cpu: BTreeMap<(String, String, usize, u64), Vec<f64>> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("RETRY\t") {
            if let Some(key) = side_key(rest) {
                *retries.entry(key).or_default() += 1;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("CPU\t") {
            let fields: Vec<&str> = rest.split('\t').collect();
            if let (Some(key), Some(pct)) = (
                side_key(rest),
                fields.get(4).and_then(|f| f.trim().parse::<f64>().ok()),
            ) {
                cpu.entry(key).or_default().push(pct);
            }
            continue;
        }
        if !line.starts_with("ROW") {
            continue;
        }
        let row = parse_row_line(line, index + 1)?;
        let Some(declared) = catalog::find(&row.case) else {
            return Err(anyhow::anyhow!(
                "line {}: {} is not in the catalog",
                index + 1,
                row.case
            ));
        };
        if !declared
            .implementations
            .contains(&row.implementation.as_str())
        {
            return Err(anyhow::anyhow!(
                "line {}: {} does not declare {}; the rows on disk are from another catalog",
                index + 1,
                row.case,
                row.implementation
            ));
        }
        groups
            .entry((
                row.case.clone(),
                row.implementation.clone(),
                row.payload_bytes,
                row.rate_hz,
            ))
            .or_default()
            .push(row);
    }

    let mut records = Vec::with_capacity(groups.len());
    for ((case, implementation, payload_bytes, rate_hz), rows) in groups {
        let declared = catalog::find(&case).expect("checked above");
        let key = (case.clone(), implementation.clone(), payload_bytes, rate_hz);
        let mut record = fold(
            case,
            declared.display.to_string(),
            declared.group.to_string(),
            implementation,
            payload_bytes,
            rate_hz,
            &rows,
        );
        record.retries = retries.get(&key).copied().unwrap_or(0);
        record.server_cpu_pct = cpu.get_mut(&key).map(|values| median(values));
        records.push(record);
    }
    if records.is_empty() {
        return Err(anyhow::anyhow!(
            "{}: no ROW records were parsed",
            path.display()
        ));
    }
    Ok(records)
}

/// The `(case, implementation, payload, rate)` a `RETRY` or `CPU` line names.
fn side_key(rest: &str) -> Option<(String, String, usize, u64)> {
    let mut fields = rest.split('\t');
    let case = fields.next()?.trim().to_string();
    let implementation = fields.next()?.trim().to_string();
    let payload = fields.next()?.trim().parse().ok()?;
    let rate = fields.next()?.trim().parse().ok()?;
    Some((case, implementation, payload, rate))
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
    rate_hz: u64,
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
        rate_hz,
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
        retries: 0,
        server_cpu_pct: None,
    }
}

/// Fail the run when a row received under nine tenths of its paced rate. Call
/// it after the report is written.
///
/// # Errors
///
/// Returns an error that names every such row.
pub fn check_achieved_rate(records: &[Record]) -> anyhow::Result<()> {
    let short: Vec<String> = records
        .iter()
        .filter(|r| r.achieved_hz < r.rate_hz as f64 * 0.9)
        .map(|r| {
            format!(
                "{}/{} at {} B achieved {:.1} Hz of {} asked",
                r.case, r.implementation, r.payload_bytes, r.achieved_hz, r.rate_hz
            )
        })
        .collect();
    if short.is_empty() {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "rows below the asked rate:\n  {}",
        short.join("\n  ")
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

/// One sentence on who won an operation, on the median and on the p99. It
/// says "within noise" when the run ranges overlap.
fn verdict(cells: &[&Record]) -> Option<String> {
    if cells.len() < 2 {
        return None;
    }
    let mut by_median: Vec<&&Record> = cells.iter().collect();
    by_median.sort_by(|a, b| a.median_us.partial_cmp(&b.median_us).unwrap());
    let (best, next) = (by_median[0], by_median[1]);
    let median = if best.max_median_us >= next.min_median_us {
        format!(
            "{} and {} within noise on the median",
            best.implementation, next.implementation
        )
    } else {
        format!(
            "{} {:.1}x faster than {} on the median",
            best.implementation,
            next.median_us / best.median_us,
            next.implementation
        )
    };
    let mut by_tail: Vec<&&Record> = cells.iter().collect();
    by_tail.sort_by(|a, b| a.p99_us.partial_cmp(&b.p99_us).unwrap());
    let (best_tail, next_tail) = (by_tail[0], by_tail[1]);
    let ratio = next_tail.p99_us / best_tail.p99_us;
    let tail = if best_tail.p99_us <= 0.0 {
        String::new()
    } else if best.max_median_us < next.min_median_us
        && best_tail.implementation == best.implementation
    {
        format!(" and {ratio:.1}x on the p99")
    } else {
        format!(", {} {ratio:.1}x on the p99", best_tail.implementation)
    };
    Some(format!("{median}{tail}."))
}

fn loss_cell(loss_pct: f64) -> String {
    if loss_pct == 0.0 {
        "0%".to_string()
    } else {
        format!("{loss_pct:.2}%")
    }
}

pub fn markdown(conditions: &Conditions, records: &[Record]) -> String {
    let mut out = String::new();
    out.push_str("# Benchmark Results\n\n");
    out.push_str(
        "Latency is one way, in microseconds. [BENCHMARK.md](BENCHMARK.md) explains how it \
is measured and how to read the tables, and `results.json` beside this file has every \
percentile. Regenerate with `bench sweep`.\n\n",
    );

    let mut machine = conditions.os.clone();
    if conditions.kernel != "unknown" {
        machine.push(' ');
        machine.push_str(&conditions.kernel);
    }
    if conditions.cpu != "unknown" {
        machine.push_str(", ");
        machine.push_str(&conditions.cpu);
    }
    let _ = writeln!(out, "|  |  |\n|---|---|");
    let _ = writeln!(out, "|date|{}|", conditions.date);
    let _ = writeln!(
        out,
        "|machine|{machine}, {} logical cpus|",
        conditions.cores
    );
    let _ = writeln!(
        out,
        "|state|governor {}, boost {}, load {:.2} at start|",
        conditions.governor,
        if conditions.boost { "on" } else { "off" },
        conditions.loadavg
    );
    let _ = writeln!(out, "|pinning|{}|", conditions.pinning);
    let _ = writeln!(out, "|commit|{}|", conditions.commit);
    let _ = writeln!(
        out,
        "|measured|{} Hz, {} samples after {} warmup, {} reps|",
        list(conditions.rates.iter().map(|r| r.to_string())),
        conditions.samples,
        conditions.warmup,
        conditions.reps
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
    let any_loss = records.iter().any(|r| r.loss_pct > 0.0);
    let (loss_head, loss_rule) = if any_loss {
        ("Loss|", "---|")
    } else {
        ("", "")
    };

    for group in groups {
        let _ = writeln!(out, "\n## {}", heading(group));
        let cells: BTreeSet<(u64, usize)> = records
            .iter()
            .filter(|r| r.group == group)
            .map(|r| (r.rate_hz, r.payload_bytes))
            .collect();
        for (rate_hz, payload) in cells {
            let _ = writeln!(out, "\n### {rate_hz} Hz, {payload} B\n");
            let _ = writeln!(
                out,
                "|Operation|Implementation|Median|Range|P99|Server CPU|{}",
                loss_head
            );
            let _ = writeln!(out, "|---|---|---|---|---|---|{}", loss_rule);
            let cases: BTreeSet<&str> = records
                .iter()
                .filter(|r| r.group == group && r.payload_bytes == payload && r.rate_hz == rate_hz)
                .map(|r| r.case.as_str())
                .collect();
            let mut verdicts = Vec::new();
            let mut retried = Vec::new();
            for case in cases {
                let mut present: Vec<&Record> = records
                    .iter()
                    .filter(|r| {
                        r.case == case && r.payload_bytes == payload && r.rate_hz == rate_hz
                    })
                    .collect();
                present.sort_by(|a, b| a.median_us.partial_cmp(&b.median_us).unwrap());
                for r in &present {
                    let loss = if any_loss {
                        format!("{}|", loss_cell(r.loss_pct))
                    } else {
                        String::new()
                    };
                    let cpu = r
                        .server_cpu_pct
                        .map_or_else(|| "-".to_string(), |pct| format!("{pct:.1}%"));
                    let _ = writeln!(
                        out,
                        "|{}|{}|{:.1}|{:.1} to {:.1}|{:.1}|{cpu}|{loss}",
                        r.display,
                        r.implementation,
                        r.median_us,
                        r.min_median_us,
                        r.max_median_us,
                        r.p99_us,
                    );
                    if r.retries > 0 {
                        retried.push(format!(
                            "- `{}`/{}: {} attempt{} reported nothing and {} retried.",
                            r.display,
                            r.implementation,
                            r.retries,
                            if r.retries == 1 { "" } else { "s" },
                            if r.retries == 1 { "was" } else { "were" }
                        ));
                    }
                }
                if let Some(verdict) = verdict(&present) {
                    let display = present.first().map(|r| r.display.as_str()).unwrap_or(case);
                    verdicts.push(format!("- `{display}`: {verdict}"));
                }
            }
            if !verdicts.is_empty() {
                let _ = writeln!(out, "\n{}", verdicts.join("\n"));
            }
            if !retried.is_empty() {
                let _ = writeln!(out, "{}", retried.join("\n"));
            }
        }
    }

    let payloads = list(
        records
            .iter()
            .map(|r| r.payload_bytes)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|p| format!("{p} B")),
    );
    let rates = list(
        records
            .iter()
            .map(|r| r.rate_hz)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|r| r.to_string()),
    );
    let subscribers = if records.iter().any(|r| r.case == "fanout") {
        format!("more than {} subscribers, ", catalog::FANOUT)
    } else {
        "more than one subscriber, ".to_string()
    };
    let _ = writeln!(
        out,
        "\nNot measured: {subscribers}rates other than {rates} Hz, payloads other than {payloads}, \
traffic that crosses a network, memory."
    );

    out
}

/// `a`, `a and b`, or `a, b and c`.
fn list(items: impl IntoIterator<Item = String>) -> String {
    let items: Vec<String> = items.into_iter().collect();
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// # Errors
///
/// Returns any error from writing `path`.
pub fn write_markdown(
    path: &Path,
    conditions: &Conditions,
    records: &[Record],
) -> anyhow::Result<()> {
    Ok(std::fs::write(path, markdown(conditions, records))?)
}

/// Write the machine-readable record of the run.
///
/// # Errors
///
/// Returns any error from writing `path`.
pub fn write_json(path: &Path, conditions: &Conditions, records: &[Record]) -> anyhow::Result<()> {
    let cases: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "case": r.case,
                "group": r.group,
                "impl": r.implementation,
                "implementation_version": r.implementation_version,
                "payload_bytes": r.payload_bytes,
                "rate_hz": r.rate_hz,
                "runs": r.runs,
                "samples": r.samples,
                "p0_us": r.p0_us,
                "median_us": r.median_us,
                "p80_us": r.p80_us,
                "p90_us": r.p90_us,
                "p95_us": r.p95_us,
                "p99_us": r.p99_us,
                "p999_us": r.p999_us,
                "max_us": r.max_us,
                "loss_pct": r.loss_pct,
                "spread_pct": r.spread_pct,
                "min_median_us": r.min_median_us,
                "max_median_us": r.max_median_us,
                "achieved_hz": r.achieved_hz,
                "retries": r.retries,
                "server_cpu_pct": r.server_cpu_pct,
            })
        })
        .collect();
    let document = serde_json::json!({
        "schema": 6,
        "conditions": {
            "os": conditions.os,
            "kernel": conditions.kernel,
            "cpu": conditions.cpu,
            "governor": conditions.governor,
            "boost": conditions.boost,
            "loadavg": conditions.loadavg,
            "commit": conditions.commit,
            "date": conditions.date,
            "cores": conditions.cores,
            "pinning": conditions.pinning,
            "rates_hz": conditions.rates,
            "samples": conditions.samples,
            "warmup": conditions.warmup,
            "reps": conditions.reps,
        },
        "implementations": conditions.implementations,
        "cases": cases,
    });
    Ok(std::fs::write(
        path,
        serde_json::to_string_pretty(&document)?,
    )?)
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
            rate_hz: 500,
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
            retries: 0,
            server_cpu_pct: Some(6.8),
        }
    }

    #[test]
    fn the_header_names_the_day_the_machine_state_and_the_pinning() {
        let out = markdown(&Conditions::sample(), &[record("publish", "tarwyn", 30.0)]);
        assert!(out.contains("|date|2026-09-21|"), "{out}");
        assert!(out.contains("12 logical cpus"), "{out}");
        assert!(
            out.contains("governor powersave, boost on, load 0.54"),
            "{out}"
        );
        assert!(out.contains("|pinning|publisher on cpu 3"), "{out}");
    }

    #[test]
    fn a_retried_row_says_so_under_its_table() {
        let mut r = record("publish", "tarwyn", 30.0);
        r.retries = 1;
        let out = markdown(&Conditions::sample(), &[r]);
        assert!(
            out.contains("- `publish`/tarwyn: 1 attempt reported nothing and was retried."),
            "{out}"
        );
    }

    #[test]
    fn server_cpu_is_a_percent_to_one_decimal_or_a_dash() {
        let out = markdown(&Conditions::sample(), &[record("publish", "tarwyn", 30.0)]);
        assert!(out.contains("|6.8%|"), "{out}");
        let mut r = record("publish", "tarwyn", 30.0);
        r.server_cpu_pct = None;
        let out = markdown(&Conditions::sample(), &[r]);
        assert!(out.contains("|-|"), "{out}");
    }

    #[test]
    fn the_report_ends_by_saying_what_it_did_not_measure() {
        let out = markdown(&Conditions::sample(), &[record("publish", "tarwyn", 30.0)]);
        assert!(
            out.contains(
                "Not measured: more than one subscriber, rates other than 500 Hz, payloads other than 96 B"
            ),
            "{out}"
        );
        assert!(out.contains("### 500 Hz, 96 B"), "{out}");
    }

    #[test]
    fn the_date_arithmetic_matches_known_days() {
        assert_eq!(super::date_from_unix_days(0), "1970-01-01");
        assert_eq!(super::date_from_unix_days(19_723), "2024-01-01");
        assert_eq!(super::date_from_unix_days(20_718), "2026-09-22");
    }

    #[test]
    fn the_matrix_has_a_cell_for_every_case_and_implementation() {
        let records = vec![
            record("publish", "tarwyn", 34.1),
            record("publish", "ntcore", 49.5),
        ];
        let out = markdown(&Conditions::sample(), &records);
        assert!(out.contains("publish"), "the matrix must list publish");
        assert!(out.contains("|34.1|"), "cells carry the median");
        assert!(out.contains("|49.5|"));
    }

    #[test]
    fn an_implementation_without_a_record_reads_as_absent() {
        let records = vec![record("publish", "tarwyn", 71.2)];
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
        let mut ours = record("publish", "tarwyn", 36.03);
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
        let mut fast = record("publish", "tarwyn", 35.33);
        fast.min_median_us = 34.21;
        fast.max_median_us = 35.33;
        let out = markdown(&Conditions::sample(), &[slow, fast]);
        assert!(
            out.contains("- `publish`: tarwyn 1.5x faster than ntcore on the median"),
            "{out}"
        );
    }

    #[test]
    fn the_tail_is_judged_on_its_own_beside_the_median() {
        let mut close = record("publish", "ntcore", 38.53);
        close.min_median_us = 38.46;
        close.max_median_us = 41.18;
        close.p99_us = 190.0;
        let mut ours = record("publish", "tarwyn", 36.03);
        ours.min_median_us = 34.72;
        ours.max_median_us = 39.39;
        ours.p99_us = 66.0;
        let out = markdown(&Conditions::sample(), &[close, ours]);
        assert!(
            out.contains("tarwyn and ntcore within noise on the median, tarwyn 2.9x on the p99."),
            "a median tie still reports the tail: {out}"
        );
    }

    #[test]
    fn the_range_column_carries_the_lowest_and_highest_run() {
        let mut r = record("publish", "tarwyn", 30.0);
        r.min_median_us = 28.0;
        r.max_median_us = 34.0;
        let out = markdown(&Conditions::sample(), &[r]);
        assert!(out.contains("|30.0|28.0 to 34.0|"), "{out}");
    }

    #[test]
    fn the_loss_column_appears_only_when_something_was_lost() {
        let out = markdown(&Conditions::sample(), &[record("publish", "tarwyn", 30.0)]);
        assert!(!out.contains("Loss"), "{out}");
        let mut lossy = record("publish", "tarwyn", 30.0);
        lossy.loss_pct = 0.5;
        let out = markdown(&Conditions::sample(), &[lossy]);
        assert!(out.contains("|Loss|"), "{out}");
        assert!(out.contains("|0.50%|"), "{out}");
    }

    #[test]
    fn an_unknown_release_or_cpu_is_left_out_rather_than_printed() {
        let mut bare = Conditions::sample();
        bare.kernel = "unknown".to_string();
        bare.cpu = "unknown".to_string();
        let out = markdown(&bare, &[record("publish", "tarwyn", 34.1)]);
        assert!(out.contains("|machine|linux, 12 logical cpus|"), "{out}");
        assert!(
            !out.contains("unknown"),
            "the report never prints unknown: {out}"
        );
    }

    #[test]
    fn the_testbed_block_names_the_machine_the_numbers_came_from() {
        let out = markdown(&Conditions::sample(), &[record("publish", "tarwyn", 34.1)]);
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
        let err =
            check_achieved_rate(&[slow]).expect_err("a row at 430 of 500 Hz measured a backlog");
        assert!(err.to_string().contains("430"), "{err}");
    }

    #[test]
    fn a_row_at_its_rate_passes() {
        check_achieved_rate(&[record("publish", "ntcore", 34.1)])
            .expect("500 Hz of 500 asked is the whole rate");
    }

    #[test]
    fn a_malformed_row_is_rejected_with_its_line_number() {
        let short = "ROW\tpublish\ttarwyn\t0.1.0\t96\t500\t34.10\t10.0\t20.0";
        let err = parse_row_line(short, 42).expect_err("too few fields must error");
        assert!(
            err.to_string().contains("42"),
            "the error must name the line number: {err}"
        );
    }

    #[test]
    fn a_row_without_a_version_is_rejected() {
        let row = "ROW\tpublish\tntcore\t\t96\t500\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00\t3000\t500.0";
        let err = parse_row_line(row, 7).expect_err("an unversioned row is not comparable");
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn a_full_row_parses_every_field() {
        let row = "ROW\tpublish\tntcore\t2027.0.0\t96\t500\t51.25\t34.92\t62.56\t67.55\t72.28\t867.93\t1843.47\t3213.57\t0.00\t3000\t499.4";
        let parsed = parse_row_line(row, 5).expect("a full row must parse");
        assert_eq!(parsed.case, "publish");
        assert_eq!(parsed.rate_hz, 500);
        assert_eq!(parsed.implementation, "ntcore");
        assert_eq!(parsed.version, "2027.0.0");
        assert_eq!(parsed.median_us, 51.25);
        assert_eq!(parsed.p99_us, 867.93);
        assert_eq!(parsed.achieved_hz, 499.4);
    }
}
