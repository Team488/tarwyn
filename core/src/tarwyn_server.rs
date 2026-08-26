use arc_swap::ArcSwap;
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::utils::{log::LOGGER, ports, ring_buffer::RingBuffer};
use tarwyn_protobuf::telemetry;

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    CompareAndSetCommand, Publish, Push, Reply, ReplyCompareAndSetCommand, ReplyDataCommand,
    ReplyDeleteCommand, ReplyJsonCommand, ReplyLogsCommand, ReplyPingCommand,
    ReplyStatisticsCommand, ReplyTablesCommand, ReplyTelemetryCommand, Request, SendDataCommand,
    SupportedValues, publish, push, reply, request, supported_values,
};

use zmq::{
    Context, SNDMORE,
    SocketType::{PUB, PULL, REP},
};

const TELEMETRY_TTL: Duration = Duration::from_secs(10);
const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";

const DEFAULT_REP_PORT: u16 = ports::DEFAULT_REQ_REP_PORT;
const DEFAULT_PUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PULL_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

pub struct TarwynServer {
    pub_socket: Arc<Mutex<zmq::Socket>>,
    pull_socket: Arc<Mutex<zmq::Socket>>,
    rep_socket: Arc<Mutex<zmq::Socket>>,
    cached_messages: Arc<Mutex<HashMap<String, RingBuffer<supported_values::Kind>>>>,
    telemetry_subscribers: Arc<ArcSwap<HashMap<u32, Vec<SocketAddr>>>>,
    telemetry_registry: Arc<Mutex<HashMap<u32, HashMap<SocketAddr, Instant>>>>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    started: Instant,
}

impl TarwynServer {
    pub fn new() -> Self {
        Self::with_ports(DEFAULT_PUB_PORT, DEFAULT_PULL_PORT, DEFAULT_REP_PORT)
    }

    pub fn with_ports(pub_port: u16, pull_port: u16, rep_port: u16) -> Self {
        let context = Context::new();

        let cached_messages = Arc::new(Mutex::new(HashMap::new()));
        let telemetry_subscribers = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let telemetry_registry = Arc::new(Mutex::new(HashMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let pub_socket = Arc::new(Mutex::new(context.socket(PUB).unwrap()));
        let pull_socket = Arc::new(Mutex::new(context.socket(PULL).unwrap()));
        let rep_socket = Arc::new(Mutex::new(context.socket(REP).unwrap()));

        pub_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", pub_port))
            .unwrap();
        pull_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", pull_port))
            .unwrap();
        rep_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", rep_port))
            .unwrap();

        TarwynServer {
            pub_socket,
            pull_socket,
            rep_socket,
            cached_messages,
            telemetry_subscribers,
            telemetry_registry,
            stop,
            initialized,
            started: Instant::now(),
        }
    }

    fn now_nanos() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos() as u64)
            .unwrap_or(0)
    }

    fn compare_and_set(
        cached: &mut HashMap<String, RingBuffer<supported_values::Kind>>,
        command: CompareAndSetCommand,
    ) -> (bool, Option<supported_values::Kind>) {
        let ring = cached
            .entry(command.channel)
            .or_insert_with(|| RingBuffer::new(100));
        let current = ring.peek().cloned();

        let matches = if command.expect_absent {
            current.is_none()
        } else {
            match (&current, command.expected.and_then(|value| value.kind)) {
                (Some(current), Some(expected)) => *current == expected,
                _ => false,
            }
        };

        if !matches {
            return (false, current);
        }

        let Some(kind) = command.value.and_then(|value| value.kind) else {
            return (false, current);
        };
        ring.push(kind.clone());
        (true, Some(kind))
    }

    fn write_json_value(out: &mut String, kind: &supported_values::Kind) {
        use supported_values::Kind;
        match kind {
            Kind::String(value) => Self::write_json_string(out, value),
            Kind::Int32(value) => out.push_str(&value.to_string()),
            Kind::Int64(value) => out.push_str(&value.to_string()),
            Kind::Uint32(value) => out.push_str(&value.to_string()),
            Kind::Uint64(value) => out.push_str(&value.to_string()),
            Kind::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Kind::Double(value) => Self::write_json_number(out, *value),
            Kind::Float(value) => Self::write_json_number(out, f64::from(*value)),
            Kind::Bytes(value) => {
                out.push('"');
                for byte in value {
                    out.push_str(&format!("{byte:02x}"));
                }
                out.push('"');
            }
            Kind::StringList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_string(out, value)
                });
            }
            Kind::BytesList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push('"');
                    for byte in value {
                        out.push_str(&format!("{byte:02x}"));
                    }
                    out.push('"');
                });
            }
            Kind::BoolList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(if *value { "true" } else { "false" })
                });
            }
            Kind::FloatList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_number(out, f64::from(*value))
                });
            }
            Kind::DoubleList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    Self::write_json_number(out, *value)
                });
            }
            Kind::IntegerList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(&value.to_string())
                });
            }
            Kind::LongList(list) => {
                Self::write_json_array(out, &list.values, |out, value| {
                    out.push_str(&value.to_string())
                });
            }
            Kind::CoordinateList(list) => {
                Self::write_json_array(out, &list.coordinates, |out, coordinate| {
                    out.push_str("{\"x\":");
                    Self::write_json_number(out, coordinate.x);
                    out.push_str(",\"y\":");
                    Self::write_json_number(out, coordinate.y);
                    out.push('}');
                });
            }
        }
    }

    fn write_json_array<T>(out: &mut String, values: &[T], mut write: impl FnMut(&mut String, &T)) {
        out.push('[');
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write(out, value);
        }
        out.push(']');
    }

    fn write_json_number(out: &mut String, value: f64) {
        if value.is_finite() {
            out.push_str(&value.to_string());
        } else {
            out.push_str("null");
        }
    }

    fn write_json_string(out: &mut String, value: &str) {
        out.push('"');
        for character in value.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                control if (control as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", control as u32))
                }
                other => out.push(other),
            }
        }
        out.push('"');
    }

    fn to_json(
        cached: &HashMap<String, RingBuffer<supported_values::Kind>>,
        prefix: &str,
    ) -> String {
        let mut names: Vec<&String> = cached
            .keys()
            .filter(|name| name.starts_with(prefix))
            .collect();
        names.sort();

        let mut out = String::from("{");
        let mut first = true;
        for name in names {
            let Some(kind) = cached.get(name).and_then(|ring| ring.peek()) else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            Self::write_json_string(&mut out, name);
            out.push(':');
            Self::write_json_value(&mut out, kind);
        }
        out.push('}');
        out
    }

    fn data_reply(data: Option<supported_values::Kind>) -> Vec<u8> {
        let kind =
            data.unwrap_or_else(|| supported_values::Kind::String(String::from(NO_DATA_SENTINEL)));

        Reply {
            payload: Some(reply::Payload::Data(ReplyDataCommand {
                value: Some(SupportedValues { kind: Some(kind) }),
            })),
        }
        .encode_to_vec()
    }

    fn publish_data(channel: &str, data: supported_values::Kind) -> Vec<u8> {
        Publish {
            payload: Some(publish::Payload::Data(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues { kind: Some(data) }),
            })),
        }
        .encode_to_vec()
    }

    pub fn start(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            info!("Initializing Tarwyn server...");
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            info!("Starting Tarwyn server...");
            self.stop.store(false, Ordering::SeqCst);
        } else {
            info!("Tarwyn server is already running.");
            return;
        }

        {
            let cached_messages = self.cached_messages.clone();
            let pull_socket = self.pull_socket.clone();
            let pub_socket = self.pub_socket.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            std::thread::spawn(move || {
                let pull_socket = pull_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let bytes = match pull_socket.recv_bytes(0) {
                        Ok(bytes) => bytes,
                        Err(zmq::Error::ETERM) => break,
                        Err(error) => {
                            info!("dropping a push that could not be received: {error}");
                            continue;
                        }
                    };

                    let Some(payload) = Push::decode(&bytes[..]).ok().and_then(|push| push.payload)
                    else {
                        info!("dropping a malformed push of {} bytes", bytes.len());
                        continue;
                    };

                    match payload {
                        push::Payload::Send(command) => {
                            let channel = command.channel;
                            let Some(data) = command.value.and_then(|value| value.kind) else {
                                info!("dropping a push on '{channel}' that carried no value");
                                continue;
                            };

                            let Ok(mut cached) = cached_messages.lock() else {
                                continue;
                            };
                            let ring_buffer = cached
                                .entry(channel.clone())
                                .or_insert(RingBuffer::new(100));

                            let message = Self::publish_data(&channel, data.clone());
                            ring_buffer.push(data);
                            drop(cached);

                            let Ok(pub_socket) = pub_socket.lock() else {
                                continue;
                            };
                            if pub_socket.send(&channel, SNDMORE).is_ok() {
                                let _ = pub_socket.send(message, 0);
                            }
                        }
                    }
                }
            });
        }

        self.start_telemetry_relay();

        {
            let cached_buffers = self.cached_messages.clone();
            let telemetry_subscribers = self.telemetry_subscribers.clone();
            let telemetry_registry = self.telemetry_registry.clone();
            let rep_socket = self.rep_socket.clone();
            let stop = self.stop.clone();
            let started = self.started;

            std::thread::spawn(move || {
                let rep_socket = rep_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }

                    let bytes = match rep_socket.recv_bytes(0) {
                        Ok(bytes) => bytes,
                        Err(zmq::Error::ETERM) => break,
                        Err(error) => {
                            info!("dropping a request that could not be received: {error}");
                            continue;
                        }
                    };

                    let Some(payload) = Request::decode(&bytes[..])
                        .ok()
                        .and_then(|request| request.payload)
                    else {
                        info!(
                            "answering a malformed request of {} bytes with no data",
                            bytes.len()
                        );
                        let _ = rep_socket.send(Self::data_reply(None), 0);
                        continue;
                    };

                    match payload {
                        request::Payload::Data(command) => {
                            let data = match cached_buffers.lock() {
                                Ok(mut cached) => cached
                                    .entry(command.channel)
                                    .or_insert_with(|| RingBuffer::new(100))
                                    .peek()
                                    .cloned(),
                                Err(_) => None,
                            };

                            let _ = rep_socket.send(Self::data_reply(data), 0);
                        }
                        request::Payload::Delete(command) => {
                            let deleted = match cached_buffers.lock() {
                                Ok(mut cached) => {
                                    if command.channel.is_empty() {
                                        let count = cached.len();
                                        cached.clear();
                                        count
                                    } else {
                                        usize::from(cached.remove(&command.channel).is_some())
                                    }
                                }
                                Err(_) => 0,
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::Delete(ReplyDeleteCommand {
                                    deleted: deleted as u32,
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Tables(command) => {
                            let channels = match cached_buffers.lock() {
                                Ok(cached) => {
                                    let mut names: Vec<String> = cached
                                        .keys()
                                        .filter(|name| name.starts_with(&command.prefix))
                                        .cloned()
                                        .collect();
                                    names.sort();
                                    names
                                }
                                Err(_) => Vec::new(),
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::Tables(ReplyTablesCommand {
                                    channels,
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Ping(command) => {
                            let message = Reply {
                                payload: Some(reply::Payload::Ping(ReplyPingCommand {
                                    sent_nanos: command.sent_nanos,
                                    server_nanos: Self::now_nanos(),
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Statistics(_) => {
                            let (channels, values) = match cached_buffers.lock() {
                                Ok(cached) => (
                                    cached.len() as u64,
                                    cached.values().map(|ring| ring.items.len() as u64).sum(),
                                ),
                                Err(_) => (0, 0),
                            };
                            let subscribers = telemetry_subscribers
                                .load()
                                .values()
                                .map(|addresses| addresses.len() as u64)
                                .sum();

                            let message = Reply {
                                payload: Some(reply::Payload::Statistics(ReplyStatisticsCommand {
                                    channels,
                                    values,
                                    telemetry_subscribers: subscribers,
                                    uptime_seconds: started.elapsed().as_secs(),
                                    version: env!("CARGO_PKG_VERSION").to_string(),
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Json(command) => {
                            let json = match cached_buffers.lock() {
                                Ok(cached) => Self::to_json(&cached, &command.prefix),
                                Err(_) => String::from("{}"),
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::Json(ReplyJsonCommand { json })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::CompareAndSet(command) => {
                            let (swapped, current) = match cached_buffers.lock() {
                                Ok(mut cached) => Self::compare_and_set(&mut cached, command),
                                Err(_) => (false, None),
                            };

                            let message = Reply {
                                payload: Some(reply::Payload::CompareAndSet(
                                    ReplyCompareAndSetCommand {
                                        swapped,
                                        current: current
                                            .map(|kind| SupportedValues { kind: Some(kind) }),
                                    },
                                )),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::RegisterTelemetry(command) => {
                            let registered = command
                                .address
                                .parse::<SocketAddr>()
                                .map(|address| {
                                    Self::register_telemetry(
                                        &telemetry_registry,
                                        &telemetry_subscribers,
                                        telemetry::topic_hash(&command.channel),
                                        address,
                                    )
                                })
                                .unwrap_or(false);

                            let message = Reply {
                                payload: Some(reply::Payload::Telemetry(ReplyTelemetryCommand {
                                    registered,
                                })),
                            }
                            .encode_to_vec();
                            let _ = rep_socket.send(message, 0);
                        }
                        request::Payload::Logs(_) => {
                            let logs = LOGGER.get_logs();
                            if let Some(logs) = logs {
                                info!("Sending logs in response to request.");
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand { logs })),
                                }
                                .encode_to_vec();

                                let _ = rep_socket.send(message, 0);
                            } else {
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand {
                                        logs: vec![],
                                    })),
                                }
                                .encode_to_vec();

                                let _ = rep_socket.send(message, 0);
                            }
                        }
                    }
                }
            });
        }
    }

    fn register_telemetry(
        registry: &Mutex<HashMap<u32, HashMap<SocketAddr, Instant>>>,
        published: &ArcSwap<HashMap<u32, Vec<SocketAddr>>>,
        channel_hash: u32,
        address: SocketAddr,
    ) -> bool {
        let Ok(mut registry) = registry.lock() else {
            return false;
        };
        let now = Instant::now();
        registry
            .entry(channel_hash)
            .or_default()
            .insert(address, now);

        for addresses in registry.values_mut() {
            addresses.retain(|_, seen| now.duration_since(*seen) < TELEMETRY_TTL);
        }
        registry.retain(|_, addresses| !addresses.is_empty());

        let snapshot: HashMap<u32, Vec<SocketAddr>> = registry
            .iter()
            .map(|(hash, addresses)| (*hash, addresses.keys().copied().collect()))
            .collect();
        published.store(Arc::new(snapshot));
        true
    }

    fn start_telemetry_relay(&self) {
        let subscribers = self.telemetry_subscribers.clone();
        let stop = self.stop.clone();

        let socket = match UdpSocket::bind(("0.0.0.0", telemetry::DEFAULT_TELEMETRY_PORT)) {
            Ok(socket) => socket,
            Err(error) => {
                info!("telemetry relay disabled, could not bind: {error}");
                return;
            }
        };
        telemetry::tune(&socket);
        let _ = socket.set_read_timeout(Some(std::time::Duration::from_millis(100)));

        std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok((len, _from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((channel_hash, _timestamp, _payload)) = telemetry::decode(&buf[..len])
                else {
                    continue;
                };
                let routes = subscribers.load();
                let Some(targets) = routes.get(&channel_hash) else {
                    continue;
                };
                for target in targets {
                    let _ = socket.send_to(&buf[..len], target);
                }
            }
        });
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        info!("Tarwyn server has been stopped.");
    }
}

impl Default for TarwynServer {
    fn default() -> Self {
        TarwynServer::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarwyn_protobuf::protobuf::GetDataCommand;

    fn valid_push(channel: &str, value: &str) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues {
                    kind: Some(supported_values::Kind::String(value.to_string())),
                }),
            })),
        }
        .encode_to_vec()
    }

    fn valueless_push(channel: &str) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: None,
            })),
        }
        .encode_to_vec()
    }

    fn get_request(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Data(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    fn read_string(bytes: &[u8]) -> String {
        let reply = Reply::decode(bytes).expect("the server sent something that is not a Reply");
        match reply.payload {
            Some(reply::Payload::Data(command)) => match command.value.and_then(|value| value.kind)
            {
                Some(supported_values::Kind::String(value)) => value,
                other => panic!("expected a string, got {other:?}"),
            },
            other => panic!("expected a data reply, got {other:?}"),
        }
    }

    fn requester(context: &Context, port: u16) -> zmq::Socket {
        let socket = context.socket(zmq::SocketType::REQ).unwrap();
        socket.set_rcvtimeo(3000).unwrap();
        socket.set_sndtimeo(3000).unwrap();
        socket.connect(&format!("tcp://127.0.0.1:{port}")).unwrap();
        socket
    }

    fn string(value: &str) -> supported_values::Kind {
        supported_values::Kind::String(value.to_string())
    }

    fn wrap(kind: supported_values::Kind) -> Option<SupportedValues> {
        Some(SupportedValues { kind: Some(kind) })
    }

    #[test]
    fn compare_and_set_claims_an_empty_channel_once() {
        let mut cached = HashMap::new();

        let (claimed, _) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "lock".into(),
                expected: None,
                value: wrap(string("agent-a")),
                expect_absent: true,
            },
        );
        assert!(claimed);

        let (stolen, current) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "lock".into(),
                expected: None,
                value: wrap(string("agent-b")),
                expect_absent: true,
            },
        );
        assert!(
            !stolen,
            "a second claimant took a lock that was already held"
        );
        assert_eq!(current, Some(string("agent-a")));
    }

    #[test]
    fn compare_and_set_refuses_a_stale_expectation() {
        let mut cached = HashMap::new();
        cached
            .entry(String::from("counter"))
            .or_insert_with(|| RingBuffer::new(100))
            .push(supported_values::Kind::Double(1.0));

        let (moved, _) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "counter".into(),
                expected: wrap(supported_values::Kind::Double(1.0)),
                value: wrap(supported_values::Kind::Double(2.0)),
                expect_absent: false,
            },
        );
        assert!(moved);

        let (again, current) = TarwynServer::compare_and_set(
            &mut cached,
            CompareAndSetCommand {
                channel: "counter".into(),
                expected: wrap(supported_values::Kind::Double(1.0)),
                value: wrap(supported_values::Kind::Double(3.0)),
                expect_absent: false,
            },
        );
        assert!(!again, "a read-modify-write raced and both writers won");
        assert_eq!(current, Some(supported_values::Kind::Double(2.0)));
    }

    #[test]
    fn json_escapes_what_would_otherwise_break_the_document() {
        let mut cached = HashMap::new();
        cached
            .entry(String::from("quote\"and\\slash"))
            .or_insert_with(|| RingBuffer::new(100))
            .push(string("line\nbreak\ttab"));

        let json = TarwynServer::to_json(&cached, "");
        assert_eq!(
            json, r#"{"quote\"and\\slash":"line\nbreak\ttab"}"#,
            "the document would not parse"
        );
    }

    #[test]
    fn json_leaves_out_channels_outside_the_prefix() {
        let mut cached = HashMap::new();
        for name in ["robot/a", "robot/b", "vision/c"] {
            cached
                .entry(String::from(name))
                .or_insert_with(|| RingBuffer::new(100))
                .push(supported_values::Kind::Bool(true));
        }

        assert_eq!(
            TarwynServer::to_json(&cached, "robot/"),
            r#"{"robot/a":true,"robot/b":true}"#
        );
    }

    #[test]
    fn a_malformed_push_does_not_stop_the_write_path() {
        let server = TarwynServer::with_ports(47941, 47942, 47943);
        server.start();

        let context = Context::new();
        let push = context.socket(zmq::SocketType::PUSH).unwrap();
        push.connect("tcp://127.0.0.1:47942").unwrap();
        std::thread::sleep(Duration::from_millis(200));

        push.send(&[][..], 0).unwrap();
        push.send(&[0xff, 0xff, 0xff][..], 0).unwrap();
        push.send(valueless_push("survives"), 0).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        push.send(valid_push("survives", "still here"), 0).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let req = requester(&context, 47943);
        req.send(get_request("survives"), 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("the server stopped answering after a malformed push");

        assert_eq!(read_string(&bytes), "still here");
        server.stop();
    }

    #[test]
    fn a_malformed_request_is_answered_so_the_socket_stays_usable() {
        let server = TarwynServer::with_ports(47951, 47952, 47953);
        server.start();
        std::thread::sleep(Duration::from_millis(200));

        let context = Context::new();
        let req = requester(&context, 47953);

        req.send(&[][..], 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("a malformed request went unanswered, wedging the REQ/REP pair");
        assert_eq!(read_string(&bytes), NO_DATA_SENTINEL);

        req.send(get_request("anything"), 0).unwrap();
        let bytes = req
            .recv_bytes(0)
            .expect("the server stopped answering after a malformed request");
        assert_eq!(read_string(&bytes), NO_DATA_SENTINEL);

        server.stop();
    }
}
