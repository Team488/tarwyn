//! Running one pair for a long time, to answer whether latency grows with it.

use super::env::Env;
use super::process::{Cores, spawn, spawn_with_env, wait_for_port};
use super::{Settings, noise_check};
use std::io;

use std::time::{Duration, Instant};

struct Window {
    received: u64,
    median_us: f64,
    p95_us: f64,
    max_us: f64,
    lost: u64,
}

fn parse_windows(log: &str) -> Vec<Window> {
    log.lines()
        .filter_map(|line| {
            let mut f = line.strip_prefix("WINDOW\t")?.split('\t');
            let _index = f.next()?;
            Some(Window {
                received: f.next()?.parse().ok()?,
                median_us: f.next()?.parse().ok()?,
                p95_us: f.next()?.parse().ok()?,
                max_us: f.next()?.parse().ok()?,
                lost: f.next()?.trim().parse().ok()?,
            })
        })
        .collect()
}

/// Whether latency grew over the run, which is what a queue looks like.
///
/// Compares the first quarter of windows against the last. A stream that
/// queues looks fine for the first thousand samples and worse forever after,
/// so the average over the whole run would hide it.
fn drift_verdict(windows: &[Window]) -> String {
    let silent = windows.iter().filter(|w| w.received == 0).count();
    if silent > 0 {
        return format!(
            "FAIL: {silent} of {} windows received nothing, so the stream stopped",
            windows.len()
        );
    }
    if windows.len() < 4 {
        return "too few windows to judge drift".to_string();
    }
    let quarter = (windows.len() / 4).max(1);
    let mean = |slice: &[Window], pick: fn(&Window) -> f64| {
        slice.iter().map(pick).sum::<f64>() / slice.len() as f64
    };
    let first = &windows[..quarter];
    let last = &windows[windows.len() - quarter..];
    let (first_median, last_median) = (mean(first, |w| w.median_us), mean(last, |w| w.median_us));
    let (first_p95, last_p95) = (mean(first, |w| w.p95_us), mean(last, |w| w.p95_us));
    let lost: u64 = windows.iter().map(|w| w.lost).sum();
    let grew = last_median > first_median * 1.25 || last_p95 > first_p95 * 1.25;
    format!(
        "first {quarter} windows: median {first_median:.2} us, p95 {first_p95:.2} us\n\
         last  {quarter} windows: median {last_median:.2} us, p95 {last_p95:.2} us\n\
         drift: median {:+.1}%, p95 {:+.1}%, {lost} lost overall\n{}",
        100.0 * (last_median - first_median) / first_median,
        100.0 * (last_p95 - first_p95) / first_p95,
        if grew {
            "FAIL: latency grew with time, which is what a queue looks like"
        } else {
            "PASS: latency did not grow with time"
        }
    )
}

/// The server's resident memory, in kB, if the platform reports it.
fn server_rss_kb(pid: u32) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmRSS"))
        .and_then(|line| line.split_whitespace().nth(1)?.parse().ok())
}

/// Run one publisher and one subscriber for `duration`, reporting per window.
///
/// The server's resident memory is sampled alongside, since a queue that costs
/// latency usually costs memory too.
///
/// # Errors
///
/// Returns any error from spawning a process or reading the subscriber's log,
/// and an error if the run recorded no windows or latency grew with time.
pub fn soak(settings: &Settings, duration: Duration, window: Duration) -> io::Result<()> {
    let env = Env::discover()?;
    std::fs::create_dir_all(&settings.rows_dir)?;
    let cores = Cores::pick(settings.pin);
    let server_core = cores.server();
    let (pub_core, sub_core) = cores.probe(true);
    let payload = settings.payloads.first().copied().unwrap_or(96);
    let seconds = duration.as_secs();
    let server_log = settings.rows_dir.join("soak_server.log");
    let sub_log = settings.rows_dir.join("soak_subscriber.log");
    let pub_log = settings.rows_dir.join("soak_publisher.log");

    eprintln!(
        "soaking for {seconds}s at {} Hz, {payload} B payload, reporting every {}s",
        settings.rate_hz,
        window.as_secs()
    );
    noise_check();

    let server = spawn(
        &env.server.display().to_string(),
        &[],
        server_core,
        &server_log,
        settings,
    )?;
    if !wait_for_port(5810, Instant::now() + Duration::from_secs(20)) {
        return Err(io::Error::other("the server never listened on 5810"));
    }
    let server_pid = server.id();

    let one = |role: &str, extra: Vec<String>| {
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

    let mut subscriber = spawn_with_env(
        &env.exe.display().to_string(),
        &one(
            "subscriber",
            vec![
                "--samples".into(),
                (settings.rate_hz * (seconds + 60)).to_string(),
            ],
        ),
        sub_core,
        &sub_log,
        settings,
        &[
            ("BENCH_WINDOW_SECS", window.as_secs().to_string()),
            ("BENCH_DEADLINE_SECS", seconds.to_string()),
        ],
    )?;
    std::thread::sleep(Duration::from_secs(1));

    let publisher = spawn(
        &env.exe.display().to_string(),
        &one(
            "publisher",
            vec![
                "--rate".into(),
                settings.rate_hz.to_string(),
                "--count".into(),
                (settings.rate_hz * (seconds + 30)).to_string(),
            ],
        ),
        pub_core,
        &pub_log,
        settings,
    )?;

    let mut rss = Vec::new();
    let ends = Instant::now() + duration + Duration::from_secs(90);
    loop {
        if subscriber.finished()? {
            break;
        }
        if Instant::now() >= ends {
            break;
        }
        if let Some(pid) = server_pid
            && let Some(kb) = server_rss_kb(pid)
        {
            rss.push(kb);
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    drop(publisher);
    drop(subscriber);
    drop(server);

    let windows = parse_windows(&std::fs::read_to_string(&sub_log)?);
    if windows.is_empty() {
        return Err(io::Error::other(format!(
            "no windows recorded, see {}",
            sub_log.display()
        )));
    }

    println!("\n|Window|Received|Median (us)|P95 (us)|Max (us)|Lost|");
    println!("|---|---|---|---|---|---|");
    for (index, w) in windows.iter().enumerate() {
        println!(
            "|{index}|{}|{:.2}|{:.2}|{:.2}|{}|",
            w.received, w.median_us, w.p95_us, w.max_us, w.lost
        );
    }
    let verdict = drift_verdict(&windows);
    println!("\n{verdict}");
    if let (Some(first), Some(last)) = (rss.first(), rss.last()) {
        println!(
            "\nserver RSS: {first} kB to {last} kB ({:+.1}%)",
            100.0 * (*last as f64 - *first as f64) / *first as f64
        );
    }
    if verdict.contains("FAIL") {
        return Err(io::Error::other("the soak failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{drift_verdict, parse_windows};

    fn window(median: f64, p95: f64, received: u64) -> super::Window {
        super::Window {
            received,
            median_us: median,
            p95_us: p95,
            max_us: p95 * 4.0,
            lost: 0,
        }
    }

    #[test]
    fn a_steady_stream_passes() {
        let windows: Vec<_> = (0..8).map(|_| window(40.0, 90.0, 5000)).collect();
        assert!(
            drift_verdict(&windows).contains("PASS"),
            "{}",
            drift_verdict(&windows)
        );
    }

    #[test]
    fn latency_that_grows_with_time_fails() {
        let mut windows: Vec<_> = (0..4).map(|_| window(40.0, 90.0, 5000)).collect();
        windows.extend((0..4).map(|_| window(80.0, 200.0, 5000)));
        assert!(
            drift_verdict(&windows).contains("FAIL"),
            "{}",
            drift_verdict(&windows)
        );
    }

    #[test]
    fn a_window_that_received_nothing_means_the_stream_stopped() {
        let mut windows: Vec<_> = (0..8).map(|_| window(40.0, 90.0, 5000)).collect();
        windows[5] = window(0.0, 0.0, 0);
        let verdict = drift_verdict(&windows);
        assert!(verdict.contains("stream stopped"), "{verdict}");
    }

    #[test]
    fn only_window_lines_are_parsed() {
        let log = "subscribed, waiting for 10 samples...\nWINDOW\t0\t5000\t40.00\t90.00\t900.00\t0\nROW\tpublish\n";
        let windows = parse_windows(log);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].received, 5000);
        assert_eq!(windows[0].median_us, 40.0);
    }
}
