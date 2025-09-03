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
    tarwyn::{
        GetDataCommand, Publish, Push, Reply, Request, SendDataCommand, SupportedValues, publish,
        push, reply, request, supported_values,
    },
};

const DEFAULT_REQ_PORT: u16 = ports::DEFAULT_REQ_REP_PORT;
const DEFAULT_SUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PUSH_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

type Listener = Box<dyn Fn(&supported_values::Kind) + Send + 'static>;
type ListenerMap = Arc<Mutex<HashMap<String, SlotMap<DefaultKey, Listener>>>>;

pub struct TarwynClient {
    listeners: ListenerMap,
    push_socket: zmq::Socket,
    sub_socket: Arc<Mutex<zmq::Socket>>,
    req_socket: Rc<zmq::Socket>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
}

impl TarwynClient {
    pub fn new() -> Self {
        let context = Context::new();

        let listeners: ListenerMap = Arc::new(Mutex::new(HashMap::new()));

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

        sub_socket.lock().unwrap().set_subscribe(b"").unwrap();
        push_socket.set_rcvhwm(500).unwrap();
        push_socket.set_sndhwm(500).unwrap();

        TarwynClient {
            listeners,
            push_socket,
            sub_socket,
            req_socket,
            stop,
            initialized,
        }
    }

    fn construct_push_message(channel: &str, data: supported_values::Kind) -> Vec<u8> {
        Push {
            payload: Some(push::Payload::Send(SendDataCommand {
                channel: channel.to_string(),
                value: Some(SupportedValues { kind: Some(data) }),
            })),
        }
        .encode_to_vec()
    }

    fn construct_request_message(channel: &str) -> Vec<u8> {
        Request {
            payload: Some(request::Payload::Get(GetDataCommand {
                channel: channel.to_string(),
            })),
        }
        .encode_to_vec()
    }

    pub fn send_string(&self, channel: &str, data: &str) {
        let message =
            Self::construct_push_message(channel, supported_values::Kind::String(data.to_string()));

        self.push_socket.send(message, 0).expect("failed to send");
    }

    pub fn send_long(&self, channel: &str, data: i64) {
        let message = Self::construct_push_message(channel, supported_values::Kind::Int64(data));

        self.push_socket.send(message, 0).expect("failed to send");
    }

    pub fn get(&self, channel: &str) -> supported_values::Kind {
        let channel = channel.to_string();
        let req_socket = self.req_socket.clone();

        let message = Self::construct_request_message(&channel);

        req_socket.send(message, 0).unwrap();
        let buffer = Cursor::new(req_socket.recv_bytes(0).unwrap());
        let payload = Reply::decode(buffer).unwrap().payload.unwrap();
        match &payload {
            reply::Payload::Send(command) => command
                .value
                .as_ref()
                .unwrap()
                .kind
                .as_ref()
                .unwrap()
                .clone(),
        }
    }

    pub fn subscribe<F>(&self, channel: &str, callback: F) -> impl FnOnce() + Send + 'static
    where
        F: Fn(&supported_values::Kind) + Send + 'static,
    {
        let initial_value = self.get(channel);
        callback(&initial_value);

        let mut listeners = self.listeners.lock().unwrap();
        let callback = Box::new(callback);
        let key = listeners
            .entry(channel.to_string())
            .or_default()
            .insert(Box::new(callback));

        let listeners = Arc::clone(&self.listeners);
        let channel = channel.to_string();
        move || {
            let mut listeners = listeners.lock().unwrap();
            if let Some(slotmap) = listeners.get_mut(&channel) {
                slotmap.remove(key);
            }
        }
    }

    pub fn start(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            println!("Initializing Tarwyn client...");
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            println!("Starting Tarwyn client...");
            self.stop.store(false, Ordering::SeqCst);
        } else {
            println!("Tarwyn client is already running.");
            return;
        }
        {
            let sub_socket = self.sub_socket.clone();
            let listeners = self.listeners.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            task::spawn_blocking(move || {
                let sub_socket = sub_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let bytes = sub_socket.recv_bytes(0).unwrap();
                    let data = Publish::decode(Cursor::new(bytes)).unwrap();
                    let payload = &data.payload.unwrap();

                    match payload {
                        publish::Payload::Send(command) => {
                            let mut listeners = listeners.lock().unwrap();
                            let channel = &command.channel;
                            let data = command.value.clone().unwrap().kind.unwrap();
                            listeners
                                .entry(channel.clone())
                                .or_default()
                                .iter()
                                .for_each(|(_, callback)| {
                                    callback(&data);
                                });
                        }
                    }
                }
            });
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        println!("Stopping tarwyn client...");
    }
}
