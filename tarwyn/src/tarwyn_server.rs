use std::{
    collections::HashMap,
    io::Cursor,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use prost::Message;
use tokio::task;
use zmq::{
    Context,
    SocketType::{PUB, PULL, REP},
};

use crate::{
    utils::{ports, ring_buffer::RingBuffer},
    tarwyn::{
        DataReplyCommand, Publish, Push, Reply, Request, SendDataCommand, SupportedValues, publish,
        push, reply, request, supported_values,
    },
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

    pub fn start(&self) {
        if self.initialized.load(Ordering::SeqCst) == false {
            println!("Initializing Tarwyn server...");
            self.initialized.store(true, Ordering::SeqCst);
        } else if self.stop.load(Ordering::SeqCst) {
            println!("Starting Tarwyn server...");
            self.stop.store(false, Ordering::SeqCst);
        } else {
            println!("Tarwyn server is already running.");
            return;
        }
        {
            let cached_messages = self.cached_messages.clone();
            let pull_socket = self.pull_socket.clone();
            let pub_socket = self.pub_socket.clone();
            let stop: Arc<AtomicBool> = self.stop.clone();

            task::spawn_blocking(move || {
                let pull_socket = pull_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let bytes = pull_socket.recv_bytes(0).unwrap();

                    let push_request = Push::decode(Cursor::new(bytes)).unwrap();
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
                                supported_values::Kind::LongValue(data) => {
                                    let message = Publish {
                                        payload: Some(publish::Payload::Send(SendDataCommand {
                                            channel: channel,
                                            value: Some(SupportedValues {
                                                kind: Some(supported_values::Kind::LongValue(data)),
                                            }),
                                        })),
                                    }
                                    .encode_to_vec();
                                    ring_buffer.push(supported_values::Kind::LongValue(data));
                                    pub_socket.send(message, 0).unwrap();
                                }
                                supported_values::Kind::StringValue(data) => {
                                    let message = Publish {
                                        payload: Some(publish::Payload::Send(SendDataCommand {
                                            channel: channel,
                                            value: Some(SupportedValues {
                                                kind: Some(supported_values::Kind::StringValue(
                                                    data.clone(),
                                                )),
                                            }),
                                        })),
                                    }
                                    .encode_to_vec();
                                    ring_buffer.push(supported_values::Kind::StringValue(data));
                                    pub_socket.send(message, 0).unwrap();
                                }
                            }
                        }
                    }
                }
            });
        }

        {
            let cached_buffers = self.cached_messages.clone();
            let rep_socket = self.rep_socket.clone();
            let stop = self.stop.clone();

            task::spawn_blocking(move || {
                let rep_socket = rep_socket.lock().unwrap();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }

                    let bytes = rep_socket.recv_bytes(0).unwrap();

                    let request = Request::decode(Cursor::new(bytes)).unwrap();
                    let payload = request.payload.unwrap();

                    match payload {
                        request::Payload::Get(command) => {
                            let channel = command.channel;
                            let mut ring_buffer = cached_buffers.lock().unwrap();
                            let ring_buffer = ring_buffer
                                .entry(channel)
                                .or_insert_with(|| RingBuffer::new(100));

                            let data: supported_values::Kind = ring_buffer
                                .peek()
                                .unwrap_or(&supported_values::Kind::StringValue(String::from("")))
                                .clone()
                                .try_into()
                                .unwrap();

                            let message = Reply {
                                payload: Some(reply::Payload::Send(DataReplyCommand {
                                    value: Some(SupportedValues { kind: Some(data) }),
                                })),
                            }
                            .encode_to_vec();

                            rep_socket.send(message, 0).unwrap();
                        }
                        request::Payload::Register(_command) => {
                            todo!("Implement registration logic");
                        }
                    }
                }
            });
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        println!("Stopping tarwyn server...");
    }
}
