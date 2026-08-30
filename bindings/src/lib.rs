//! The boundary every non-Rust client is generated from.
//!
//! BoltFFI reads the `#[data]` and `#[export]` items here and emits the Java and
//! Python clients, together with the native glue they call. Nothing in this crate
//! is hand-mirrored into another language.
//!
//! This is a wrapper rather than annotations on [`tarwyn_client`] itself.
//! BoltFFI's boundary forbids raw pointers, non-static lifetimes and generics, and
//! those constraints should not reach the Rust client's own API.

use boltffi::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tarwyn_client::tarwyn_client::{TarwynClient as Inner, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{BezierCurve, BezierCurves, BezierCurvesList, ControlPoint};

/// Server counters, as reported by [`TarwynClient::get_server_statistics`].
#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct ServerStatistics {
    pub channels: u64,
    pub values: u64,
    pub telemetry_subscribers: u64,
    pub uptime_seconds: u64,
    pub dropped_publishes: u64,
    pub dropped_logs: u64,
    pub version: String,
}

/// An `(x, y)` pair, as carried by the coordinate list type.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Coordinate {
    pub x: f64,
    pub y: f64,
}

/// One control point of a bezier curve. `rotation_degrees` is absent for a point
/// that does not constrain heading.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub rotation_degrees: Option<f64>,
}

/// A pose on the field plane.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pose2d {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

/// A pose in space, with its rotation as a quaternion.
///
/// The field order is WPILib's `Pose3d` struct layout - a `Translation3d`
/// followed by a `Rotation3d`, which is a `Quaternion` written `w` first - so a
/// value written here reads back through WPILib's own deserialiser.
///
/// Rotation is a quaternion rather than roll, pitch and yaw because converting
/// between the two means committing to a rotation order, and getting that wrong
/// is silent. `Rotation3d` converts in both directions: construct one from
/// `roll`, `pitch`, `yaw` and read `getQuaternion()`, or take `getX()`, `getY()`
/// and `getZ()` back out.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pose3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub qw: f64,
    pub qx: f64,
    pub qy: f64,
    pub qz: f64,
}

/// A value published to a channel, delivered to a subscriber.
///
/// The payload is the encoded value; `channel` names what it arrived on, so one
/// subscription can carry several channels.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub channel: String,
    pub value: Vec<u8>,
}

/// A telemetry datagram, with the publisher's clock.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct Telemetry {
    pub timestamp_micros: u64,
    pub payload: Vec<u8>,
}

fn pack_le_doubles(fields: &[f64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(fields.len() * 8);
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes
}

fn unpack_le_doubles<const N: usize>(value: Kind) -> Option<[f64; N]> {
    let Kind::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != N * 8 {
        return None;
    }
    let mut fields = [0.0; N];
    for (index, field) in fields.iter_mut().enumerate() {
        let chunk: [u8; 8] = bytes[index * 8..index * 8 + 8].try_into().ok()?;
        *field = f64::from_le_bytes(chunk);
    }
    Some(fields)
}

fn curve_from(points: Vec<Point>) -> BezierCurve {
    BezierCurve {
        control_points: points
            .into_iter()
            .map(|point| ControlPoint {
                x: point.x,
                y: point.y,
                rotation_degrees: point.rotation_degrees,
            })
            .collect(),
    }
}

fn curve_into(curve: BezierCurve) -> Vec<Point> {
    curve
        .control_points
        .into_iter()
        .map(|point| Point {
            x: point.x,
            y: point.y,
            rotation_degrees: point.rotation_degrees,
        })
        .collect()
}

/// A connection to an TARWYN server.
///
/// Constructing it never blocks: ZeroMQ dials in the background, so a client may
/// be built before the server exists. Nothing is received until [`Self::start`].
pub struct TarwynClient {
    inner: Inner,
    updates: Arc<StreamProducer<Update>>,
    telemetry: Arc<StreamProducer<Telemetry>>,
    logs: Arc<StreamProducer<String>>,
    cancel_handles: Mutex<HashMap<String, Box<dyn FnOnce() + Send>>>,
}

const STREAM_DEPTH_BEFORE_DROPPING_OLDEST: usize = 256;

#[export]
impl TarwynClient {
    /// Connect to a server on localhost with the default ports.
    pub fn new() -> Self {
        Self::from_inner(Inner::new())
    }

    /// Connect to a server on another machine - a coprocessor, or the robot controller.
    pub fn connect(host: String) -> Self {
        Self::from_inner(Inner::connect(&host))
    }

    /// Connect with every port and the request timeout spelled out.
    pub fn with_ports(
        host: String,
        push_port: u16,
        req_port: u16,
        sub_port: u16,
        telemetry_port: u16,
        request_timeout_ms: u64,
        send_high_water_mark: i32,
    ) -> Self {
        Self::from_inner(Inner::with_config(TarwynConfig {
            host,
            push_port,
            req_port,
            sub_port,
            telemetry_port,
            request_timeout: std::time::Duration::from_millis(request_timeout_ms),
            send_high_water_mark,
        }))
    }

    /// Start the receive threads, so subscriptions begin delivering.
    ///
    /// Publishing and reading work without this.
    pub fn start(&self) {
        self.inner.start();
    }

    /// Stop the receive threads. Subscriptions survive and resume on the next start.
    pub fn stop(&self) {
        self.inner.stop();
    }

    /// Publish a string.
    pub fn put_string(&self, channel: String, value: String) {
        self.inner.send_string(&channel, &value);
    }

    /// Publish a 32-bit signed integer.
    pub fn put_integer(&self, channel: String, value: i32) {
        self.inner.send_i32(&channel, value);
    }

    /// Publish a 64-bit signed integer.
    pub fn put_long(&self, channel: String, value: i64) {
        self.inner.send_i64(&channel, value);
    }

    /// Publish a double.
    pub fn put_double(&self, channel: String, value: f64) {
        self.inner.send_double(&channel, value);
    }

    /// Publish a float.
    pub fn put_float(&self, channel: String, value: f32) {
        self.inner.send_float(&channel, value);
    }

    /// Publish a boolean.
    pub fn put_boolean(&self, channel: String, value: bool) {
        self.inner.send_bool(&channel, value);
    }

    /// Publish raw bytes.
    pub fn put_bytes(&self, channel: String, value: Vec<u8>) {
        self.inner.send_bytes(&channel, &value);
    }

    /// Publish a list of strings.
    pub fn put_string_list(&self, channel: String, value: Vec<String>) {
        self.inner.send_string_list(&channel, &value);
    }

    /// Publish a list of byte strings.
    pub fn put_bytes_list(&self, channel: String, value: Vec<Vec<u8>>) {
        self.inner.send_bytes_list(&channel, &value);
    }

    /// Publish a list of doubles.
    pub fn put_double_list(&self, channel: String, value: Vec<f64>) {
        self.inner.send_double_list(&channel, &value);
    }

    /// Publish a list of floats.
    pub fn put_float_list(&self, channel: String, value: Vec<f32>) {
        self.inner.send_float_list(&channel, &value);
    }

    /// Publish a list of 32-bit integers.
    pub fn put_integer_list(&self, channel: String, value: Vec<i32>) {
        self.inner.send_integer_list(&channel, &value);
    }

    /// Publish a list of 64-bit integers.
    pub fn put_long_list(&self, channel: String, value: Vec<i64>) {
        self.inner.send_long_list(&channel, &value);
    }

    /// Publish a list of booleans.
    pub fn put_boolean_list(&self, channel: String, value: Vec<bool>) {
        self.inner.send_bool_list(&channel, &value);
    }

    /// Publish a list of `(x, y)` coordinates.
    pub fn put_coordinates(&self, channel: String, value: Vec<Coordinate>) {
        let pairs: Vec<(f64, f64)> = value.iter().map(|point| (point.x, point.y)).collect();
        self.inner.send_coordinates(&channel, &pairs);
    }

    /// Publish a pose on the field plane.
    pub fn put_pose2d(&self, channel: String, value: Pose2d) {
        self.inner.send_bytes(
            &channel,
            &pack_le_doubles(&[value.x, value.y, value.rotation]),
        );
    }

    /// Publish a pose in space.
    pub fn put_pose3d(&self, channel: String, value: Pose3d) {
        self.inner.send_bytes(
            &channel,
            &pack_le_doubles(&[
                value.x, value.y, value.z, value.qw, value.qx, value.qy, value.qz,
            ]),
        );
    }

    /// Publish one bezier curve.
    pub fn put_bezier_curve(&self, channel: String, value: Vec<Point>) {
        self.inner.send_bezier_curve(&channel, curve_from(value));
    }

    /// Publish a bezier path already encoded as protobuf, byte-identical to TARWYN'.
    pub fn put_bezier_curves(&self, channel: String, value: Vec<u8>) -> bool {
        match <BezierCurves as prost_decode::Decode>::decode(&value) {
            Some(curves) => {
                self.inner.send_bezier_curves(&channel, curves);
                true
            }
            None => false,
        }
    }

    /// Publish several bezier paths, encoded as protobuf.
    pub fn put_bezier_curves_list(&self, channel: String, value: Vec<u8>) -> bool {
        match <BezierCurvesList as prost_decode::Decode>::decode(&value) {
            Some(list) => {
                self.inner.send_bezier_curves_list(&channel, list.values);
                true
            }
            None => false,
        }
    }

    /// Publish bytes whose type the caller does not know.
    pub fn put_unknown_bytes(&self, channel: String, value: Vec<u8>) {
        self.inner.send_unknown_bytes(&channel, &value);
    }

    /// Publish a value already encoded in TARWYN' byte layout, given its type tag.
    ///
    /// Returns false, publishing nothing, when a recognised tag comes with bytes
    /// that are not a valid value of that type.
    pub fn put_typed_bytes(&self, channel: String, tarwyn_type: i32, value: Vec<u8>) -> bool {
        self.inner.send_typed_bytes(&channel, tarwyn_type, &value)
    }

    /// Read a string. Absent if the channel holds nothing, or another type.
    pub fn get_string(&self, channel: String) -> Option<String> {
        match self.inner.get(&channel)? {
            Kind::String(value) => Some(value),
            _ => None,
        }
    }

    /// Read a 32-bit signed integer.
    pub fn get_integer(&self, channel: String) -> Option<i32> {
        match self.inner.get(&channel)? {
            Kind::Int32(value) => Some(value),
            _ => None,
        }
    }

    /// Read a 64-bit signed integer.
    pub fn get_long(&self, channel: String) -> Option<i64> {
        match self.inner.get(&channel)? {
            Kind::Int64(value) => Some(value),
            _ => None,
        }
    }

    /// Read a double.
    pub fn get_double(&self, channel: String) -> Option<f64> {
        match self.inner.get(&channel)? {
            Kind::Double(value) => Some(value),
            _ => None,
        }
    }

    /// Read a float.
    pub fn get_float(&self, channel: String) -> Option<f32> {
        match self.inner.get(&channel)? {
            Kind::Float(value) => Some(value),
            _ => None,
        }
    }

    /// Read a boolean.
    pub fn get_boolean(&self, channel: String) -> Option<bool> {
        match self.inner.get(&channel)? {
            Kind::Bool(value) => Some(value),
            _ => None,
        }
    }

    /// Read raw bytes.
    pub fn get_bytes(&self, channel: String) -> Option<Vec<u8>> {
        match self.inner.get(&channel)? {
            Kind::Bytes(value) => Some(value),
            _ => None,
        }
    }

    /// Read a list of strings.
    pub fn get_string_list(&self, channel: String) -> Option<Vec<String>> {
        match self.inner.get(&channel)? {
            Kind::StringList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of byte strings.
    pub fn get_bytes_list(&self, channel: String) -> Option<Vec<Vec<u8>>> {
        match self.inner.get(&channel)? {
            Kind::BytesList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of doubles.
    pub fn get_double_list(&self, channel: String) -> Option<Vec<f64>> {
        match self.inner.get(&channel)? {
            Kind::DoubleList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of floats.
    pub fn get_float_list(&self, channel: String) -> Option<Vec<f32>> {
        match self.inner.get(&channel)? {
            Kind::FloatList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of 32-bit integers.
    pub fn get_integer_list(&self, channel: String) -> Option<Vec<i32>> {
        match self.inner.get(&channel)? {
            Kind::IntegerList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of 64-bit integers.
    pub fn get_long_list(&self, channel: String) -> Option<Vec<i64>> {
        match self.inner.get(&channel)? {
            Kind::LongList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a list of booleans.
    pub fn get_boolean_list(&self, channel: String) -> Option<Vec<bool>> {
        match self.inner.get(&channel)? {
            Kind::BoolList(list) => Some(list.values),
            _ => None,
        }
    }

    /// Read a coordinate list.
    pub fn get_coordinates(&self, channel: String) -> Option<Vec<Coordinate>> {
        Some(
            self.inner
                .get_coordinates(&channel)?
                .into_iter()
                .map(|(x, y)| Coordinate { x, y })
                .collect(),
        )
    }

    /// Read a pose on the field plane.
    pub fn get_pose2d(&self, channel: String) -> Option<Pose2d> {
        let fields = unpack_le_doubles::<3>(self.inner.get(&channel)?)?;
        Some(Pose2d {
            x: fields[0],
            y: fields[1],
            rotation: fields[2],
        })
    }

    /// Read a pose in space.
    pub fn get_pose3d(&self, channel: String) -> Option<Pose3d> {
        let fields = unpack_le_doubles::<7>(self.inner.get(&channel)?)?;
        Some(Pose3d {
            x: fields[0],
            y: fields[1],
            z: fields[2],
            qw: fields[3],
            qx: fields[4],
            qy: fields[5],
            qz: fields[6],
        })
    }

    /// Read one bezier curve as its control points.
    pub fn get_bezier_curve(&self, channel: String) -> Option<Vec<Point>> {
        Some(curve_into(self.inner.get_bezier_curve(&channel)?))
    }

    /// Read a bezier path as encoded protobuf, byte-identical to TARWYN'.
    pub fn get_bezier_curves(&self, channel: String) -> Option<Vec<u8>> {
        Some(prost_decode::encode(
            &self.inner.get_bezier_curves(&channel)?,
        ))
    }

    /// Read a list of bezier paths as encoded protobuf.
    pub fn get_bezier_curves_list(&self, channel: String) -> Option<Vec<u8>> {
        let values = self.inner.get_bezier_curves_list(&channel)?;
        Some(prost_decode::encode(&BezierCurvesList { values }))
    }

    /// Read a channel holding raw bytes whose type the caller does not know.
    pub fn get_unknown_bytes(&self, channel: String) -> Option<Vec<u8>> {
        self.inner.get_unknown_bytes(&channel)
    }

    /// Delete a channel. Returns how many were removed, 0 or 1.
    pub fn delete(&self, channel: String) -> u32 {
        self.inner.delete(&channel)
    }

    /// Delete every channel. Returns how many were removed.
    pub fn delete_all(&self) -> u32 {
        self.inner.delete_all()
    }

    /// List the channel names beginning with `prefix`. Pass "" for all of them.
    pub fn get_tables(&self, prefix: String) -> Vec<String> {
        self.inner.tables(&prefix)
    }

    /// Round-trip time to the server in nanoseconds, absent if it does not answer.
    pub fn get_ping(&self) -> Option<u64> {
        Some(self.inner.ping()?.as_nanos() as u64)
    }

    /// Server counters. Absent if the server does not answer.
    pub fn get_server_statistics(&self) -> Option<ServerStatistics> {
        let reply = self.inner.statistics()?;
        Some(ServerStatistics {
            channels: reply.channels,
            values: reply.values,
            telemetry_subscribers: reply.telemetry_subscribers,
            uptime_seconds: reply.uptime_seconds,
            dropped_publishes: reply.dropped_publishes,
            dropped_logs: reply.dropped_logs,
            version: reply.version,
        })
    }

    /// The channels beginning with `prefix`, as a JSON document.
    pub fn get_raw_json(&self, prefix: String) -> String {
        self.inner.raw_json(&prefix)
    }

    /// Set a channel to `value` only while it is empty, and report whether it swapped.
    pub fn compare_and_set_absent_string(&self, channel: String, value: String) -> bool {
        self.inner
            .compare_and_set(&channel, None, Kind::String(value))
    }

    /// Set a channel to `value` only if it currently holds `expected`.
    pub fn compare_and_set_string(&self, channel: String, expected: String, value: String) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::String(expected)), Kind::String(value))
    }

    /// Set a channel to `value` only if it currently holds `expected`.
    pub fn compare_and_set_double(&self, channel: String, expected: f64, value: f64) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Double(expected)), Kind::Double(value))
    }

    /// Set a channel to `value` only if it currently holds `expected`.
    pub fn compare_and_set_long(&self, channel: String, expected: i64, value: i64) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Int64(expected)), Kind::Int64(value))
    }

    /// Set a channel to `value` only if it currently holds `expected`.
    pub fn compare_and_set_boolean(&self, channel: String, expected: bool, value: bool) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Bool(expected)), Kind::Bool(value))
    }

    /// Publish on the UDP telemetry plane, which trades delivery guarantees for latency.
    pub fn publish_telemetry(&self, channel: String, payload: Vec<u8>) {
        self.inner.publish_telemetry(&channel, &payload);
    }

    /// Mirror every published value into a WPILOG file.
    pub fn log_to(&self, path: String) -> bool {
        self.inner.log_to(path).is_ok()
    }

    /// As `log_to`, onto the first writable removable mount. Returns the path chosen.
    pub fn log_to_drive(&self, filename: String) -> Option<String> {
        self.inner
            .log_to_drive(&filename)
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    }

    /// How many log records were dropped because the writer queue was full.
    pub fn dropped_log_records(&self) -> u64 {
        self.inner.log_dropped()
    }

    /// Whether the log writer is still succeeding.
    pub fn logging_healthy(&self) -> bool {
        self.inner.logging_healthy()
    }

    /// How many publishes were dropped rather than queued, across both transports.
    pub fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }

    /// Deliver every value published to `channel`.
    ///
    /// Values arrive as soon as they are published: the consumer is woken rather
    /// than polling, so delivery is not paced by an interval.
    pub fn subscribe(&self, channel: String) -> bool {
        let producer = Arc::clone(&self.updates);
        let name = channel.clone();
        self.subscribe_once(format!("data:{channel}"), || {
            self.inner.subscribe(&channel, move |value| {
                producer.push(Update {
                    channel: name.clone(),
                    value: encoded(value),
                });
            })
        })
    }

    /// Stop delivering values from `channel`. False if it was not subscribed.
    pub fn unsubscribe(&self, channel: String) -> bool {
        self.cancel(&format!("data:{channel}"))
    }

    /// The stream every [`Self::subscribe`] call feeds.
    #[ffi_stream(item = Update)]
    pub fn updates(&self) -> Arc<EventSubscription<Update>> {
        self.updates.subscribe()
    }

    /// Receive telemetry on `channel`. Absent if another channel already claimed
    /// this one's topic hash - a collision is refused rather than cross-wired.
    pub fn subscribe_telemetry(&self, channel: String) -> bool {
        let producer = Arc::clone(&self.telemetry);
        let key = format!("telemetry:{channel}");
        let Ok(mut cancel_handles) = self.cancel_handles.lock() else {
            return false;
        };
        if cancel_handles.contains_key(&key) {
            return false;
        }
        let handle = self.inner.subscribe_telemetry_timestamped(
            &channel,
            move |timestamp_micros, payload| {
                producer.push(Telemetry {
                    timestamp_micros,
                    payload: payload.to_vec(),
                });
            },
        );
        match handle {
            Some(cancel) => {
                cancel_handles.insert(key, Box::new(cancel));
                true
            }
            None => false,
        }
    }

    /// Stop delivering telemetry from `channel`. False if it was not subscribed.
    pub fn unsubscribe_telemetry(&self, channel: String) -> bool {
        self.cancel(&format!("telemetry:{channel}"))
    }

    /// The stream every [`Self::subscribe_telemetry`] call feeds.
    #[ffi_stream(item = Telemetry)]
    pub fn telemetry(&self) -> Arc<EventSubscription<Telemetry>> {
        self.telemetry.subscribe()
    }

    /// Deliver every log line the server emits.
    pub fn subscribe_to_logs(&self) -> bool {
        let producer = Arc::clone(&self.logs);
        self.subscribe_once(String::from("logs"), || {
            self.inner.subscribe_to_logs(move |line| {
                producer.push(line.clone());
            })
        })
    }

    /// Stop delivering log lines. False if they were not subscribed.
    pub fn unsubscribe_from_logs(&self) -> bool {
        self.cancel("logs")
    }

    /// The stream [`Self::subscribe_to_logs`] feeds.
    #[ffi_stream(item = String)]
    pub fn logs(&self) -> Arc<EventSubscription<String>> {
        self.logs.subscribe()
    }
}

impl TarwynClient {
    fn subscribe_once<F, C>(&self, key: String, subscribe: F) -> bool
    where
        F: FnOnce() -> C,
        C: FnOnce() + Send + 'static,
    {
        let Ok(mut cancel_handles) = self.cancel_handles.lock() else {
            return false;
        };
        if cancel_handles.contains_key(&key) {
            return false;
        }
        cancel_handles.insert(key, Box::new(subscribe()));
        true
    }

    fn cancel(&self, key: &str) -> bool {
        let Ok(mut cancel_handles) = self.cancel_handles.lock() else {
            return false;
        };
        match cancel_handles.remove(key) {
            Some(cancel) => {
                drop(cancel_handles);
                cancel();
                true
            }
            None => false,
        }
    }

    fn from_inner(inner: Inner) -> Self {
        Self {
            inner,
            updates: Arc::new(StreamProducer::new(STREAM_DEPTH_BEFORE_DROPPING_OLDEST)),
            telemetry: Arc::new(StreamProducer::new(STREAM_DEPTH_BEFORE_DROPPING_OLDEST)),
            logs: Arc::new(StreamProducer::new(STREAM_DEPTH_BEFORE_DROPPING_OLDEST)),
            cancel_handles: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for TarwynClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Encoding helpers kept out of the exported surface.
mod prost_decode {
    use prost::Message;

    pub trait Decode: Sized {
        fn decode(bytes: &[u8]) -> Option<Self>;
    }

    impl<T: Message + Default> Decode for T {
        fn decode(bytes: &[u8]) -> Option<Self> {
            T::decode(bytes).ok()
        }
    }

    pub fn encode<T: Message>(value: &T) -> Vec<u8> {
        value.encode_to_vec()
    }
}

/// The wire bytes of a published value, so a subscriber receives what was sent
/// rather than a type this boundary had to choose for it.
fn encoded(value: &Kind) -> Vec<u8> {
    use prost::Message;
    tarwyn_protobuf::protobuf::SupportedValues {
        kind: Some(value.clone()),
    }
    .encode_to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pose_is_packed_rather_than_written_as_a_list() {
        let bytes = pack_le_doubles(&[1.5, -2.0, 0.25]);

        assert_eq!(bytes.len(), 3 * 8, "a Pose2d is three doubles, not a list");
        assert_eq!(&bytes[0..8], &1.5f64.to_le_bytes());
        assert_eq!(&bytes[8..16], &(-2.0f64).to_le_bytes());
        assert_eq!(&bytes[16..24], &0.25f64.to_le_bytes());
    }

    #[test]
    fn a_packed_pose_reads_back_field_for_field() {
        let fields = [1.5, -2.0, 0.25, 100.0, f64::MIN, f64::MAX, 0.0];
        let bytes = pack_le_doubles(&fields);

        assert_eq!(unpack_le_doubles::<7>(Kind::Bytes(bytes)), Some(fields));
    }

    #[test]
    fn a_pose3d_puts_w_before_x_y_and_z() {
        let pose = Pose3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            qw: 0.5,
            qx: 0.5,
            qy: 0.5,
            qz: 0.5,
        };
        let bytes = pack_le_doubles(&[pose.x, pose.y, pose.z, pose.qw, pose.qx, pose.qy, pose.qz]);

        assert_eq!(bytes.len(), 7 * 8);
        assert_eq!(&bytes[24..32], &0.5f64.to_le_bytes(), "w precedes x, y, z");
    }

    #[test]
    fn a_value_of_the_wrong_width_is_refused_rather_than_misread() {
        assert_eq!(
            unpack_le_doubles::<3>(Kind::Bytes(pack_le_doubles(&[1.0, 2.0]))),
            None
        );
        assert_eq!(
            unpack_le_doubles::<3>(Kind::Bytes(pack_le_doubles(&[1.0, 2.0, 3.0, 4.0]))),
            None
        );
        assert_eq!(
            unpack_le_doubles::<3>(Kind::DoubleList(tarwyn_protobuf::protobuf::DoubleList {
                values: vec![1.0, 2.0, 3.0],
            })),
            None,
            "a double list is not the pose encoding and must not be read as one"
        );
    }
}
