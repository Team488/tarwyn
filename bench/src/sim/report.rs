//! The `bench sim` section of `results.json` and `RESULTS.md`.

use crate::report::{existing_json, splice};
use crate::sim::sweep::{Headroom, StepResult};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// The run a report describes.
#[derive(Debug, Clone)]
pub struct Meta {
    pub profile: String,
    /// The profile's own record of where it came from.
    pub source: Json,
    pub warmup: f64,
    pub seconds: f64,
    pub reps: u32,
    pub max_multiplier: u32,
    /// `(implementation, version)` for every implementation run.
    pub versions: Vec<(String, String)>,
    pub machine: String,
    pub date: String,
    pub commit: String,
}

/// A latency in µs to one decimal below 1 ms, otherwise in ms.
pub fn duration(us: f64) -> String {
    if us < 1000.0 {
        format!("{us:.1} µs")
    } else {
        format!("{:.1} ms", us / 1000.0)
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

/// One row's cells for `class`: the reps' median p50 and median p99.
fn class_cell(results: &[&StepResult], class: &str) -> String {
    let stats: Vec<_> = results
        .iter()
        .filter_map(|result| result.classes.get(class))
        .filter(|stats| stats.received > 0)
        .collect();
    match (
        median(stats.iter().map(|s| s.p50_us).collect()),
        median(stats.iter().map(|s| s.p99_us).collect()),
    ) {
        (Some(p50), Some(p99)) => format!("{} / {}", duration(p50), duration(p99)),
        _ => "-".to_string(),
    }
}

/// The verdict most reps reached, and how many reached it.
fn verdict_cell(results: &[&StepResult]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for result in results {
        let text = result.verdict.text();
        let key = if text == "pass" {
            text
        } else {
            text.split(':').next().unwrap_or("").to_string()
        };
        *counts.entry(key).or_default() += 1;
    }
    let (most, count) = counts
        .iter()
        .max_by_key(|(text, count)| (**count, text.as_str() == "pass"))
        .map(|(text, count)| (text.clone(), *count))
        .unwrap_or_default();
    let detail = results
        .iter()
        .map(|result| result.verdict.text())
        .find(|text| text.starts_with(&most) && text != &most)
        .map(|text| text[most.len()..].to_string())
        .unwrap_or_default();
    format!("{most}{detail} ({count} of {})", results.len())
}

/// The cells after the classes: loss, staleness, connect times, CPU, memory
/// and the verdict.
fn tail_cells(results: &[&StepResult]) -> String {
    let worst = |pick: &dyn Fn(&crate::sim::sweep::ClassStats) -> Option<f64>| {
        median(
            results
                .iter()
                .filter_map(|result| result.classes.values().filter_map(pick).reduce(f64::max))
                .collect(),
        )
    };
    let loss = worst(&|stats| stats.loss_pct).map_or("-".into(), |pct| {
        if pct == 0.0 {
            "0%".into()
        } else {
            format!("{pct:.2}%")
        }
    });
    let stale = worst(&|stats| stats.stale_p99_us).map_or("-".into(), duration);
    let conns: Vec<_> = results
        .iter()
        .flat_map(|result| result.conn.iter())
        .collect();
    let connect = if conns.iter().any(|conn| conn.values_ms.is_none()) {
        "timeout".to_string()
    } else {
        match (
            median(conns.iter().filter_map(|c| c.announced_ms).collect()),
            median(conns.iter().filter_map(|c| c.values_ms).collect()),
        ) {
            (Some(announced), Some(values)) => format!("{announced:.1} / {values:.1} ms"),
            _ => "-".into(),
        }
    };
    let cpu = median(results.iter().filter_map(|r| r.cpu_pct).collect())
        .map_or("-".into(), |pct| format!("{pct:.1}%"));
    let rss = median(
        results
            .iter()
            .filter_map(|r| r.rss_kb.map(|kb| kb as f64))
            .collect(),
    )
    .map_or("-".into(), |kb| format!("{:.1} MB", kb / 1024.0));
    format!(
        "{loss}|{stale}|{connect}|{cpu}|{rss}|{}|",
        verdict_cell(results)
    )
}

const HEAD: &str = "|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|";

/// The rows of one table: one per step setting and implementation, folding
/// the reps.
fn rows(results: &[&StepResult], first: impl Fn(&StepResult) -> String) -> String {
    let mut groups: Vec<(String, String, Vec<&StepResult>)> = Vec::new();
    for result in results {
        let key = (first(result), result.step.implementation.clone());
        match groups
            .iter_mut()
            .find(|(lead, implementation, _)| (lead, implementation) == (&key.0, &key.1))
        {
            Some((_, _, members)) => members.push(result),
            None => groups.push((key.0, key.1, vec![result])),
        }
    }
    let mut out = String::new();
    for (lead, implementation, members) in groups {
        let _ = writeln!(
            out,
            "|{lead}|{implementation}|{}|{}|{}|{}",
            class_cell(&members, "camera"),
            class_cell(&members, "struct"),
            class_cell(&members, "slow"),
            tail_cells(&members)
        );
    }
    out
}

/// The markdown section for a run.
pub fn section(meta: &Meta, results: &[StepResult], headroom: &[Headroom]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## Robot load: {}\n", meta.profile);
    let source = |key: &str| {
        meta.source
            .get(key)
            .and_then(Json::as_str)
            .filter(|text| !text.is_empty())
    };
    let origin = match (source("kind"), source("commit"), source("mode")) {
        (Some("sim-capture"), Some(commit), Some(mode)) => {
            format!("The profile was captured from {commit} in simulation, in {mode}. ")
        }
        (Some("sim-capture"), Some(commit), None) => {
            format!("The profile was captured from {commit} in simulation. ")
        }
        _ => String::new(),
    };
    let versions: Vec<String> = meta
        .versions
        .iter()
        .map(|(name, version)| format!("{name}={version}"))
        .collect();
    let _ = writeln!(
        out,
        "{origin}Run {} on {}, commit {}, with {} s of warmup and {} s measured per step and {} reps. Implementations: {}. [BENCHMARK.md](BENCHMARK.md#bench-sim) explains the columns and verdicts.\n",
        meta.date,
        meta.machine,
        meta.commit,
        meta.warmup,
        meta.seconds,
        meta.reps,
        versions.join(", ")
    );
    if !headroom.is_empty() {
        let _ = writeln!(
            out,
            "### Headroom\n\n|Implementation|Budget|Passed up to|Stopped by|\n|---|---|---|---|"
        );
        for entry in headroom {
            let passed = entry.passed.map_or("-".to_string(), |m| format!("{m}x"));
            let stopped = entry.stopped.clone().unwrap_or_else(|| {
                format!(
                    "did not break by {}x",
                    entry.passed.unwrap_or(meta.max_multiplier)
                )
            });
            let _ = writeln!(
                out,
                "|{}|{}|{passed}|{stopped}|",
                entry.implementation,
                entry.budget.as_str()
            );
        }
        out.push('\n');
    }
    let mut budgets: Vec<_> = results
        .iter()
        .filter(|r| r.step.axis == "rate")
        .map(|r| r.step.budget)
        .collect();
    budgets.dedup();
    budgets.sort();
    budgets.dedup();
    for budget in budgets {
        let rate: Vec<&StepResult> = results
            .iter()
            .filter(|r| r.step.axis == "rate" && r.step.budget == budget)
            .collect();
        let _ = writeln!(
            out,
            "### Rate, budget {}\n\n|Rate|Implementation{HEAD}\n|---|---|---|---|---|---|---|---|---|---|---|",
            budget.as_str()
        );
        out.push_str(&rows(&rate, |r| format!("{}x", r.step.multiplier)));
        out.push('\n');
    }
    let clients: Vec<&StepResult> = results
        .iter()
        .filter(|r| r.step.axis == "clients")
        .collect();
    if !clients.is_empty() {
        let _ = writeln!(
            out,
            "### Clients\n\n|Extra dashboards|Implementation{HEAD}\n|---|---|---|---|---|---|---|---|---|---|---|"
        );
        out.push_str(&rows(&clients, |r| format!("+{}", r.step.extra)));
        out.push('\n');
    }
    out
}

/// The `sim` object `results.json` keeps for a run.
pub fn to_json(meta: &Meta, results: &[StepResult], headroom: &[Headroom]) -> Json {
    let steps: Vec<Json> = results
        .iter()
        .map(|result| {
            let classes: serde_json::Map<String, Json> = result
                .classes
                .iter()
                .map(|(class, s)| {
                    (
                        class.clone(),
                        json!({"p50_us": s.p50_us, "p99_us": s.p99_us, "max_us": s.max_us,
                            "allow_us": s.allow_us, "delivered": s.delivered,
                            "loss_pct": s.loss_pct, "stale_p99_us": s.stale_p99_us,
                            "received": s.received}),
                    )
                })
                .collect();
            json!({
                "axis": result.step.axis,
                "impl": result.step.implementation,
                "budget": result.step.budget.as_str(),
                "multiplier": result.step.multiplier,
                "extra_dashboards": result.step.extra,
                "rep": result.step.rep,
                "verdict": result.verdict.text(),
                "cpu_pct": result.cpu_pct,
                "rss_kb": result.rss_kb,
                "hwm_kb": result.hwm_kb,
                "pacing": result.pacing.map(|[blind, hit, early, late]|
                    json!({"blind": blind, "hit": hit, "early": early, "late": late})),
                "connect": result.conn.iter().map(|c|
                    json!({"announced_ms": c.announced_ms, "values_ms": c.values_ms})).collect::<Vec<_>>(),
                "classes": classes,
            })
        })
        .collect();
    json!({
        "profile": meta.profile,
        "source": meta.source,
        "run": {"date": meta.date, "machine": meta.machine, "commit": meta.commit,
            "warmup_s": meta.warmup, "seconds": meta.seconds, "reps": meta.reps},
        "implementations": meta.versions.iter().map(|(name, version)| format!("{name}={version}")).collect::<Vec<_>>(),
        "headroom": headroom.iter().map(|entry| json!({
            "impl": entry.implementation, "budget": entry.budget.as_str(),
            "passed_multiplier": entry.passed, "stopped_by": entry.stopped})).collect::<Vec<_>>(),
        "steps": steps,
    })
}

/// Put the run into `results.json` and `RESULTS.md`, leaving their latency
/// sections alone. A schema 6 record's `cases` becomes schema 7's `latency`.
///
/// # Errors
///
/// Returns any error from writing either file.
pub fn write(
    json_path: &Path,
    markdown_path: &Path,
    meta: &Meta,
    results: &[StepResult],
    headroom: &[Headroom],
) -> anyhow::Result<()> {
    let mut document = existing_json(json_path);
    if let Some(cases) = document.remove("cases") {
        document.entry("latency").or_insert(cases);
    }
    document.insert("schema".into(), json!(7));
    document.insert("sim".into(), to_json(meta, results, headroom));
    std::fs::write(
        json_path,
        serde_json::to_string_pretty(&Json::Object(document))?,
    )?;
    let existing = std::fs::read_to_string(markdown_path).unwrap_or_default();
    std::fs::write(
        markdown_path,
        splice(&existing, Some(&section(meta, results, headroom))),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::sweep::{Budget, ClassStats, Headroom, Step, Verdict};

    fn meta() -> Meta {
        Meta {
            profile: "tiny".into(),
            source: serde_json::json!({"kind": "hand-written"}),
            warmup: 5.0,
            seconds: 30.0,
            reps: 3,
            max_multiplier: 16,
            versions: vec![("tarwyn".into(), "0.1.1".into())],
            machine: "AMD Ryzen 5 5600X".into(),
            date: "2026-10-16".into(),
            commit: "abc1234".into(),
        }
    }

    fn result(implementation: &str, multiplier: u32, p99: f64, verdict: Verdict) -> StepResult {
        let stats = ClassStats {
            p50_us: 30.0,
            p99_us: p99,
            allow_us: 40_000.0,
            max_us: p99 * 2.0,
            delivered: Some(1.0),
            loss_pct: Some(0.0),
            stale_p99_us: None,
            received: 100,
        };
        StepResult {
            step: Step {
                axis: "rate".into(),
                implementation: implementation.into(),
                budget: Budget::None,
                multiplier,
                extra: 0,
                rep: 1,
            },
            classes: [("struct".to_string(), stats)].into(),
            conn: Vec::new(),
            cpu_pct: Some(4.0),
            rss_kb: Some(9_000),
            hwm_kb: Some(9_500),
            pacing: None,
            verdict,
        }
    }

    #[test]
    fn durations_switch_to_milliseconds_at_one_millisecond() {
        assert_eq!(duration(999.94), "999.9 µs");
        assert_eq!(duration(1_500.0), "1.5 ms");
    }

    #[test]
    fn the_headroom_table_names_the_last_pass_and_what_stopped_it() {
        let headroom = [
            Headroom {
                implementation: "tarwyn".into(),
                budget: Budget::None,
                passed: Some(4),
                stopped: Some("8x: broke: struct p99 52.0 ms over 40.0 ms".into()),
            },
            Headroom {
                implementation: "ntcore-2027".into(),
                budget: Budget::None,
                passed: Some(16),
                stopped: None,
            },
            Headroom {
                implementation: "ntcore-2026".into(),
                budget: Budget::Quarter,
                passed: None,
                stopped: Some("1x: broke: robot exited".into()),
            },
        ];
        let text = section(
            &meta(),
            &[result("tarwyn", 1, 300.0, Verdict::Pass)],
            &headroom,
        );
        assert!(
            text.contains("|tarwyn|none|4x|8x: broke: struct p99 52.0 ms over 40.0 ms|"),
            "{text}"
        );
        assert!(
            text.contains("|ntcore-2027|none|16x|did not break by 16x|"),
            "{text}"
        );
        assert!(
            text.contains("|ntcore-2026|25pct|-|1x: broke: robot exited|"),
            "{text}"
        );
    }

    #[test]
    fn a_step_row_carries_the_median_of_its_reps() {
        let mut slow = result("tarwyn", 2, 900.0, Verdict::Pass);
        slow.step.rep = 2;
        let mut slower = result("tarwyn", 2, 1_200.0, Verdict::Broke("x".into()));
        slower.step.rep = 3;
        let rows = [result("tarwyn", 2, 300.0, Verdict::Pass), slow, slower];
        let text = section(&meta(), &rows, &[]);
        assert!(text.contains("|2x|tarwyn|-|30.0 µs / 900.0 µs|"), "{text}");
        assert!(text.contains("|pass (2 of 3)|"), "{text}");
    }

    #[test]
    fn a_class_that_received_nothing_shows_a_dash() {
        let mut empty = result("tarwyn", 1, 0.0, Verdict::Pass);
        empty.classes.get_mut("struct").unwrap().received = 0;
        let text = section(&meta(), &[empty], &[]);
        assert!(text.contains("|1x|tarwyn|-|-|-|"), "{text}");
    }

    #[test]
    fn an_older_latency_record_moves_to_its_schema_7_key() {
        let dir = std::env::temp_dir().join(format!("bench-sim-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (json, md) = (dir.join("results.json"), dir.join("RESULTS.md"));
        std::fs::write(&json, r#"{"schema": 6, "cases": [{"case": "publish"}]}"#).unwrap();
        write(&json, &md, &meta(), &[], &[]).unwrap();
        let doc: Json = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert!(doc.get("cases").is_none(), "{doc}");
        assert_eq!(doc["latency"][0]["case"], "publish");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_json_section_keeps_every_step() {
        let rows = [
            result("tarwyn", 1, 300.0, Verdict::Pass),
            result("tarwyn", 2, 300.0, Verdict::Pass),
        ];
        let doc = to_json(&meta(), &rows, &[]);
        assert_eq!(doc["steps"].as_array().unwrap().len(), 2);
        assert_eq!(doc["steps"][0]["classes"]["struct"]["p99_us"], 300.0);
        assert_eq!(doc["profile"], "tiny");
    }
}
