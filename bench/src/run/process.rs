//! Starting processes, waiting for them, and killing them when they overstay.

use super::Settings;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The physical cores a pinned run uses.
///
/// The publisher and subscriber get one core each, which is what keeps the
/// run-to-run spread small, and only because they send on the thread that
/// paced the send. The server gets every other physical core: both servers
/// are multi-threaded, and one core starves them. Core 0 and its siblings are
/// skipped throughout.
///
/// On a part with two kinds of core the probes take the fastest ones, by the
/// maximum clock `lscpu` reports; a probe on an efficiency core measures that
/// core rather than the server. Where every core reports the same clock the
/// order is the enumeration order.
///
/// Every field is `None` when the run is unpinned, when `lscpu` is not there
/// to ask, or when the machine has fewer than three physical cores to spare.
pub(crate) struct Cores {
    server: Option<String>,
    publisher: Option<String>,
    subscriber: Option<String>,
}

impl Cores {
    /// Linux only, through `taskset`; anywhere else the run is simply
    /// unpinned, which costs spread rather than correctness.
    pub(crate) fn pick(pin: bool) -> Self {
        let none = Cores {
            server: None,
            publisher: None,
            subscriber: None,
        };
        if !pin {
            return none;
        }
        let Some(out) = Command::new("lscpu")
            .arg("-p=CPU,CORE,MAXMHZ")
            .output()
            .ok()
        else {
            return none;
        };
        if !out.status.success() {
            return none;
        }
        let cpus = fastest_first(&String::from_utf8_lossy(&out.stdout));
        if cpus.len() < 3 {
            return none;
        }
        Cores {
            publisher: Some(cpus[0].clone()),
            subscriber: Some(cpus[1].clone()),
            server: Some(cpus[2..].join(",")),
        }
    }

    /// Every physical core the probes are not on, as a `taskset` list.
    pub(crate) fn server(&self) -> Option<&str> {
        self.server.as_deref()
    }

    /// The publisher's core, and the subscriber's, for a probe that may be
    /// pinned at all; see [`super::plan::Probe::pinnable`].
    pub(crate) fn probe(&self, pinnable: bool) -> (Option<&str>, Option<&str>) {
        if pinnable {
            (self.publisher.as_deref(), self.subscriber.as_deref())
        } else {
            (None, None)
        }
    }
}

/// One cpu per physical core other than core 0, fastest core first.
///
/// `text` is `lscpu -p=CPU,CORE,MAXMHZ`; a line without a clock, or with one
/// that does not parse, sorts after the ones that have one.
fn fastest_first(text: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut cpus: Vec<(f64, usize, String)> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut fields = line.split(',');
        let (Some(cpu), Some(core)) = (fields.next(), fields.next()) else {
            continue;
        };
        if core == "0" || seen.iter().any(|s| s == core) {
            continue;
        }
        let clock = fields
            .next()
            .and_then(|mhz| mhz.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        seen.push(core.to_string());
        cpus.push((clock, cpus.len(), cpu.to_string()));
    }
    cpus.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    cpus.into_iter().map(|(_, _, cpu)| cpu).collect()
}

/// A child that is killed when it goes out of scope, however the run ends.
pub(crate) struct Running(Option<Child>);

impl Running {
    /// Hand the child over, so it can be waited on rather than killed.
    ///
    /// The child is then the caller's to end: dropping a bare [`Child`] leaves
    /// it running, which for a server means the next one cannot bind its port
    /// and the harness goes on measuring the first.
    pub(crate) fn take(&mut self) -> Option<Child> {
        self.0.take()
    }

    /// The child's process id while it is still running.
    pub(crate) fn id(&self) -> Option<u32> {
        self.0.as_ref().map(Child::id)
    }

    /// Whether the child has exited, without blocking on it.
    pub(crate) fn finished(&mut self) -> io::Result<bool> {
        match self.0.as_mut() {
            Some(child) => Ok(child.try_wait()?.is_some()),
            None => Ok(true),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Spawn a child with the environment every probe reads.
///
/// `BENCH_WARMUP` and `BENCH_DEADLINE_SECS` are passed explicitly rather than
/// inherited: a probe that guesses its own warmup records a different number of
/// samples than the report says it did.
pub(crate) fn spawn(
    program: &str,
    args: &[String],
    core: Option<&str>,
    log: &Path,
    settings: &Settings,
) -> io::Result<Running> {
    spawn_with_env(program, args, core, log, settings, &[])
}

pub(crate) fn spawn_with_env(
    program: &str,
    args: &[String],
    core: Option<&str>,
    log: &Path,
    settings: &Settings,
    extra: &[(&str, String)],
) -> io::Result<Running> {
    let file = std::fs::File::create(log)?;
    let errors = file.try_clone()?;
    let mut command = match core {
        Some(core) => {
            let mut c = Command::new("taskset");
            c.arg("-c").arg(core).arg(program);
            c
        }
        None => Command::new(program),
    };
    command
        .args(args)
        .env("BENCH_WARMUP", settings.warmup.to_string())
        .env(
            "BENCH_DEADLINE_SECS",
            settings.probe_deadline().as_secs().to_string(),
        )
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(errors));
    for (key, value) in extra {
        command.env(key, value);
    }
    Ok(Running(Some(command.spawn()?)))
}

/// Wait until something accepts a connection on `port`.
pub(crate) fn wait_for_port(port: u16, deadline: Instant) -> bool {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&address.into(), Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Wait until `log` contains `marker`, or the deadline passes.
pub(crate) fn wait_for_marker(log: &Path, marker: &str, deadline: Instant) -> bool {
    while Instant::now() < deadline {
        if std::fs::read_to_string(log).is_ok_and(|text| text.contains(marker)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Wait for a child, killing it if it outlives `limit`.
pub(crate) fn wait_with_limit(child: &mut Child, limit: Duration) -> io::Result<()> {
    let deadline = Instant::now() + limit;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::fastest_first;

    #[test]
    fn probes_take_the_fastest_cores_and_the_rest_keep_their_order() {
        let text =
            "# CPU,Core,Maxmhz\n0,0,3000\n1,1,3000\n2,2,3000\n3,3,5000\n4,4,5000\n5,5,3000\n";
        assert_eq!(fastest_first(text), vec!["3", "4", "1", "2", "5"]);
    }

    #[test]
    fn a_uniform_part_keeps_the_enumeration_order_and_skips_core_zero_siblings() {
        let text =
            "# CPU,Core,Maxmhz\n0,0,2000\n1,1,2000\n2,2,2000\n3,0,2000\n4,1,2000\n5,2,2000\n";
        assert_eq!(fastest_first(text), vec!["1", "2"]);
    }

    #[test]
    fn a_missing_clock_column_still_yields_the_cores() {
        let text = "# CPU,Core\n0,0\n1,1\n2,2\n3,3\n";
        assert_eq!(fastest_first(text), vec!["1", "2", "3"]);
    }
}
