//! The reader thread: what arrives on the connection, and where it goes.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::Receiver,
};

use prost::Message;
use tungstenite::{Bytes, Message as WebsocketMessage, WebSocket};

use tarwyn_protobuf::protobuf::Reply;

use tarwyn_server::value::Value;
use tarwyn_server::websocket::message::{ControlMessage, ValueMessage};

use crate::client::{LOG_TOPIC, PendingRequest};
use crate::connection::{
    OUTBOUND_POLL, POLL_INTERVAL, ReadHalf, SharedWriter, connect_websocket, drain_outbound,
    drain_outbound_dropped, is_timeout, set_read_timeout, split_connection,
};
use crate::listeners::{
    LogListener, LogListenerMap, SessionState, SubscribeListener, SubscribeListenerMap, Topic,
    TopicNames,
};
use tarwyn_server::websocket::protocol::data_type_from_string;

/// Route a decoded value message to the right listeners by topic name.
pub(crate) fn fan_out_value(
    vm: ValueMessage,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_names: &TopicNames,
) {
    let topic = {
        let names = topic_names.lock().unwrap_or_else(|p| p.into_inner());
        names.get(&vm.topic_id).cloned()
    };
    let Some(Topic { name, data_type }) = topic else {
        return;
    };
    let vm = ValueMessage {
        value: vm.value.conformed(data_type),
        ..vm
    };

    if name == LOG_TOPIC {
        if let Value::StringArray(lines) = vm.value {
            let callbacks: Vec<LogListener> = log_listeners
                .lock()
                .ok()
                .map(|l| l.values().map(Arc::clone).collect())
                .unwrap_or_default();
            for line in &lines {
                for callback in &callbacks {
                    callback(line);
                }
            }
        }
        return;
    }

    let callbacks: Vec<SubscribeListener> = data_listeners
        .lock()
        .ok()
        .map(|l| {
            l.get(&name)
                .map(|slotmap| slotmap.values().map(Arc::clone).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let value = vm.value.widened();
    for callback in callbacks {
        callback(&value);
    }
}

/// Handle one binary frame: a batch of value messages, a control reply, or
/// noise.
///
/// A reply is handed over only if its id matches the pending request (or is 0
/// from an older server).
pub(crate) fn handle_binary(
    payload: Bytes,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_names: &TopicNames,
    pending: &Arc<Mutex<Option<PendingRequest>>>,
) {
    if let Ok(messages) = ValueMessage::decode_all(&payload) {
        for vm in messages {
            fan_out_value(vm, data_listeners, log_listeners, topic_names);
        }
        return;
    }
    let Ok(reply) = Reply::decode(&payload[..]) else {
        return;
    };
    let Ok(mut pending) = pending.lock() else {
        return;
    };
    let matches = pending
        .as_ref()
        .is_some_and(|waiting| reply.id == 0 || reply.id == waiting.id);
    if matches && let Some(waiting) = pending.take() {
        let _ = waiting.reply.send(payload.to_vec());
    }
}

/// Handle one text frame: an NT4 announcement, which corrects the topic map.
pub(crate) fn handle_text(text: String, topic_names: &TopicNames) {
    if let Ok(ControlMessage::Announce {
        name,
        id,
        data_type,
        ..
    }) = ControlMessage::from_json(&text)
    {
        let data_type = data_type_from_string(&data_type);
        topic_names
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, Topic { name, data_type });
    }
}

/// Re-send the session's publishes and subscriptions. Returns whether every
/// frame went out.
pub(crate) fn replay_session(websocket: &mut WebSocket<ReadHalf>, session: &SessionState) -> bool {
    let frames: Vec<Vec<u8>> = {
        let registered = session.lock().unwrap_or_else(|p| p.into_inner());
        registered.values().cloned().collect()
    };
    for frame in frames {
        if websocket.send(WebsocketMessage::binary(frame)).is_err() {
            return false;
        }
    }
    true
}

/// The single connection owner: connects (retrying), replays the session,
/// drains outbound, and demuxes inbound frames until told to stop.
#[expect(clippy::too_many_arguments)]
pub(crate) fn reader_loop(
    outbound: Receiver<Vec<u8>>,
    url: String,
    subprotocol: String,
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    topic_names: TopicNames,
    pending: Arc<Mutex<Option<PendingRequest>>>,
    session: SessionState,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    reader_alive: Arc<AtomicBool>,
    writer: SharedWriter,
    busy_poll: std::time::Duration,
    predict: std::time::Duration,
) {
    reader_alive.store(true, Ordering::SeqCst);

    'outer: loop {
        *writer.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let (mut websocket, spare) = match connect_websocket(&url, &subprotocol) {
            Ok(websocket) => split_connection(websocket, &writer, busy_poll, predict),
            Err(_) => {
                drain_outbound_dropped(&outbound, &dropped);
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
        };
        let poll = if spare.is_some() {
            POLL_INTERVAL
        } else {
            OUTBOUND_POLL
        };
        set_read_timeout(&websocket, poll);

        topic_names
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        if !replay_session(&mut websocket, &session) {
            continue;
        }
        if !drain_outbound(&mut websocket, &outbound) {
            continue;
        }
        *writer.lock().unwrap_or_else(|p| p.into_inner()) = spare;

        loop {
            if stop.load(Ordering::SeqCst) {
                break 'outer;
            }
            if !drain_outbound(&mut websocket, &outbound) {
                break;
            }
            match websocket.read() {
                Ok(WebsocketMessage::Binary(payload)) => {
                    handle_binary(
                        payload,
                        &data_listeners,
                        &log_listeners,
                        &topic_names,
                        &pending,
                    );
                }
                Ok(WebsocketMessage::Text(text)) => handle_text(text.to_string(), &topic_names),
                Ok(WebsocketMessage::Ping(payload)) => {
                    let _ = websocket.send(WebsocketMessage::Pong(payload));
                }
                Ok(WebsocketMessage::Pong(_)) => {}
                Ok(WebsocketMessage::Close(_)) => break,
                Ok(WebsocketMessage::Frame(_)) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => break,
            }
        }
    }

    reader_alive.store(false, Ordering::SeqCst);
    *writer.lock().unwrap_or_else(|p| p.into_inner()) = None;
}
