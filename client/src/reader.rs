//! The reader thread: what arrives on the connection, and where it goes.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{Receiver, Sender},
};

use prost::Message;
use tungstenite::{Bytes, Message as WebsocketMessage, WebSocket};

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
    payload: Bytes,
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
        let _ = tx.send(payload.to_vec());
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
///
/// With a duplicate of the socket to publish through, nothing reaches the
/// queue while the connection is up, so the read only has to wake for the
/// stop flag; without one, over TLS, it wakes every [`OUTBOUND_POLL`] to
/// drain what the publishers queued.
///
/// Each new connection starts clean. The previous connection's writing half
/// points at a dead socket, so it is cleared before anything is written.
/// Topic ids belong to the connection that announced them and a new server
/// reassigns them, so the map is emptied and refilled by re-announcements.
/// Publishes go straight out only once the session has been replayed and the
/// queue drained, so nothing written inline can overtake what the connection
/// was owed.
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
