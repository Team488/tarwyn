use std::{
    collections::HashMap,
    io::Cursor,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use prost::Message;

use slotmap::{DefaultKey, SlotMap};
use tokio::task;
use zmq::{
    Context,
    SocketType::{PUSH, REQ, SUB},
};

use crate::{
    ports,
    protobuf::{
        GetDataCommand, GetLogsCommand, Publish, Push, Reply, Request, SendDataCommand,
        SupportedValues, publish, push, reply, request, supported_values,
    },
};

const DEFAULT_REQ_PORT: u16 = ports::DEFAULT_REQ_REP_PORT;
const DEFAULT_SUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PUSH_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

type SubscribeListener = Box<dyn Fn(&supported_values::Kind) + Send + 'static>;
type SubscribeListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, SubscribeListener>>>>;

type LogListener = Box<dyn Fn(&String) + Send + 'static>;
type LogListenerMap = Arc<Mutex<SlotMap<DefaultKey, LogListener>>>;

pub struct TarwynClient {
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    push_socket: zmq::Socket,
    sub_socket: Arc<Mutex<zmq::Socket>>,
    req_socket: Rc<zmq::Socket>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
}

impl TarwynClient {
    pub fn new() -> Self {
        let context = Context::new();

        let listeners: SubscribeListenerMap = Arc::new(Mutex::new(HashMap::new()));
        let log_listeners: LogListenerMap = Arc::new(Mutex::new(SlotMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let push_socket = context.socket(PUSH).unwrap();
        let req_socket = Rc::new(context.socket(REQ).unwrap());
        let sub_socket = Arc::new(Mutex::new(context.socket(SUB).unwrap()));

        push_socket
            .connect(&format!("tcp://:{}", DEFAULT_PUSH_PORT))
            .unwrap();

        req_socket
            .connect(&format!("tcp://:{}", DEFAULT_REQ_PORT))
            .unwrap();

        sub_socket
            .lock()
            .unwrap()
            .connect(&format!("tcp://:{}", DEFAULT_SUB_PORT))
            .unwrap();

        push_socket.set_rcvhwm(500).unwrap();
        push_socket.set_sndhwm(500).unwrap();

        TarwynClient {
            data_listeners: listeners,
            push_socket,
            sub_socket,
            req_socket,
            stop,
            initialized,
            log_listeners,
        }
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
        self.push_socket.send(message, 0).expect("failed to send");
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

    pub fn get(&self, channel: &str) -> supported_values::Kind {
        let channel = channel.to_string();
        let req_socket = self.req_socket.clone();

        let message = Self::request_data(&channel);

        req_socket.send(message, 0).unwrap();
        let buffer = Cursor::new(req_socket.recv_bytes(0).unwrap());
        let payload = Reply::decode(buffer).unwrap().payload.unwrap();

        match &payload {
            reply::Payload::Data(command) => command
                .value
                .as_ref()
                .unwrap()
                .kind
                .as_ref()
                .unwrap()
                .clone(),

            _ => panic!("Unexpected reply payload type received"),
        }
    }

    fn get_logs(&self) -> Vec<String> {
        let req_socket = self.req_socket.clone();

        let message = Self::request_log();

        req_socket.send(message, 0).unwrap();
        let buffer = Cursor::new(req_socket.recv_bytes(0).unwrap());
        let payload = Reply::decode(buffer).unwrap().payload.unwrap();

        match &payload {
            reply::Payload::Logs(command) => command.logs.clone(),

            _ => panic!("Unexpected reply payload type received"),
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

        let initial_value = self.get(channel);
        callback(&initial_value);

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

            task::spawn_blocking(move || {
                let sub_socket = sub_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let topic = sub_socket.recv_string(0).unwrap().unwrap();
                    let bytes = sub_socket.recv_bytes(0).unwrap();
                    let data = Publish::decode(Cursor::new(bytes)).unwrap();
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
