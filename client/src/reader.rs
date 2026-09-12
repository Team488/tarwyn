//! The reader thread: what arrives on the connection, and where it goes.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{Receiver, Sender},
};

use prost::Message;
use tungstenite::{Message as WebsocketMessage, WebSocket};

use tarwyn_protobuf::protobuf::Reply;

use tarwyn_server::value::Value;
use tarwyn_server::websocket::message::{ControlMessage, ValueMessage};

use crate::client::LOG_TOPIC;
use crate::connection::{
    OUTBOUND_POLL, POLL_INTERVAL, ReadHalf, SharedWriter, connect_websocket, drain_outbound,
    drain_outbound_dropped, is_timeout, set_read_timeout, split_connection,
};
use crate::listeners::{
    LogListener, LogListenerMap, SessionState, SubscribeListener, SubscribeListenerMap, TopicNames,
};

/// Route a decoded value message to the right listeners by topic name.
pub(crate) fn fan_out_value(
    vm: ValueMessage,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_names: &TopicNames,
) {
    let name = {
        let names = topic_names.lock().unwrap_or_else(|p| p.into_inner());
        names.get(&vm.topic_id).cloned()
    };
    let Some(name) = name else {
        return;
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

/// Handle one binary frame: a value message, a control reply, or noise.
pub(crate) fn handle_binary(
    payload: Vec<u8>,
    data_listeners: &SubscribeListenerMap,
    log_listeners: &LogListenerMap,
    topic_names: &TopicNames,
    pending: &Arc<Mutex<Option<Sender<Vec<u8>>>>>,
) {
    if let Ok(vm) = ValueMessage::decode(&payload) {
        fan_out_value(vm, data_listeners, log_listeners, topic_names);
        return;
    }
    if Reply::decode(&payload[..]).is_ok()
        && let Some(tx) = pending.lock().ok().and_then(|mut p| p.take())
    {
        let _ = tx.send(payload);
    }
}

/// Handle one text frame: an NT4 announcement, which corrects the topic map.
pub(crate) fn handle_text(text: String, topic_names: &TopicNames) {
    if let Ok(ControlMessage::Announce { name, id, .. }) = ControlMessage::from_json(&text) {
        topic_names
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, name);
    }
}

/// Re-send the publishes and subscriptions that make up this client's session.
///
/// A frame may still be sitting in the outbound queue as well, so the server
/// can see a publish twice; re-publishing an existing publisher UID is a
/// re-announce in NT4, not a second publisher, so that is harmless.
///
/// Returns whether every frame went out; a failure here means the connection
/// died during the replay, and the caller reconnects.
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

/// The single connection owner: connects (retrying), drains outbound, and
/// demuxes inbound frames until told to stop.
#[expect(clippy::too_many_arguments)]
pub(crate) fn reader_loop(
    outbound: Receiver<Vec<u8>>,
    url: String,
    subprotocol: String,
    data_listeners: SubscribeListenerMap,
    log_listeners: LogListenerMap,
    topic_names: TopicNames,
    pending: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
    session: SessionState,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    reader_alive: Arc<AtomicBool>,
    writer: SharedWriter,
) {
    reader_alive.store(true, Ordering::SeqCst);

    'outer: loop {
        // The previous connection's writing half, if any, points at a dead
        // socket: anything written through it is lost, replay included.
        *writer.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let (mut websocket, spare) = match connect_websocket(&url, &subprotocol) {
            Ok(websocket) => split_connection(websocket, &writer),
            Err(_) => {
                drain_outbound_dropped(&outbound, &dropped);
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
        };
        set_read_timeout(&websocket, OUTBOUND_POLL);

        // Topic ids belong to the connection that announced them; a new server
        // reassigns them, so keeping the old ones routes values to the wrong
        // subscribers. Re-announcements refill this.
        topic_names
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        if !replay_session(&mut websocket, &session) {
            continue;
        }
        // Only now may publishes go straight out: everything the session owed
        // this connection has been written, and anything queued while it was
        // down is drained here, so nothing written inline can overtake it.
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
                        payload.to_vec(),
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
