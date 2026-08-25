use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use prost::Message;
use slotmap::{DefaultKey, SlotMap};

use tarwyn_protobuf::protobuf::{
    GetDataCommand, GetLogsCommand, Publish, Push, Reply, Request, SendDataCommand,
    SupportedValues, publish, push, reply, request, supported_values,
};

use zmq::{
    Context,
    SocketType::{PUSH, REQ, SUB},
};

use crate::ports;

const NO_DATA_SENTINEL: &str = "TARWYN_INTERNAL_NO_DATA_AVAILABLE";

#[derive(Clone, Debug)]
pub struct TarwynConfig {
    pub host: String,
    pub push_port: u16,
    pub req_port: u16,
    pub sub_port: u16,
    pub request_timeout: Duration,
}

impl Default for TarwynConfig {
    fn default() -> Self {
        TarwynConfig {
            host: "127.0.0.1".to_string(),
            push_port: ports::DEFAULT_PUSH_PULL_PORT,
            req_port: ports::DEFAULT_REQ_REP_PORT,
            sub_port: ports::DEFAULT_PUB_SUB_PORT,
            request_timeout: Duration::from_millis(500),
        }
    }
}

type SubscribeListener = Box<dyn Fn(&supported_values::Kind) + Send + 'static>;
type SubscribeListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

type LogListener = Box<dyn Fn(&String) + Send + 'static>;
type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

pub struct TarwynClient {
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    push_socket: Mutex<zmq::Socket>,
    sub_socket: Arc<Mutex<zmq::Socket>>,
    req_socket: Mutex<zmq::Socket>,
    request_timeout: Duration,
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

        for socket in [&push_socket, &req_socket, &sub_socket] {
            socket.set_linger(0).unwrap();
        }

        req_socket.set_req_relaxed(true).unwrap();
        req_socket.set_req_correlate(true).unwrap();
        let timeout_ms = config.request_timeout.as_millis().min(i32::MAX as u128) as i32;
        req_socket.set_rcvtimeo(timeout_ms).unwrap();
        req_socket.set_sndtimeo(timeout_ms).unwrap();

        push_socket.set_rcvhwm(500).unwrap();
        push_socket.set_sndhwm(500).unwrap();

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
            req_socket: Mutex::new(req_socket),
            request_timeout: config.request_timeout,
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

    fn send_message(&self, channel: &str, kind: supported_values::Kind) {
        let message = Self::push_data(channel, kind);
        if let Ok(socket) = self.push_socket.lock() {
            let _ = socket.send(message, zmq::DONTWAIT);
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
            reply::Payload::Logs(_) => None,
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
        let sub_socket = self.sub_socket.clone();
        sub_socket
            .lock()
            .unwrap()
            .set_subscribe(channel.as_bytes())
            .unwrap();

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
        let channel = channel.to_string();

        move || {
            let mut listeners = listeners.lock().unwrap();
            if let Some(slotmap) = listeners.get_mut(&channel) {
                slotmap.remove(key);
                if slotmap.is_empty() {
                    listeners.remove(&channel);
                    sub_socket
                        .lock()
                        .unwrap()
                        .set_unsubscribe(channel.as_bytes())
                        .unwrap();
                }
            }
        }
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
            let data_listeners = self.data_listeners.clone();
            let log_listeners = self.log_listeners.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            std::thread::spawn(move || {
                let sub_socket = sub_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let topic = sub_socket.recv_string(0).unwrap().unwrap();
                    let bytes = sub_socket.recv_bytes(0).unwrap();
                    let data = Publish::decode(&bytes[..]).unwrap();
                    let payload = &data.payload.unwrap();

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
}
