use crate::harness::{HEADER_LEN, Pacer, Recorder, decode, encode};
use prost::Message;
use std::time::Duration;
use tarwyn_protobuf::protobuf::{
    Publish, SendDataCommand, SupportedValues, publish, supported_values,
};
use zmq::{
    Context, SNDMORE,
    SocketType::{PUB, SUB},
};

pub const CHANNEL: &str = "bench";
pub const DEFAULT_ENDPOINT: &str = "tcp://127.0.0.1:48815";

fn wrap(payload: &[u8]) -> Vec<u8> {
    Publish {
        payload: Some(publish::Payload::Data(SendDataCommand {
            channel: CHANNEL.to_string(),
            value: Some(SupportedValues {
                kind: Some(supported_values::Kind::Bytes(payload.to_vec())),
            }),
        })),
    }
    .encode_to_vec()
}

pub fn publish(endpoint: &str, payload: usize, rate_hz: u64, count: u64) -> std::io::Result<()> {
    let context = Context::new();
    let socket = context.socket(PUB).unwrap();
    socket.set_linger(0).unwrap();
    socket.bind(endpoint).unwrap();

    let mut buf = vec![0u8; payload.max(HEADER_LEN)];
    let mut pacer = Pacer::new(rate_hz);

    std::thread::sleep(Duration::from_millis(500));

    for seq in 0..count {
        pacer.wait();
        encode(&mut buf, seq);
        let message = wrap(&buf);
        socket.send(CHANNEL, SNDMORE).unwrap();
        socket.send(message, 0).unwrap();
    }
    println!("sent {count} messages of {} B", buf.len());
    Ok(())
}

pub fn subscribe(endpoint: &str, payload: usize, samples: u64) -> std::io::Result<()> {
    let context = Context::new();
    let socket = context.socket(SUB).unwrap();
    socket.set_linger(0).unwrap();
    socket.connect(endpoint).unwrap();
    socket.set_subscribe(CHANNEL.as_bytes()).unwrap();
    socket.set_rcvtimeo(30_000).unwrap();

    let mut recorder = Recorder::new();
    println!("subscribed on {endpoint}, waiting for {samples} samples...");

    while recorder.len() < samples {
        let Ok(_topic) = socket.recv_bytes(0) else {
            break;
        };
        let Ok(bytes) = socket.recv_bytes(0) else {
            break;
        };
        let Ok(message) = Publish::decode(&bytes[..]) else {
            continue;
        };
        if let Some(publish::Payload::Data(command)) = message.payload
            && let Some(value) = command.value
            && let Some(supported_values::Kind::Bytes(body)) = value.kind
            && let Some((seq, sent)) = decode(&body)
        {
            recorder.record(seq, sent);
        }
    }
    recorder.report("zmq-direct", payload.max(HEADER_LEN));
    Ok(())
}
