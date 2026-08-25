use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use prost::Message;
use slotmap::{DefaultKey, SlotMap};

use tarwyn_protobuf::protobuf::{
    BoolList, BytesList, FloatList, GetDataCommand, GetLogsCommand, Publish, Push,
    RegisterTelemetryCommand, Reply, Request, SendDataCommand, StringList, SupportedValues,
    publish, push, reply, request, supported_values,
};
use tarwyn_protobuf::telemetry;

use zmq::{
    Context,
    SocketType::{PUSH, REQ, SUB},
};

use crate::ports;

const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";

const POLL_INTERVAL_MS: i32 = 100;

enum TopicChange {
    Subscribe(String),
    Unsubscribe(String),
}

#[derive(Clone, Debug)]
pub struct TarwynConfig {
    pub host: String,
    pub push_port: u16,
    pub req_port: u16,
    pub sub_port: u16,
    pub request_timeout: Duration,
    pub send_high_water_mark: i32,
    pub telemetry_port: u16,
}

impl Default for TarwynConfig {
    fn default() -> Self {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: ports::DEFAULT_PUSH_PULL_PORT,
            req_port: ports::DEFAULT_REQ_REP_PORT,
            sub_port: ports::DEFAULT_PUB_SUB_PORT,
            request_timeout: Duration::from_millis(500),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
    }
}

type SubscribeListener = Box<dyn Fn(&supported_values::Kind) + Send + 'static>;
type SubscribeListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

type LogListener = Box<dyn Fn(&String) + Send + 'static>;
type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

pub struct CachedSubscriber {
    values: Arc<Mutex<VecDeque<supported_values::Kind>>>,
}

impl CachedSubscriber {
    pub fn read_all(&self) -> Vec<supported_values::Kind> {
        match self.values.lock() {
            Ok(mut values) => values.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn latest(&self) -> Option<supported_values::Kind> {
        self.values.lock().ok()?.back().cloned()
    }

    pub fn len(&self) -> usize {
        self.values.lock().map(|v| v.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct TarwynClient {
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    push_socket: Mutex<zmq::Socket>,
    sub_socket: Arc<Mutex<zmq::Socket>>,
    topic_changes: Arc<Mutex<Vec<TopicChange>>>,
    telemetry_socket: Arc<std::net::UdpSocket>,
    telemetry_target: std::net::SocketAddr,
    telemetry_listeners: SubscribeListenerMap,
    telemetry_started: Arc<AtomicBool>,
    req_socket: Mutex<zmq::Socket>,
    request_timeout: Duration,
    dropped: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
}

impl TarwynClient {
    pub fn new() -> Self {
        Self::with_config(TarwynConfig::default())
    }

    pub fn connect(host: &str) -> Self {
        Self::with_config(TarwynConfig {
            host: host.to_string(),
            ..Default::default()
        })
    }

    pub fn with_config(config: TarwynConfig) -> Self {
        let context = Context::new();

        let listeners: SubscribeListenerMap = Arc::new(Mutex::new(HashMap::new()));
        let log_listeners: LogListenerMap = Arc::new(Mutex::new(SlotMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let push_socket = context.socket(PUSH).unwrap();
        let req_socket = context.socket(REQ).unwrap();
        let sub_socket = context.socket(SUB).unwrap();
        sub_socket.set_rcvtimeo(POLL_INTERVAL_MS).unwrap();

        for socket in [&push_socket, &req_socket, &sub_socket] {
            socket.set_linger(0).unwrap();
        }

        req_socket.set_req_relaxed(true).unwrap();
        req_socket.set_req_correlate(true).unwrap();
        let timeout_ms = config.request_timeout.as_millis().min(i32::MAX as u128) as i32;
        req_socket.set_rcvtimeo(timeout_ms).unwrap();
        req_socket.set_sndtimeo(timeout_ms).unwrap();

        push_socket.set_rcvhwm(config.send_high_water_mark).unwrap();
        push_socket.set_sndhwm(config.send_high_water_mark).unwrap();

        push_socket
            .connect(&format!("tcp://{}:{}", config.host, config.push_port))
            .unwrap();
        req_socket
            .connect(&format!("tcp://{}:{}", config.host, config.req_port))
            .unwrap();
        sub_socket
            .connect(&format!("tcp://{}:{}", config.host, config.sub_port))
            .unwrap();

        TarwynClient {
            data_listeners: listeners,
            push_socket: Mutex::new(push_socket),
            sub_socket: Arc::new(Mutex::new(sub_socket)),
            topic_changes: Arc::new(Mutex::new(Vec::new())),
            telemetry_socket: Arc::new(
                telemetry::bind_ephemeral().expect("could not bind a telemetry socket"),
            ),
            telemetry_target: format!("{}:{}", config.host, config.telemetry_port)
                .parse()
                .unwrap_or_else(|_| {
                    std::net::SocketAddr::from(([127, 0, 0, 1], config.telemetry_port))
                }),
            telemetry_listeners: Arc::new(Mutex::new(HashMap::new())),
            telemetry_started: Arc::new(AtomicBool::new(false)),
            req_socket: Mutex::new(req_socket),
            request_timeout: config.request_timeout,
            dropped: Arc::new(AtomicU64::new(0)),
            stop,
            initialized,
            log_listeners,
        }
    }

    fn request(&self, message: Vec<u8>) -> Option<reply::Payload> {
        let socket = self.req_socket.lock().ok()?;
        socket.send(message, 0).ok()?;
        let bytes = socket.recv_bytes(0).ok()?;
        Reply::decode(&bytes[..]).ok()?.payload
    }

    fn push_data(channel: &str, data: supported_values::Kind) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues { kind: Some(data) }),
            })),
        }
        .encode_to_vec()
    }

    fn request_data(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Data(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    fn request_log() -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Logs(GetLogsCommand {})),
        }
        .encode_to_vec()
    }

    pub fn send_message_public(&self, channel: &str, kind: supported_values::Kind) {
        self.send_message(channel, kind);
    }

    fn send_message(&self, channel: &str, kind: supported_values::Kind) {
        let message = Self::push_data(channel, kind);
        if let Ok(socket) = self.push_socket.lock()
            && socket.send(message, zmq::DONTWAIT).is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn send_string(&self, channel: &str, data: &str) {
        self.send_message(channel, supported_values::Kind::String(data.to_string()));
    }

    pub fn send_i32(&self, channel: &str, data: i32) {
        self.send_message(channel, supported_values::Kind::Int32(data));
    }

    pub fn send_i64(&self, channel: &str, data: i64) {
        self.send_message(channel, supported_values::Kind::Int64(data));
    }

    pub fn send_u32(&self, channel: &str, data: u32) {
        self.send_message(channel, supported_values::Kind::Uint32(data));
    }

    pub fn send_u64(&self, channel: &str, data: u64) {
        self.send_message(channel, supported_values::Kind::Uint64(data));
    }

    pub fn send_bool(&self, channel: &str, data: bool) {
        self.send_message(channel, supported_values::Kind::Bool(data));
    }

    pub fn send_double(&self, channel: &str, data: f64) {
        self.send_message(channel, supported_values::Kind::Double(data));
    }

    pub fn send_float(&self, channel: &str, data: f32) {
        self.send_message(channel, supported_values::Kind::Float(data));
    }

    pub fn send_bytes(&self, channel: &str, data: &[u8]) {
        self.send_message(channel, supported_values::Kind::Bytes(data.to_vec()));
    }

    pub fn send_string_list(&self, channel: &str, data: &[String]) {
        self.send_message(
            channel,
            supported_values::Kind::StringList(StringList {
                values: data.to_vec(),
            }),
        );
    }

    pub fn send_float_list(&self, channel: &str, data: &[f32]) {
        self.send_message(
            channel,
            supported_values::Kind::FloatList(FloatList {
                values: data.to_vec(),
            }),
        );
    }

    pub fn send_bytes_list(&self, channel: &str, data: &[Vec<u8>]) {
        self.send_message(
            channel,
            supported_values::Kind::BytesList(BytesList {
                values: data.to_vec(),
            }),
        );
    }

    pub fn send_bool_list(&self, channel: &str, data: &[bool]) {
        self.send_message(
            channel,
            supported_values::Kind::BoolList(BoolList {
                values: data.to_vec(),
            }),
        );
    }

    pub fn publish_telemetry(&self, channel: &str, payload: &[u8]) {
        let mut buf = vec![0u8; telemetry::HEADER_LEN + payload.len()];
        let len = telemetry::encode(
            &mut buf,
            telemetry::topic_hash(channel),
            telemetry::now_micros(),
            payload,
        );
        if self
            .telemetry_socket
            .send_to(&buf[..len], self.telemetry_target)
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn subscribe_telemetry<F>(&self, channel: &str, callback: F) -> bool
    where
        F: Fn(&supported_values::Kind) + Send + 'static,
    {
        let Ok(local) = self.telemetry_socket.local_addr() else {
            return false;
        };
        let address = format!("{}:{}", self.telemetry_target.ip(), local.port());
        let message = Request {
            payload: Some(request::Payload::RegisterTelemetry(
                RegisterTelemetryCommand {
                    channel: channel.to_string(),
                    address,
                },
            )),
        }
        .encode_to_vec();

        let registered = matches!(
            self.request(message),
            Some(reply::Payload::Telemetry(ack)) if ack.registered
        );
        if !registered {
            return false;
        }

        if let Ok(mut listeners) = self.telemetry_listeners.lock() {
            listeners
                .entry(channel.to_string())
                .or_default()
                .insert(Box::new(Box::new(callback)));
        }
        self.start_telemetry_receiver();
        true
    }

    fn start_telemetry_receiver(&self) {
        if self.telemetry_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let socket = Arc::clone(&self.telemetry_socket);
        let listeners = Arc::clone(&self.telemetry_listeners);
        let stop = Arc::clone(&self.stop);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

        std::thread::spawn(move || {
            let mut buf = vec![0u8; telemetry::MAX_DATAGRAM];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok((len, _from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((channel_hash, _timestamp, payload)) = telemetry::decode(&buf[..len])
                else {
                    continue;
                };
                let value = supported_values::Kind::Bytes(payload.to_vec());
                if let Ok(listeners) = listeners.lock() {
                    for (channel, slots) in listeners.iter() {
                        if telemetry::topic_hash(channel) == channel_hash {
                            for (_, callback) in slots.iter() {
                                callback(&value);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn dropped_publishes(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn get(&self, channel: &str) -> Option<supported_values::Kind> {
        match self.request(Self::request_data(channel))? {
            reply::Payload::Data(command) => {
                let kind = command.value?.kind?;
                if kind == supported_values::Kind::String(NO_DATA_SENTINEL.to_string()) {
                    None
                } else {
                    Some(kind)
                }
            }
            reply::Payload::Logs(_) | reply::Payload::Telemetry(_) => None,
        }
    }

    fn get_logs(&self) -> Vec<String> {
        match self.request(Self::request_log()) {
            Some(reply::Payload::Logs(command)) => command.logs,
            _ => Vec::new(),
        }
    }

    pub fn subscribe<F>(&self, channel: &str, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&supported_values::Kind) + Send + 'static,
    {
        self.queue_topic_change(TopicChange::Subscribe(channel.to_string()));

        if let Some(initial_value) = self.get(channel) {
            callback(&initial_value);
        }

        let mut listeners = self.data_listeners.lock().unwrap();
        let callback = Box::new(callback);
        let key = listeners
            .entry(channel.to_string())
            .or_default()
            .insert(Box::new(callback));

        let listeners = Arc::clone(&self.data_listeners);
        let topic_changes = Arc::clone(&self.topic_changes);
        let channel = channel.to_string();

        move || {
            let mut listeners = listeners.lock().unwrap();
            if let Some(slotmap) = listeners.get_mut(&channel) {
                slotmap.remove(key);
                if slotmap.is_empty() {
                    listeners.remove(&channel);
                    if let Ok(mut pending) = topic_changes.lock() {
                        pending.push(TopicChange::Unsubscribe(channel.clone()));
                    }
                }
            }
        }
    }

    fn queue_topic_change(&self, change: TopicChange) {
        if let Ok(mut pending) = self.topic_changes.lock() {
            pending.push(change);
        }
    }

    pub fn subscribe_cached(
        &self,
        channel: &str,
        depth: usize,
    ) -> (CachedSubscriber, impl FnOnce() + Send + 'static) {
        let values = Arc::new(Mutex::new(VecDeque::with_capacity(depth.max(1))));
        let sink = Arc::clone(&values);
        let depth = depth.max(1);
        let unsubscribe = self.subscribe(channel, move |value| {
            if let Ok(mut buffered) = sink.lock() {
                if buffered.len() == depth {
                    buffered.pop_front();
                }
                buffered.push_back(value.clone());
            }
        });
        (CachedSubscriber { values }, unsubscribe)
    }

    pub fn subscribe_to_logs<F>(&self, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&String) + Send + 'static,
    {
        let sub_socket = self.sub_socket.clone();

        sub_socket
            .lock()
            .unwrap()
            .set_subscribe("TARWYN_INTERNAL_LOG".as_bytes())
            .unwrap();

        let initial_value = self.get_logs();

        initial_value.iter().for_each(|log| {
            callback(log);
        });

        let mut listeners = self.log_listeners.lock().unwrap();
        let callback = Box::new(callback);

        let key = listeners.insert(Box::new(callback));

        let listeners = Arc::clone(&self.log_listeners);

        move || {
            listeners.lock().unwrap().remove(key);
            if listeners.lock().unwrap().is_empty() {
                sub_socket
                    .lock()
                    .unwrap()
                    .set_unsubscribe("TARWYN_INTERNAL_LOG".as_bytes())
                    .unwrap();
            }
        }
    }

    pub fn start(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            self.stop.store(false, Ordering::SeqCst);
        } else {
            return;
        }
        {
            let sub_socket = self.sub_socket.clone();
            let topic_changes = self.topic_changes.clone();
            let data_listeners = self.data_listeners.clone();
            let log_listeners = self.log_listeners.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            std::thread::spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let received = {
                        let Ok(socket) = sub_socket.lock() else {
                            break;
                        };
                        if let Ok(mut pending) = topic_changes.lock() {
                            for change in pending.drain(..) {
                                let _ = match change {
                                    TopicChange::Subscribe(topic) => {
                                        socket.set_subscribe(topic.as_bytes())
                                    }
                                    TopicChange::Unsubscribe(topic) => {
                                        socket.set_unsubscribe(topic.as_bytes())
                                    }
                                };
                            }
                        }
                        match socket.recv_string(0) {
                            Ok(Ok(topic)) => socket.recv_bytes(0).ok().map(|bytes| (topic, bytes)),
                            _ => None,
                        }
                    };
                    let Some((topic, bytes)) = received else {
                        continue;
                    };
                    let Ok(data) = Publish::decode(&bytes[..]) else {
                        continue;
                    };
                    let Some(payload) = data.payload.as_ref() else {
                        continue;
                    };

                    match payload {
                        publish::Payload::Data(command) => {
                            let mut listeners = data_listeners.lock().unwrap();
                            let data = command.value.clone().unwrap().kind.unwrap();

                            listeners
                                .entry(topic)
                                .or_default()
                                .iter()
                                .for_each(|(_, callback)| {
                                    callback(&data);
                                });
                        }
                        publish::Payload::Logs(command) => {
                            let listeners = log_listeners.lock().unwrap();

                            command.logs.iter().for_each(|log| {
                                listeners.iter().for_each(|(_, callback)| {
                                    callback(log);
                                });
                            });
                        }
                    }
                }
            });
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Default for TarwynClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn offline_config() -> TarwynConfig {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47901,
            req_port: 47902,
            sub_port: 47903,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        }
    }

    #[test]
    fn client_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TarwynClient>();
    }

    #[test]
    fn publishes_reach_a_bound_peer() {
        let context = Context::new();
        let pull = context.socket(zmq::SocketType::PULL).unwrap();
        pull.bind("tcp://127.0.0.1:47911").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47911,
            req_port: 47912,
            sub_port: 47913,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let mut received = None;
        for _ in 0..30 {
            client.send_double("probe", 1.5);
            if let Ok(bytes) = pull.recv_bytes(zmq::DONTWAIT) {
                received = Some(bytes);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let bytes = received.expect("no message reached the bound peer within 3s");
        let payload = Push::decode(&bytes[..]).unwrap().payload.unwrap();
        let push::Payload::Send(command) = payload;
        assert_eq!(command.channel, "probe");
        assert_eq!(
            command.value.unwrap().kind.unwrap(),
            supported_values::Kind::Double(1.5)
        );
    }

    #[test]
    fn send_does_not_block_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        for i in 0..100 {
            client.send_double("no-such-channel", i as f64);
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "send should drop rather than block, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn get_returns_none_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        assert!(client.get("no-such-channel").is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "get() should give up after request_timeout, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn subscribe_does_not_block_when_server_is_absent() {
        let client = TarwynClient::with_config(offline_config());
        let started = Instant::now();
        let _unsubscribe = client.subscribe("no-such-channel", |_| {});
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "subscribe() should not block on an absent server, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn request_socket_recovers_after_timeout() {
        let client = TarwynClient::with_config(offline_config());
        assert!(client.get("first").is_none());
        let started = Instant::now();
        assert!(client.get("second").is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "second request wedged after the first timed out, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn publish_drops_are_counted_not_silent() {
        let client = TarwynClient::with_config(TarwynConfig {
            send_high_water_mark: 4,
            ..offline_config()
        });
        for i in 0..200 {
            client.send_double("no-such-channel", i as f64);
        }
        assert!(
            client.dropped_publishes() > 0,
            "publishes past the high water mark should be counted, saw {}",
            client.dropped_publishes()
        );
    }

    #[test]
    fn list_types_survive_the_wire() {
        let context = Context::new();
        let pull = context.socket(zmq::SocketType::PULL).unwrap();
        pull.bind("tcp://127.0.0.1:47921").unwrap();
        pull.set_rcvtimeo(3000).unwrap();

        let client = TarwynClient::with_config(TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: 47921,
            req_port: 47922,
            sub_port: 47923,
            request_timeout: Duration::from_millis(150),
            send_high_water_mark: 500,
            telemetry_port: telemetry::DEFAULT_TELEMETRY_PORT,
        });

        let expected = vec!["alpha".to_string(), "beta".to_string()];
        let mut received = None;
        for _ in 0..30 {
            client.send_string_list("paths", &expected);
            if let Ok(bytes) = pull.recv_bytes(zmq::DONTWAIT) {
                received = Some(bytes);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let bytes = received.expect("no list message reached the bound peer within 3s");
        let push::Payload::Send(command) = Push::decode(&bytes[..]).unwrap().payload.unwrap();
        assert_eq!(command.channel, "paths");
        match command.value.unwrap().kind.unwrap() {
            supported_values::Kind::StringList(list) => assert_eq!(list.values, expected),
            other => panic!("expected a string list, got {other:?}"),
        }
    }

    #[test]
    fn cached_subscriber_keeps_only_its_depth() {
        let client = TarwynClient::with_config(offline_config());
        let (cache, _unsubscribe) = client.subscribe_cached("depth-test", 3);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.latest().is_none());
        assert!(cache.read_all().is_empty());
    }

    #[test]
    fn subscribe_works_after_start() {
        let client = TarwynClient::with_config(offline_config());
        client.start();
        let started = Instant::now();
        let _unsubscribe = client.subscribe("after-start", |_| {});
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "subscribing after start() deadlocked on the sub socket, took {:?}",
            started.elapsed()
        );
        client.stop();
    }
}
