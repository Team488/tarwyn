use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::utils::{log::LOGGER, ports, ring_buffer::RingBuffer};
use tarwyn_protobuf::telemetry;

use log::info;
use prost::Message;
use tarwyn_protobuf::protobuf::{
    Publish, Push, Reply, ReplyDataCommand, ReplyLogsCommand, ReplyTelemetryCommand, Request,
    SendDataCommand, SupportedValues, publish, push, reply, request, supported_values,
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
    telemetry_subscribers: Arc<Mutex<HashMap<u32, HashSet<SocketAddr>>>>,
    stop: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
}

impl TarwynServer {
    pub fn new() -> Self {
        let context = Context::new();

        let cached_messages = Arc::new(Mutex::new(HashMap::new()));
        let telemetry_subscribers = Arc::new(Mutex::new(HashMap::new()));

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
            telemetry_subscribers,
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

                            let message = Self::publish_data(&channel, data.clone());
                            ring_buffer.push(data);

                            let pub_socket = pub_socket.lock().unwrap();
                            pub_socket.send(&channel, SNDMORE).unwrap();
                            pub_socket.send(message, 0).unwrap();
                        }
                    }
                }
            });
        }

        self.start_telemetry_relay();

        {
            let cached_buffers = self.cached_messages.clone();
            let telemetry_subscribers = self.telemetry_subscribers.clone();
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
                        request::Payload::RegisterTelemetry(command) => {
                            let registered = command
                                .address
                                .parse::<SocketAddr>()
                                .map(|address| {
                                    if let Ok(mut subscribers) = telemetry_subscribers.lock() {
                                        subscribers
                                            .entry(telemetry::topic_hash(&command.channel))
                                            .or_default()
                                            .insert(address);
                                        true
                                    } else {
                                        false
                                    }
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
                let targets = match subscribers.lock() {
                    Ok(subscribers) => subscribers
                        .get(&channel_hash)
                        .map(|set| set.iter().copied().collect::<Vec<_>>())
                        .unwrap_or_default(),
                    Err(_) => continue,
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
