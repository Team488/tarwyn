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
    Publish, Push, Reply, ReplyDataCommand, ReplyLogsCommand, ReplyTelemetryCommand, Request,
    SendDataCommand, SupportedValues, publish, push, reply, request, supported_values,
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
        }
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
