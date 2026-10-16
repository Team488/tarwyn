//! The dashboard role: raw NT4 connections that subscribe the way each
//! dashboard in the profile does, plus fresh connections timed to full state.

use crate::harness::now_nanos;
use crate::probes::networktables::connect_as;
use crate::sim::measure::{ConnLine, Fold, Mode};
use crate::sim::profile::{Class, Profile, Role, SubscribeSpec};
use crate::sim::subscriptions::{allowance_us, expected, matching};
use crate::sim::values::read_header;
use crate::sim::{RoleArgs, Window};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tarwyn_server::websocket::message::{ControlMessage, ValueMessage};
use tungstenite::{Message, WebSocket};

/// How long a timed connection waits for full state before it gives up.
const PROBE_LIMIT: Duration = Duration::from_secs(10);
/// How long a read blocks before the loop checks its deadline again.
const READ_TIMEOUT: Duration = Duration::from_millis(50);

/// The NT4 text frame that makes `subs`, with subscription ids from 0.
fn subscribe_frame(subs: &[SubscribeSpec]) -> String {
    let messages: Vec<Json> = subs
        .iter()
        .enumerate()
        .map(|(subuid, sub)| {
            let mut options = sub.options.clone();
            options.insert("prefix".into(), json!(sub.prefix));
            json!({"method": "subscribe", "params": {
                "topics": sub.topics, "subuid": subuid, "options": options}})
        })
        .collect();
    Json::Array(messages).to_string()
}

/// One frame from the server, or `None` when the read timed out.
fn read(socket: &mut WebSocket<TcpStream>) -> anyhow::Result<Option<Message>> {
    match socket.read() {
        Ok(message) => Ok(Some(message)),
        Err(tungstenite::Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

/// Open a connection as `name` and send `subs`.
fn open(host: &str, name: &str, subs: &[SubscribeSpec]) -> anyhow::Result<WebSocket<TcpStream>> {
    let mut socket = connect_as(host, name)?;
    socket.get_mut().set_read_timeout(Some(READ_TIMEOUT))?;
    socket.send(Message::text(subscribe_frame(subs)))?;
    Ok(socket)
}

/// Receive as dashboard `name` until the window closes, and return its
/// `SIM` lines: one per class and mode it measured, plus one for every class
/// it asked for in full and never heard from.
fn watch(
    profile: &Profile,
    args: &RoleArgs,
    window: Window,
    name: &str,
    subs: &[SubscribeSpec],
) -> anyhow::Result<Vec<String>> {
    let mut socket = open(&args.host, name, subs)?;
    let mut topics: HashMap<u32, (String, Class, Mode)> = HashMap::new();
    let mut folds: BTreeMap<(Class, Mode), Fold> = BTreeMap::new();
    while Instant::now() < window.closes {
        match read(&mut socket)? {
            Some(Message::Text(text)) => {
                for message in ControlMessage::from_json_batch(&text).unwrap_or_default() {
                    let ControlMessage::Announce {
                        name: topic, id, ..
                    } = message
                    else {
                        continue;
                    };
                    let (Some(spec), Some(matched)) =
                        (profile.topic(&topic), matching(subs, &topic))
                    else {
                        continue;
                    };
                    if spec.class != Class::Scalar {
                        topics.insert(id, (topic, spec.class, matched.mode));
                    }
                }
            }
            Some(Message::Binary(bytes)) => {
                let now = Instant::now();
                let received = now_nanos();
                if !window.records(now) {
                    continue;
                }
                for message in ValueMessage::decode_all(&bytes).unwrap_or_default() {
                    let Some((topic, class, mode)) = topics.get(&message.topic_id) else {
                        continue;
                    };
                    if let Some((seq, due)) = read_header(&message.value) {
                        folds.entry((*class, *mode)).or_default().record(
                            topic,
                            seq,
                            received.saturating_sub(due),
                            *mode,
                        );
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    let _ = socket.close(None);
    for class in [Class::Camera, Class::Struct, Class::Slow] {
        if expected(profile, subs, class, args.multiplier, args.seconds) > 0.0 {
            folds.entry((class, Mode::All)).or_default();
        }
    }
    Ok(folds
        .iter()
        .map(|((class, mode), fold)| {
            let wanted = match mode {
                Mode::All => expected(profile, subs, *class, args.multiplier, args.seconds),
                Mode::Coalesce => 0.0,
            };
            fold.line(
                name,
                *class,
                *mode,
                wanted,
                allowance_us(profile, subs, *class, args.multiplier),
            )
            .to_string()
        })
        .collect())
}

/// Time one fresh subscribe-all connection: from opening it to every profile
/// topic announced, and to a value for each.
fn probe(profile: &Profile, host: &str, name: &str) -> ConnLine {
    let wanted: HashSet<&str> = profile
        .publishes()
        .map(|(_, spec)| spec.topic.as_str())
        .collect();
    let started = Instant::now();
    let everything = [SubscribeSpec {
        topics: vec![String::new()],
        prefix: true,
        options: json!({"all": true})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    }];
    let Ok(mut socket) = open(host, name, &everything) else {
        return ConnLine {
            announced_ms: None,
            values_ms: None,
        };
    };
    let mut ids: HashMap<u32, bool> = HashMap::new();
    let mut valued = 0usize;
    let mut announced_ms = None;
    let mut values_ms = None;
    let elapsed_ms = || started.elapsed().as_secs_f64() * 1000.0;
    while started.elapsed() < PROBE_LIMIT && values_ms.is_none() {
        match read(&mut socket) {
            Ok(Some(Message::Text(text))) => {
                for message in ControlMessage::from_json_batch(&text).unwrap_or_default() {
                    if let ControlMessage::Announce {
                        name: topic, id, ..
                    } = message
                        && wanted.contains(topic.as_str())
                    {
                        ids.entry(id).or_insert(false);
                    }
                }
                if announced_ms.is_none() && ids.len() >= wanted.len() {
                    announced_ms = Some(elapsed_ms());
                }
            }
            Ok(Some(Message::Binary(bytes))) => {
                for message in ValueMessage::decode_all(&bytes).unwrap_or_default() {
                    if let Some(seen) = ids.get_mut(&message.topic_id)
                        && !*seen
                    {
                        *seen = true;
                        valued += 1;
                    }
                }
                if announced_ms.is_some() && valued >= wanted.len() {
                    values_ms = Some(elapsed_ms());
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = socket.close(None);
    ConnLine {
        announced_ms,
        values_ms,
    }
}

/// Run every dashboard in the profile, `args.extra` copies of the first spare
/// one, and `args.probes` timed connections spread through the window. Prints
/// each dashboard's `SIM` lines and each probe's `CONN` line when the window
/// closes.
///
/// # Errors
///
/// Returns an error when the profile cannot be loaded or a dashboard cannot
/// connect.
pub fn run(args: &RoleArgs) -> anyhow::Result<()> {
    let profile = Profile::load(&args.profile)?;
    let window = Window::from(args);
    let dashboards = profile
        .clients
        .iter()
        .filter(|client| client.role == Role::Dashboard);
    let mut viewers: Vec<(String, Vec<SubscribeSpec>)> = dashboards
        .clone()
        .filter(|client| !client.spare)
        .map(|client| (client.name.clone(), client.subscribes.clone()))
        .collect();
    if let Some(spare) = dashboards.clone().find(|client| client.spare) {
        viewers.extend(
            (1..=args.extra).map(|k| (format!("{}-{k}", spare.name), spare.subscribes.clone())),
        );
    }
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let watching: Vec<_> = viewers
            .iter()
            .map(|(name, subs)| scope.spawn(|| watch(&profile, args, window, name, subs)))
            .collect();
        let probing = scope.spawn(|| {
            let mut lines = Vec::new();
            for k in 0..args.probes {
                let at = window.opens
                    + Duration::from_secs_f64(
                        args.seconds * f64::from(k + 1) / f64::from(args.probes + 1),
                    );
                std::thread::sleep(at.saturating_duration_since(Instant::now()));
                lines.push(probe(&profile, &args.host, &format!("probe-{k}")).to_string());
            }
            lines
        });
        println!("dashboards watching: {}", viewers.len());
        let mut lines = Vec::new();
        for handle in watching {
            lines.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("a dashboard panicked"))??,
            );
        }
        lines.extend(
            probing
                .join()
                .map_err(|_| anyhow::anyhow!("a probe panicked"))?,
        );
        for line in lines {
            println!("{line}");
        }
        Ok(())
    })
}
