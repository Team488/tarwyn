//! The robot and coprocessor roles on `tarwyn_client`.

use crate::harness::now_nanos;
use crate::sim::measure::{Fold, Mode, PubLine};
use crate::sim::profile::{Class, ClientSpec, Profile, PublishSpec, Role};
use crate::sim::subscriptions::{allowance_us, expected, matching};
use crate::sim::values::{make, read_header};
use crate::sim::{RoleArgs, Window};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tarwyn_client::{Client, Config, Value};

/// When each of a client's topics is next due. Every topic goes out once at
/// the start, then each at its rate, with topics due at the same instant
/// handed out together, as a robot loop sends them.
#[derive(Debug)]
pub struct Schedule {
    periods: Vec<Option<Duration>>,
    due: Vec<Instant>,
    start: Instant,
    started: bool,
}

impl Schedule {
    /// A schedule for `publishes` at their rates times `multiplier`, from `start`.
    pub fn new(publishes: &[PublishSpec], multiplier: u32, start: Instant) -> Self {
        let periods: Vec<Option<Duration>> = publishes
            .iter()
            .map(|spec| {
                (spec.rate_hz > 0.0)
                    .then(|| Duration::from_secs_f64(1.0 / (spec.rate_hz * f64::from(multiplier))))
            })
            .collect();
        let due = periods
            .iter()
            .map(|period| start + period.unwrap_or_default())
            .collect();
        Schedule {
            periods,
            due,
            start,
            started: false,
        }
    }

    /// The next instant anything is due, and every topic due then. `None`
    /// once only topics published once remain.
    #[expect(
        clippy::should_implement_trait,
        reason = "a schedule never ends while it has a rate"
    )]
    pub fn next(&mut self) -> Option<(Instant, Vec<usize>)> {
        if !self.started {
            self.started = true;
            return Some((self.start, (0..self.periods.len()).collect()));
        }
        let soonest = (0..self.periods.len())
            .filter(|&index| self.periods[index].is_some())
            .map(|index| self.due[index])
            .min()?;
        let mut batch = Vec::new();
        for index in 0..self.periods.len() {
            if let Some(period) = self.periods[index]
                && self.due[index] == soonest
            {
                batch.push(index);
                self.due[index] += period;
            }
        }
        Some((soonest, batch))
    }
}

/// Publish `value` on `spec`'s topic through the client call for its type.
fn send(client: &Client, spec: &PublishSpec, value: Value) {
    let topic = spec.topic.as_str();
    match value {
        Value::Bytes(bytes) if spec.type_str == "raw" => client.send_bytes(topic, &bytes),
        Value::Bytes(bytes) => client.send_struct(topic, &spec.type_str, &[], bytes),
        Value::String(text) => client.send_string(topic, &text),
        Value::Bool(flag) => client.send_bool(topic, flag),
        Value::Double(number) => client.send_double(topic, number),
        Value::Int64(number) => client.send_i64(topic, number),
        Value::Float(number) => client.send_float(topic, number),
        Value::BoolArray(list) => client.send_bool_list(topic, &list),
        Value::DoubleArray(list) => client.send_double_list(topic, &list),
        Value::Int64Array(list) => client.send_long_list(topic, &list),
        Value::FloatArray(list) => client.send_float_list(topic, &list),
        Value::StringArray(list) => client.send_string_list(topic, &list),
        other => client.send_message_public(topic, other),
    }
}

/// A client for the server at `host` (`host:port`).
fn connect(host: &str) -> anyhow::Result<Client> {
    let (address, port) = host
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("{host} is not host:port"))?;
    Ok(Client::try_with_config(Config {
        host: address.to_string(),
        port: port.parse()?,
        ..Config::default()
    })?)
}

/// Publish `spec`'s topics on schedule until the window closes, and return
/// what was sent of each class against what was asked for. A send counts
/// when it leaves inside the window, so a publisher that falls behind sends
/// short.
fn publish(client: &Client, spec: &ClientSpec, args: &RoleArgs, window: Window) -> Vec<PubLine> {
    let mut schedule = Schedule::new(&spec.publishes, args.multiplier, Instant::now());
    let mut seq = vec![0u64; spec.publishes.len()];
    let mut sent: BTreeMap<Class, u64> = BTreeMap::new();
    while let Some((due, batch)) = schedule.next() {
        let now = Instant::now();
        if now >= window.closes || due >= window.closes {
            break;
        }
        std::thread::sleep(due.saturating_duration_since(now));
        let overshoot = Instant::now().saturating_duration_since(due).as_nanos() as u64;
        let due_ns = now_nanos().saturating_sub(overshoot);
        for index in batch {
            let topic = &spec.publishes[index];
            seq[index] += 1;
            send(client, topic, make(topic, seq[index], due_ns));
            if window.records(Instant::now()) && topic.rate_hz > 0.0 {
                *sent.entry(topic.class).or_default() += 1;
            }
        }
    }
    let mut asked: BTreeMap<Class, f64> = BTreeMap::new();
    for topic in spec.publishes.iter().filter(|topic| topic.rate_hz > 0.0) {
        *asked.entry(topic.class).or_default() +=
            topic.rate_hz * f64::from(args.multiplier) * args.seconds;
    }
    asked
        .into_iter()
        .map(|(class, asked)| PubLine {
            client: spec.name.clone(),
            class,
            asked,
            sent: sent.get(&class).copied().unwrap_or(0),
        })
        .collect()
}

/// The robot: publishes its topics and receives its subscriptions, then
/// prints its `PUB` lines and, as receiver `robot`, its `SIM` lines.
///
/// # Errors
///
/// Returns an error when the profile cannot be loaded or has no robot.
pub fn run_robot(args: &RoleArgs) -> anyhow::Result<()> {
    let profile = Profile::load(&args.profile)?;
    let robot = profile
        .clients
        .iter()
        .find(|client| client.role == Role::Robot)
        .ok_or_else(|| anyhow::anyhow!("the profile has no robot"))?;
    let window = Window::from(args);
    let client = connect(&args.host)?;
    let folds: Arc<Mutex<BTreeMap<Class, Fold>>> = Arc::default();
    let mut cancels = Vec::new();
    for topic in robot.subscribes.iter().flat_map(|sub| sub.topics.iter()) {
        let Some(spec) = profile.topic(topic) else {
            continue;
        };
        let (class, name, folds) = (spec.class, topic.clone(), Arc::clone(&folds));
        cancels.push(client.subscribe(topic, move |value| {
            let received = now_nanos();
            if !window.records(Instant::now()) {
                return;
            }
            if let Some((seq, due)) = read_header(value)
                && let Ok(mut folds) = folds.lock()
            {
                folds.entry(class).or_default().record(
                    &name,
                    seq,
                    received.saturating_sub(due),
                    Mode::All,
                );
            }
        }));
    }
    client.start();
    let pubs = publish(&client, robot, args, window);
    std::thread::sleep(Duration::from_millis(200));
    for cancel in cancels {
        cancel();
    }
    for line in pubs {
        println!("{line}");
    }
    let mut folds = folds
        .lock()
        .map_err(|_| anyhow::anyhow!("a receive callback panicked"))?;
    for class in [Class::Camera, Class::Struct, Class::Slow] {
        if expected(
            &profile,
            &robot.subscribes,
            class,
            args.multiplier,
            args.seconds,
        ) > 0.0
        {
            folds.entry(class).or_default();
        }
    }
    for (class, fold) in folds.iter() {
        let wanted = expected(
            &profile,
            &robot.subscribes,
            *class,
            args.multiplier,
            args.seconds,
        );
        let allow = allowance_us(&profile, &robot.subscribes, *class, args.multiplier);
        let mode = robot
            .subscribes
            .iter()
            .flat_map(|sub| sub.topics.iter())
            .find_map(|topic| matching(&robot.subscribes, topic))
            .map_or(Mode::All, |matched| matched.mode);
        println!("{}", fold.line("robot", *class, mode, wanted, allow));
    }
    client.stop();
    Ok(())
}

/// Every coprocessor, each on its own client and thread, then each one's
/// `PUB` lines.
///
/// # Errors
///
/// Returns an error when the profile cannot be loaded or a client cannot be
/// built.
pub fn run_coprocessors(args: &RoleArgs) -> anyhow::Result<()> {
    let profile = Profile::load(&args.profile)?;
    let window = Window::from(args);
    let cameras: Vec<&ClientSpec> = profile
        .clients
        .iter()
        .filter(|client| client.role == Role::Coprocessor)
        .collect();
    let lines = std::thread::scope(|scope| -> anyhow::Result<Vec<PubLine>> {
        let running: Vec<_> = cameras
            .iter()
            .map(|camera| {
                scope.spawn(move || -> anyhow::Result<Vec<PubLine>> {
                    let client = connect(&args.host)?;
                    let lines = publish(&client, camera, args, window);
                    client.stop();
                    Ok(lines)
                })
            })
            .collect();
        let mut lines = Vec::new();
        for handle in running {
            lines.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("a coprocessor panicked"))??,
            );
        }
        Ok(lines)
    })?;
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rate_hz: f64) -> PublishSpec {
        PublishSpec {
            topic: format!("/t/{rate_hz}"),
            type_str: "double".into(),
            rate_hz,
            bytes: [9, 9],
            class: Class::Scalar,
            properties: Default::default(),
        }
    }

    #[test]
    fn everything_goes_out_once_then_each_topic_at_its_rate() {
        let start = Instant::now();
        let topics = [
            at(25.0),
            PublishSpec {
                topic: "/u".into(),
                ..at(25.0)
            },
            at(0.0),
        ];
        let mut schedule = Schedule::new(&topics, 1, start);
        assert_eq!(schedule.next(), Some((start, vec![0, 1, 2])));
        assert_eq!(
            schedule.next(),
            Some((start + Duration::from_millis(40), vec![0, 1]))
        );
        assert_eq!(
            schedule.next(),
            Some((start + Duration::from_millis(80), vec![0, 1]))
        );
    }

    #[test]
    fn the_multiplier_shortens_every_period() {
        let start = Instant::now();
        let mut schedule = Schedule::new(&[at(25.0)], 2, start);
        schedule.next();
        assert_eq!(
            schedule.next(),
            Some((start + Duration::from_millis(20), vec![0]))
        );
    }

    #[test]
    fn topics_published_once_end_the_schedule() {
        let start = Instant::now();
        let mut schedule = Schedule::new(&[at(0.0)], 1, start);
        assert_eq!(schedule.next(), Some((start, vec![0])));
        assert_eq!(schedule.next(), None);
    }
}
