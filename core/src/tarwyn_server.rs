use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::utils::{log::LOGGER, ports, ring_buffer::RingBuffer};
use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    Publish, Push, Reply, ReplyDataCommand, ReplyLogsCommand, Request, SendDataCommand,
    SendLogsCommand, SupportedValues, publish, push, reply, request, supported_values,
};

use zmq::{
    Context, SNDMORE,
    SocketType::{PUB, PULL, REP},
};

const DEFAULT_REP_PORT: u16 = ports::DEFAULT_REQ_REP_PORT;
const DEFAULT_PUB_PORT: u16 = ports::DEFAULT_PUB_SUB_PORT;
const DEFAULT_PULL_PORT: u16 = ports::DEFAULT_PUSH_PULL_PORT;

pub struct TarwynServer {
    pub_socket: Arc<Mutex<zmq::Socket>>,
    pull_socket: Arc<Mutex<zmq::Socket>>,
    rep_socket: Arc<Mutex<zmq::Socket>>,
    cached_messages: Arc<Mutex<HashMap<String, RingBuffer<supported_values::Kind>>>>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
}

impl TarwynServer {
    pub fn new() -> Self {
        let context = Context::new();

        let cached_messages = Arc::new(Mutex::new(HashMap::new()));

        let stop = Arc::new(AtomicBool::new(false));
        let initialized = Arc::new(AtomicBool::new(false));

        let pub_socket = Arc::new(Mutex::new(context.socket(PUB).unwrap()));
        let pull_socket = Arc::new(Mutex::new(context.socket(PULL).unwrap()));
        let rep_socket = Arc::new(Mutex::new(context.socket(REP).unwrap()));

        pub_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", DEFAULT_PUB_PORT))
            .unwrap();
        pull_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", DEFAULT_PULL_PORT))
            .unwrap();
        rep_socket
            .lock()
            .unwrap()
            .bind(&format!("tcp://*:{}", DEFAULT_REP_PORT))
            .unwrap();

        TarwynServer {
            pub_socket,
            pull_socket,
            rep_socket,
            cached_messages,
            stop,
            initialized,
        }
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
                    let bytes = pull_socket.recv_bytes(0).unwrap();

                    let push_request = Push::decode(&bytes[..]).unwrap();
                    let payload = push_request.payload.unwrap();

                    match payload {
                        push::Payload::Send(command) => {
                            let channel = command.channel;
                            let data = command.value.unwrap().kind.unwrap();
                            let mut ring_buffer = cached_messages.lock().unwrap();
                            let ring_buffer = ring_buffer
                                .entry(channel.clone())
                                .or_insert(RingBuffer::new(100));

                            let pub_socket = pub_socket.lock().unwrap();

                            match data {
                                supported_values::Kind::Int64(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Int64(data),
                                    );
                                    ring_buffer.push(supported_values::Kind::Int64(data));
                                    info!("Publishing Int64 data on channel {}: {}", channel, data);
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Int32(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Int32(data),
                                    );
                                    info!("Publishing Int32 data on channel {}: {}", channel, data);
                                    ring_buffer.push(supported_values::Kind::Int32(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Uint32(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Uint32(data),
                                    );
                                    info!(
                                        "Publishing Uint32 data on channel {}: {}",
                                        channel, data
                                    );
                                    ring_buffer.push(supported_values::Kind::Uint32(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Uint64(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Uint64(data),
                                    );
                                    info!(
                                        "Publishing Uint64 data on channel {}: {}",
                                        channel, data
                                    );
                                    ring_buffer.push(supported_values::Kind::Uint64(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Bool(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Bool(data),
                                    );
                                    info!("Publishing Bool data on channel {}: {}", channel, data);
                                    ring_buffer.push(supported_values::Kind::Bool(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Double(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Double(data),
                                    );
                                    info!(
                                        "Publishing Double data on channel {}: {}",
                                        channel, data
                                    );
                                    ring_buffer.push(supported_values::Kind::Double(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Float(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Float(data),
                                    );
                                    info!("Publishing Float data on channel {}: {}", channel, data);
                                    ring_buffer.push(supported_values::Kind::Float(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::String(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::String(data.clone()),
                                    );
                                    info!(
                                        "Publishing String data on channel {}: {}",
                                        channel, data
                                    );
                                    ring_buffer.push(supported_values::Kind::String(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::Bytes(data) => {
                                    let message = Self::publish_data(
                                        &channel,
                                        supported_values::Kind::Bytes(data.clone()),
                                    );
                                    info!("Publishing bytes data on channel {}", channel);
                                    ring_buffer.push(supported_values::Kind::Bytes(data));
                                    pub_socket.send(&channel, SNDMORE).unwrap();
                                    pub_socket.send(message, 0).unwrap();
                                }
                            }
                        }
                    }
                }
            });
        }

        {
            let pub_socket = self.pub_socket.clone();
            let stop = self.stop.clone();

            std::thread::spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let logs = LOGGER.read_unread_logs();
                    if let Some(logs) = logs {
                        let value = Publish {
                            payload: Some(publish::Payload::Logs(SendLogsCommand { logs })),
                        }
                        .encode_to_vec();
                        pub_socket
                            .lock()
                            .unwrap()
                            .send("TARWYN_INTERNAL_LOG", SNDMORE)
                            .unwrap();
                        pub_socket.lock().unwrap().send(value, 0).unwrap();
                    }
                }
            });
        }

        {
            let cached_buffers = self.cached_messages.clone();
            let rep_socket = self.rep_socket.clone();
            let stop = self.stop.clone();

            std::thread::spawn(move || {
                let rep_socket = rep_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }

                    let bytes = rep_socket.recv_bytes(0).unwrap();

                    let request = Request::decode(&bytes[..]).unwrap();
                    let payload = request.payload.unwrap();

                    match payload {
                        request::Payload::Data(command) => {
                            let channel = command.channel;
                            let mut ring_buffer = cached_buffers.lock().unwrap();
                            let ring_buffer = ring_buffer
                                .entry(channel)
                                .or_insert_with(|| RingBuffer::new(100));

                            let data: supported_values::Kind = ring_buffer
                                .peek()
                                .unwrap_or(&supported_values::Kind::String(String::from(
                                    "TARWYN_INTERNAL_NO_DATA_AVAILABLE",
                                )))
                                .clone();

                            let message = Reply {
                                payload: Some(reply::Payload::Data(ReplyDataCommand {
                                    value: Some(SupportedValues { kind: Some(data) }),
                                })),
                            }
                            .encode_to_vec();

                            rep_socket.send(message, 0).unwrap();
                        }
                        request::Payload::Logs(_) => {
                            let logs = LOGGER.get_logs();
                            if let Some(logs) = logs {
                                info!("Sending logs in response to request.");
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand { logs })),
                                }
                                .encode_to_vec();

                                rep_socket.send(message, 0).unwrap();
                            } else {
                                let message = Reply {
                                    payload: Some(reply::Payload::Logs(ReplyLogsCommand {
                                        logs: vec![],
                                    })),
                                }
                                .encode_to_vec();

                                rep_socket.send(message, 0).unwrap();
                            }
                        }
                    }
                }
            });
        }
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
