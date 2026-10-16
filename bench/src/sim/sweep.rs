//! Running `bench sim`: steps, CPU budgets, verdicts and the axes.

use crate::harness::now_nanos;
use crate::run::env::Env;
use crate::run::plan::python;
use crate::run::process::{
    Cores, Running, cpu_seconds, kill_group, memory_kb, spawn_logged, wait_for_port,
};
use crate::sim::measure::{self, ConnLine, Lines, Mode, PubLine};
use crate::sim::profile::Profile;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The port every step's server listens on.
const PORT: u16 = 5810;
/// The slice the `25pct` budget's quota is set on.
const SLICE: &str = "tarwyn-bench.slice";
/// How long roles have to start before the window opens: `uv` takes about two
/// seconds to start a pyntcore role.
const STARTUP_NTCORE: Duration = Duration::from_secs(4);
/// How long the Rust roles have to start before the window opens.
const STARTUP_TARWYN: Duration = Duration::from_secs(1);
/// The fewest values a receiver must expect of a class before its delivery
/// is judged, since a share of a handful says nothing.
const MIN_EXPECTED: f64 = 20.0;
/// How long a role may run past its window before it counts as stuck.
const OVERRUN: Duration = Duration::from_secs(30);

/// How much CPU the server and the robot share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Budget {
    /// The server cores, unlimited.
    None,
    /// One cpu, shared.
    OneCore,
    /// One cpu, shared, at a quarter of its time.
    Quarter,
}

impl Budget {
    pub fn as_str(self) -> &'static str {
        match self {
            Budget::None => "none",
            Budget::OneCore => "1core",
            Budget::Quarter => "25pct",
        }
    }
}

impl std::str::FromStr for Budget {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        match text {
            "none" => Ok(Budget::None),
            "1core" => Ok(Budget::OneCore),
            "25pct" => Ok(Budget::Quarter),
            other => Err(anyhow::anyhow!(
                "{other} is not a budget: none, 1core or 25pct"
            )),
        }
    }
}

/// One run of the profile against one implementation.
#[derive(Debug, Clone)]
pub struct Step {
    /// `rate` or `clients`.
    pub axis: String,
    pub implementation: String,
    pub budget: Budget,
    pub multiplier: u32,
    /// Copies of the spare dashboard added.
    pub extra: usize,
    pub rep: u32,
}

/// One class's latency and delivery, folded over every receiver of a step.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassStats {
    /// The median of the receivers' medians.
    pub p50_us: f64,
    /// The p99 of the receiver nearest its allowance, and that allowance.
    pub p99_us: f64,
    pub allow_us: f64,
    /// The worst receiver's largest latency.
    pub max_us: f64,
    /// The smallest share of expected values a full receiver got.
    pub delivered: Option<f64>,
    /// The largest share of values a full receiver lost, in percent.
    pub loss_pct: Option<f64>,
    /// The worst p99 latency a coalescing receiver saw.
    pub stale_p99_us: Option<f64>,
}

/// How a step ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Pass,
    Broke(String),
    /// A publisher fell behind, so the step says nothing about the server.
    GeneratorLimited(String),
}

impl Verdict {
    /// The verdict as the report prints it.
    pub fn text(&self) -> String {
        match self {
            Verdict::Pass => "pass".to_string(),
            Verdict::Broke(reason) => format!("broke: {reason}"),
            Verdict::GeneratorLimited(reason) => format!("generator-limited: {reason}"),
        }
    }
}

/// Everything one step measured.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step: Step,
    pub classes: BTreeMap<String, ClassStats>,
    pub conn: Vec<ConnLine>,
    /// The server's and the robot's CPU, as percent of one core.
    pub cpu_pct: Option<f64>,
    pub rss_kb: Option<u64>,
    pub hwm_kb: Option<u64>,
    /// tarwyn's predictor: blind, hit, early and late reads.
    pub pacing: Option<[u64; 4]>,
    pub verdict: Verdict,
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// Fold every receiver's `SIM` lines into one [`ClassStats`] per class.
pub fn fold_classes(lines: &Lines) -> BTreeMap<String, ClassStats> {
    let mut by_class: BTreeMap<String, Vec<&measure::SimLine>> = BTreeMap::new();
    for line in &lines.sims {
        by_class
            .entry(line.class.as_str().to_string())
            .or_default()
            .push(line);
    }
    by_class
        .into_iter()
        .map(|(class, sims)| {
            let full: Vec<&&measure::SimLine> =
                sims.iter().filter(|line| line.mode == Mode::All).collect();
            let timed: Vec<&&measure::SimLine> = if full.is_empty() {
                sims.iter().collect()
            } else {
                full.clone()
            };
            let nearest = timed.iter().max_by(|a, b| {
                (a.p99_us / a.allow_us.max(1.0)).total_cmp(&(b.p99_us / b.allow_us.max(1.0)))
            });
            let delivered = full
                .iter()
                .filter(|line| line.expected >= MIN_EXPECTED)
                .map(|line| line.received as f64 / line.expected)
                .reduce(f64::min);
            let loss_pct = full
                .iter()
                .map(|line| {
                    let total = line.received + line.lost;
                    if total == 0 {
                        0.0
                    } else {
                        100.0 * line.lost as f64 / total as f64
                    }
                })
                .reduce(f64::max);
            let stale_p99_us = sims
                .iter()
                .filter(|line| line.mode == Mode::Coalesce)
                .map(|line| line.p99_us)
                .reduce(f64::max);
            let stats = ClassStats {
                p50_us: median(&mut timed.iter().map(|line| line.p50_us).collect::<Vec<_>>()),
                p99_us: nearest.map_or(0.0, |line| line.p99_us),
                allow_us: nearest.map_or(0.0, |line| line.allow_us),
                max_us: timed.iter().map(|line| line.max_us).fold(0.0, f64::max),
                delivered: delivered.map(|share| (share * 100.0).round() / 100.0),
                loss_pct,
                stale_p99_us,
            };
            (class, stats)
        })
        .collect()
}

/// How a step ended. A publisher under 95% of its asked rate makes it
/// generator-limited. Otherwise it broke when a role exited, the server
/// dropped publishes, a class was delivered under 95%, or a class's p99 went
/// over its allowance, checked in that order.
pub fn verdict(
    classes: &BTreeMap<String, ClassStats>,
    pubs: &[PubLine],
    exited: &[String],
    dropped: u64,
) -> Verdict {
    if let Some(slow) = pubs
        .iter()
        .find(|line| (line.sent as f64) < line.asked * 0.95)
    {
        return Verdict::GeneratorLimited(format!(
            "{} sent {:.0}% of {}",
            slow.client,
            100.0 * slow.sent as f64 / slow.asked.max(1.0),
            slow.class.as_str()
        ));
    }
    if let Some(role) = exited.first() {
        return Verdict::Broke(format!("{role} exited"));
    }
    if dropped > 0 {
        return Verdict::Broke(format!("{dropped} dropped publishes"));
    }
    for (class, stats) in classes {
        if let Some(share) = stats.delivered
            && share < 0.95
        {
            return Verdict::Broke(format!("{class} delivered {:.0}%", share * 100.0));
        }
    }
    for (class, stats) in classes {
        if stats.p99_us > stats.allow_us {
            return Verdict::Broke(format!(
                "{class} p99 {:.1} ms over {:.1} ms",
                stats.p99_us / 1000.0,
                stats.allow_us / 1000.0
            ));
        }
    }
    Verdict::Pass
}

/// What `bench sim` was asked for.
#[derive(Debug)]
pub struct SimSettings {
    pub profile: PathBuf,
    pub axes: Vec<String>,
    pub implementations: Vec<String>,
    pub budgets: Vec<Budget>,
    pub reps: u32,
    pub warmup: f64,
    pub seconds: f64,
    pub max_multiplier: u32,
    pub clients: Vec<usize>,
    pub probes: u32,
    pub logs: PathBuf,
    pub pin: bool,
}

/// The last multiplier an implementation passed at under one budget, and
/// what stopped it.
#[derive(Debug, Clone)]
pub struct Headroom {
    pub implementation: String,
    pub budget: Budget,
    pub passed: Option<u32>,
    pub stopped: Option<String>,
}

/// Set the slice's CPU quota for `budget`, or clear it.
fn set_quota(budget: Budget) -> anyhow::Result<()> {
    let quota = match budget {
        Budget::Quarter => vec!["CPUQuota=25%", "CPUQuotaPeriodSec=5ms"],
        _ => vec!["CPUQuota="],
    };
    let status = Command::new("systemctl")
        .args(["--user", "set-property", "--runtime", SLICE])
        .args(&quota)
        .status()
        .map_err(|error| anyhow::anyhow!("systemctl is needed for CPU budgets: {error}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "systemctl --user set-property {SLICE} failed"
        ));
    }
    Ok(())
}

/// The program and arguments for `command`, inside the budget's slice when
/// it has a quota.
fn budgeted(budget: Budget, program: String, args: Vec<String>) -> (String, Vec<String>) {
    if budget != Budget::Quarter {
        return (program, args);
    }
    let mut wrapped = vec![
        "--user".to_string(),
        "--scope".into(),
        "--quiet".into(),
        format!("--slice={SLICE}"),
        program,
    ];
    wrapped.extend(args);
    ("systemd-run".to_string(), wrapped)
}

/// Wait for a role until `deadline`, and say why it failed if it did.
fn finish(name: &str, running: &mut Running, deadline: Instant) -> Option<String> {
    let mut child = running.take()?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return None,
            Ok(Some(_)) | Err(_) => return Some(name.to_string()),
            Ok(None) if Instant::now() >= deadline => {
                kill_group(&mut child);
                return Some(format!("{name} overran"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// The predictor tallies tarwyn logs at each connection close, summed.
fn pacing(log: &str) -> Option<[u64; 4]> {
    let mut total = [0u64; 4];
    let mut seen = false;
    for line in log.lines() {
        let Some(rest) = line.split("reads blind ").nth(1) else {
            continue;
        };
        let numbers: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect();
        if let [blind, hit, early, late] = numbers[..] {
            for (slot, value) in total.iter_mut().zip([blind, hit, early, late]) {
                *slot += value;
            }
            seen = true;
        }
    }
    seen.then_some(total)
}

/// Run one step and measure it.
///
/// # Errors
///
/// Returns an error when a process cannot be spawned, a log cannot be read,
/// or a CPU budget cannot be set.
pub fn run_step(
    env: &Env,
    settings: &SimSettings,
    cores: &Cores,
    step: &Step,
    server_override: Option<&Path>,
) -> anyhow::Result<StepResult> {
    let tarwyn = step.implementation == "tarwyn";
    let stem = format!(
        "{}_{}_{}_{}x_{}d_r{}",
        step.axis,
        step.implementation,
        step.budget.as_str(),
        step.multiplier,
        step.extra,
        step.rep
    );
    let log = |role: &str| settings.logs.join(format!("{stem}_{role}.log"));
    let server_cores = cores.server().map(str::to_string);
    let controller = match step.budget {
        Budget::None => server_cores,
        _ => server_cores.and_then(|list| list.split(',').next().map(str::to_string)),
    };
    let (publisher_core, _) = cores.probe(true);
    set_quota(step.budget)?;

    let startup = if tarwyn {
        STARTUP_TARWYN
    } else {
        STARTUP_NTCORE
    };
    let opens_at_ms = now_nanos() / 1_000_000 + startup.as_millis() as u64;
    let window_args = |role: &str| {
        vec![
            role.to_string(),
            "--profile".into(),
            settings.profile.display().to_string(),
            "--multiplier".into(),
            step.multiplier.to_string(),
            "--warmup".into(),
            settings.warmup.to_string(),
            "--seconds".into(),
            settings.seconds.to_string(),
            "--opens-at".into(),
            opens_at_ms.to_string(),
        ]
    };
    let rust_role = |role: &str| {
        let mut args = vec!["sim-role".to_string(), "--role".into()];
        args.extend(window_args(role));
        (env.exe.display().to_string(), args)
    };
    let python_role = |role: &str| {
        let (program, mut args) = python(env, &step.implementation);
        args.push(
            env.root
                .join("bench/python/src/sim_roles.py")
                .display()
                .to_string(),
        );
        args.extend(window_args(role));
        args.extend(["--port".into(), PORT.to_string()]);
        (program, args)
    };

    let mut controllers: Vec<(String, Running)> = Vec::new();
    let mut server = None;
    if tarwyn {
        let binary = server_override.unwrap_or(&env.server).display().to_string();
        let args = ["--log", "--bind", "127.0.0.1", "--port", &PORT.to_string()]
            .map(str::to_string)
            .to_vec();
        let (program, args) = budgeted(step.budget, binary, args);
        server = Some(spawn_logged(
            &program,
            &args,
            controller.as_deref(),
            &log("server"),
            &[],
        )?);
        if !wait_for_port(PORT, Instant::now() + Duration::from_secs(20)) {
            return Err(anyhow::anyhow!("tarwyn never listened on {PORT}"));
        }
        let (program, args) = rust_role("robot");
        let (program, args) = budgeted(step.budget, program, args);
        controllers.push((
            "robot".into(),
            spawn_logged(&program, &args, controller.as_deref(), &log("robot"), &[])?,
        ));
    } else {
        let (program, args) = python_role("robot");
        let (program, args) = budgeted(step.budget, program, args);
        controllers.push((
            "robot".into(),
            spawn_logged(&program, &args, controller.as_deref(), &log("robot"), &[])?,
        ));
        if !wait_for_port(PORT, Instant::now() + startup + Duration::from_secs(20)) {
            return Err(anyhow::anyhow!(
                "{} never listened on {PORT}",
                step.implementation
            ));
        }
    }
    let (program, args) = if tarwyn {
        rust_role("coprocessors")
    } else {
        python_role("coprocessors")
    };
    let mut coprocessors =
        spawn_logged(&program, &args, publisher_core, &log("coprocessors"), &[])?;
    let (program, mut args) = rust_role("dashboards");
    args.extend([
        "--extra".into(),
        step.extra.to_string(),
        "--probes".into(),
        settings.probes.to_string(),
    ]);
    let mut dashboards = spawn_logged(&program, &args, None, &log("dashboards"), &[])?;

    let stats = tarwyn.then(|| tarwyn_client::Client::connect("127.0.0.1"));
    let dropped = || {
        stats
            .as_ref()
            .and_then(|client| client.statistics())
            .map(|s| s.dropped_publishes)
    };
    let opens = Instant::now()
        + Duration::from_millis(opens_at_ms.saturating_sub(now_nanos() / 1_000_000))
        + Duration::from_secs_f64(settings.warmup);
    let pids: Vec<u32> = server
        .iter()
        .chain(controllers.iter().map(|(_, running)| running))
        .filter_map(Running::id)
        .collect();
    let cpu = || {
        pids.iter()
            .map(|pid| cpu_seconds(*pid))
            .sum::<Option<f64>>()
    };
    std::thread::sleep(opens.saturating_duration_since(Instant::now()));
    let (cpu_before, dropped_before) = (cpu(), dropped());
    std::thread::sleep(Duration::from_secs_f64(settings.seconds * 0.95));
    let (cpu_after, dropped_after) = (cpu(), dropped());
    let memory = pids
        .iter()
        .map(|pid| memory_kb(*pid))
        .try_fold((0, 0), |total, each| {
            let each = each?;
            Some((total.0 + each.0, total.1 + each.1))
        });
    drop(stats);

    let deadline = opens + Duration::from_secs_f64(settings.seconds) + OVERRUN;
    let mut exited: Vec<String> = Vec::new();
    for (name, running) in &mut controllers {
        exited.extend(finish(name, running, deadline));
    }
    exited.extend(finish("coprocessors", &mut coprocessors, deadline));
    exited.extend(finish("dashboards", &mut dashboards, deadline));
    if let Some(server) = server.as_mut()
        && server.finished()?
    {
        exited.push("server".into());
    }
    std::thread::sleep(Duration::from_millis(300));
    drop(server);
    set_quota(Budget::None)?;

    let mut lines = Lines::default();
    for role in ["robot", "coprocessors", "dashboards"] {
        let parsed = measure::parse(&std::fs::read_to_string(log(role)).unwrap_or_default());
        lines.sims.extend(parsed.sims);
        lines.pubs.extend(parsed.pubs);
        lines.conns.extend(parsed.conns);
    }
    let classes = fold_classes(&lines);
    let dropped = match (dropped_before, dropped_after) {
        (Some(before), Some(after)) => after.saturating_sub(before),
        _ => 0,
    };
    let verdict = verdict(&classes, &lines.pubs, &exited, dropped);
    let measured = settings.seconds * 0.95;
    Ok(StepResult {
        step: step.clone(),
        classes,
        conn: lines.conns,
        cpu_pct: match (cpu_before, cpu_after) {
            (Some(before), Some(after)) => Some(100.0 * (after - before) / measured),
            _ => None,
        },
        rss_kb: memory.map(|(rss, _)| rss),
        hwm_kb: memory.map(|(_, hwm)| hwm),
        pacing: if tarwyn {
            pacing(&std::fs::read_to_string(log("server")).unwrap_or_default())
        } else {
            None
        },
        verdict,
    })
}

/// Run every requested axis, printing a line per step, and return every
/// step's result and the headroom the rate axis found.
///
/// # Errors
///
/// Returns an error when the profile cannot be loaded or a step cannot run.
pub fn sim(settings: &SimSettings) -> anyhow::Result<(Vec<StepResult>, Vec<Headroom>)> {
    Profile::load(&settings.profile)?;
    let env = Env::discover()?;
    std::fs::create_dir_all(&settings.logs)?;
    let cores = Cores::pick(settings.pin);
    let mut results = Vec::new();
    let mut headroom = Vec::new();
    let run = |step: Step, results: &mut Vec<StepResult>| -> anyhow::Result<Verdict> {
        let result = run_step(&env, settings, &cores, &step, None)?;
        eprintln!(
            "{} {} {} {}x +{} r{}: {}",
            step.axis,
            step.implementation,
            step.budget.as_str(),
            step.multiplier,
            step.extra,
            step.rep,
            result.verdict.text()
        );
        let verdict = result.verdict.clone();
        results.push(result);
        Ok(verdict)
    };
    if settings.axes.iter().any(|axis| axis == "rate") {
        for &budget in &settings.budgets {
            let mut active: Vec<String> = settings.implementations.clone();
            let mut found: BTreeMap<String, Headroom> = active
                .iter()
                .map(|implementation| {
                    (
                        implementation.clone(),
                        Headroom {
                            implementation: implementation.clone(),
                            budget,
                            passed: None,
                            stopped: None,
                        },
                    )
                })
                .collect();
            let mut multiplier = 1;
            while !active.is_empty() && multiplier <= settings.max_multiplier {
                let mut verdicts: BTreeMap<String, Vec<Verdict>> = BTreeMap::new();
                for rep in 1..=settings.reps {
                    for implementation in &active {
                        let step = Step {
                            axis: "rate".into(),
                            implementation: implementation.clone(),
                            budget,
                            multiplier,
                            extra: 0,
                            rep,
                        };
                        verdicts
                            .entry(implementation.clone())
                            .or_default()
                            .push(run(step, &mut results)?);
                    }
                }
                active.retain(|implementation| {
                    let seen = &verdicts[implementation];
                    let passes = seen
                        .iter()
                        .filter(|verdict| **verdict == Verdict::Pass)
                        .count();
                    let entry = found
                        .get_mut(implementation)
                        .expect("every active implementation has an entry");
                    if passes * 2 > seen.len() {
                        entry.passed = Some(multiplier);
                        true
                    } else {
                        let reason = seen
                            .iter()
                            .find(|verdict| **verdict != Verdict::Pass)
                            .map(Verdict::text);
                        entry.stopped = reason.map(|reason| format!("{multiplier}x: {reason}"));
                        false
                    }
                });
                multiplier *= 2;
            }
            headroom.extend(found.into_values());
        }
    }
    if settings.axes.iter().any(|axis| axis == "clients") {
        for &extra in &settings.clients {
            for rep in 1..=settings.reps {
                for implementation in &settings.implementations {
                    let step = Step {
                        axis: "clients".into(),
                        implementation: implementation.clone(),
                        budget: Budget::None,
                        multiplier: 1,
                        extra,
                        rep,
                    };
                    run(step, &mut results)?;
                }
            }
        }
    }
    Ok((results, headroom))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::measure::SimLine;
    use crate::sim::profile::Class;

    fn sim(
        receiver: &str,
        class: Class,
        mode: Mode,
        received: u64,
        expected: f64,
        p99: f64,
        allow: f64,
    ) -> SimLine {
        SimLine {
            receiver: receiver.into(),
            class,
            mode,
            received,
            expected,
            lost: 0,
            p50_us: p99 / 4.0,
            p99_us: p99,
            max_us: p99 * 2.0,
            allow_us: allow,
        }
    }

    fn classes(sims: Vec<SimLine>) -> BTreeMap<String, ClassStats> {
        fold_classes(&Lines {
            sims,
            ..Lines::default()
        })
    }

    #[test]
    fn a_class_under_95_percent_delivered_breaks() {
        let folded = classes(vec![sim(
            "robot",
            Class::Camera,
            Mode::All,
            91,
            100.0,
            100.0,
            16_666.0,
        )]);
        assert_eq!(
            verdict(&folded, &[], &[], 0),
            Verdict::Broke("camera delivered 91%".into())
        );
    }

    #[test]
    fn a_p99_over_the_allowance_breaks() {
        let folded = classes(vec![sim(
            "viewer",
            Class::Struct,
            Mode::All,
            100,
            100.0,
            52_000.0,
            40_000.0,
        )]);
        let Verdict::Broke(reason) = verdict(&folded, &[], &[], 0) else {
            panic!("a p99 over its allowance must break")
        };
        assert!(
            reason.contains("struct p99 52.0 ms over 40.0 ms"),
            "{reason}"
        );
    }

    #[test]
    fn a_slow_publisher_marks_the_step_generator_limited() {
        let folded = classes(vec![sim(
            "robot",
            Class::Camera,
            Mode::All,
            10,
            100.0,
            1.0,
            10.0,
        )]);
        let pubs = [PubLine {
            client: "camera-front".into(),
            class: Class::Camera,
            asked: 1000.0,
            sent: 900,
        }];
        assert_eq!(
            verdict(&folded, &pubs, &[], 0),
            Verdict::GeneratorLimited("camera-front sent 90% of camera".into())
        );
    }

    #[test]
    fn an_exited_role_breaks_the_step() {
        assert_eq!(
            verdict(&BTreeMap::new(), &[], &["robot".to_string()], 0),
            Verdict::Broke("robot exited".into())
        );
    }

    #[test]
    fn rising_drops_break_the_step() {
        assert_eq!(
            verdict(&BTreeMap::new(), &[], &[], 3),
            Verdict::Broke("3 dropped publishes".into())
        );
    }

    #[test]
    fn a_class_expecting_only_a_few_values_is_not_judged_on_delivery() {
        let folded = classes(vec![sim(
            "viewer",
            Class::Slow,
            Mode::All,
            0,
            0.6,
            0.0,
            1e6,
        )]);
        assert_eq!(folded["slow"].delivered, None);
        assert_eq!(verdict(&folded, &[], &[], 0), Verdict::Pass);
    }

    #[test]
    fn a_healthy_step_passes() {
        let folded = classes(vec![sim(
            "robot",
            Class::Camera,
            Mode::All,
            100,
            100.0,
            300.0,
            16_666.0,
        )]);
        assert_eq!(verdict(&folded, &[], &[], 0), Verdict::Pass);
    }

    #[test]
    fn receivers_fold_to_the_worst_tail_and_the_median_middle() {
        let folded = classes(vec![
            sim("a", Class::Struct, Mode::All, 100, 100.0, 400.0, 40_000.0),
            sim("b", Class::Struct, Mode::All, 97, 100.0, 4_000.0, 60_000.0),
            sim("c", Class::Struct, Mode::All, 100, 100.0, 800.0, 40_000.0),
            sim(
                "d",
                Class::Struct,
                Mode::Coalesce,
                20,
                0.0,
                9_000.0,
                160_000.0,
            ),
        ]);
        let stats = &folded["struct"];
        assert_eq!(stats.p50_us, 200.0, "the median of the receivers' medians");
        assert_eq!(
            (stats.p99_us, stats.allow_us),
            (4_000.0, 60_000.0),
            "the receiver nearest its allowance"
        );
        assert_eq!(stats.delivered, Some(0.97));
        assert_eq!(stats.stale_p99_us, Some(9_000.0));
    }

    #[test]
    fn budgets_read_their_short_names() {
        assert_eq!("25pct".parse::<Budget>().unwrap(), Budget::Quarter);
        assert_eq!("1core".parse::<Budget>().unwrap(), Budget::OneCore);
        assert!("half".parse::<Budget>().is_err());
    }
}
