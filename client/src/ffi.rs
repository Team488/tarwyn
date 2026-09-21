//! The client in plain types, with subscriptions kept by name so one can be
//! cancelled without holding onto a closure. A 2d pose is `x, y, rotation`
//! with the rotation in radians; a 3d pose is `x, y, z, qw, qx, qy, qz`.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use crate::{Client, Config, Value};
use prost::Message;
use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BezierCurvesList, BytesList, ControlPoint, SupportedValues,
};

/// A control point of a bezier curve. `rotation_degrees` is `None` when the
/// point carries no heading.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    /// Metres.
    pub x: f64,
    /// Metres.
    pub y: f64,
    /// The heading at this point, if it has one.
    pub rotation_degrees: Option<f64>,
}

/// What the server reports about itself.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct ServerStatistics {
    pub channels: u64,
    pub values: u64,
    pub telemetry_subscribers: u64,
    pub uptime_seconds: u64,
    pub dropped_publishes: u64,
    pub dropped_logs: u64,
    pub version: String,
}

/// The channel a log subscription reports its lines under.
pub const LOGS_CHANNEL: &str = "logs";

type Cancel = Box<dyn FnOnce() + Send>;

/// One connection: publishes, reads, control and subscriptions.
///
/// Every method is safe to call from any thread. Subscription callbacks run on
/// the client's receive threads, never on the caller's.
pub struct TarwynClient {
    inner: Client,
    cancels: Mutex<HashMap<String, Cancel>>,
}

impl fmt::Debug for TarwynClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarwynClient").finish_non_exhaustive()
    }
}

fn unpack_le_doubles<const N: usize>(value: Value) -> Option<[f64; N]> {
    let Value::Bytes(bytes) = value else {
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

fn curve_from(points: &[Point]) -> BezierCurve {
    BezierCurve {
        control_points: points
            .iter()
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

/// The protobuf `SupportedValues` encoding of a value, which is what every
/// value subscription hands its callback.
pub fn encode_value(value: &Value) -> Vec<u8> {
    SupportedValues {
        kind: Some(Kind::from(value.clone())),
    }
    .encode_to_vec()
}

#[allow(missing_docs)]
impl TarwynClient {
    fn wrap(inner: Client) -> Self {
        Self {
            inner,
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// A client for a server on this machine.
    pub fn new() -> Self {
        Self::wrap(Client::new())
    }

    /// A client for the server on `host`.
    pub fn connect(host: &str) -> Self {
        Self::wrap(Client::connect(host))
    }

    /// A client with every port, timeout and window spelled out.
    ///
    /// `busy_poll_micros` is how long the reader spins on its socket before
    /// blocking, and `predict_micros` how far around a predicted arrival it
    /// spins instead; see [`Config::busy_poll`] and [`Config::predict`].
    pub fn with_ports(
        host: &str,
        port: u16,
        telemetry_port: u16,
        request_timeout_ms: u64,
        send_high_water_mark: i32,
        busy_poll_micros: u64,
        predict_micros: u64,
    ) -> Self {
        Self::wrap(Client::with_config(Config {
            host: host.to_string(),
            port,
            telemetry_port,
            request_timeout: Duration::from_millis(request_timeout_ms),
            send_high_water_mark,
            busy_poll: Duration::from_micros(busy_poll_micros),
            predict: Duration::from_micros(predict_micros),
        }))
    }

    pub fn start(&self) {
        self.inner.start();
    }

    pub fn stop(&self) {
        self.inner.stop();
    }

    fn register(&self, key: String, cancel: Cancel) -> bool {
        let Ok(mut cancels) = self.cancels.lock() else {
            cancel();
            return false;
        };
        if cancels.contains_key(&key) {
            drop(cancels);
            cancel();
            return false;
        }
        cancels.insert(key, cancel);
        true
    }

    fn cancel(&self, key: &str) -> bool {
        let Ok(mut cancels) = self.cancels.lock() else {
            return false;
        };
        let Some(cancel) = cancels.remove(key) else {
            return false;
        };
        drop(cancels);
        cancel();
        true
    }

    pub fn put_string(&self, channel: &str, value: &str) {
        self.inner.send_string(channel, value);
    }

    pub fn put_integer(&self, channel: &str, value: i32) {
        self.inner.send_i32(channel, value);
    }

    pub fn put_long(&self, channel: &str, value: i64) {
        self.inner.send_i64(channel, value);
    }

    pub fn put_double(&self, channel: &str, value: f64) {
        self.inner.send_double(channel, value);
    }

    pub fn put_float(&self, channel: &str, value: f32) {
        self.inner.send_float(channel, value);
    }

    pub fn put_boolean(&self, channel: &str, value: bool) {
        self.inner.send_bool(channel, value);
    }

    pub fn put_bytes(&self, channel: &str, value: &[u8]) {
        self.inner.send_bytes(channel, value);
    }

    pub fn put_string_list(&self, channel: &str, value: &[String]) {
        self.inner.send_string_list(channel, value);
    }

    pub fn put_bytes_list(&self, channel: &str, value: &[Vec<u8>]) {
        self.inner.send_bytes_list(channel, value);
    }

    pub fn put_double_list(&self, channel: &str, value: &[f64]) {
        self.inner.send_double_list(channel, value);
    }

    pub fn put_float_list(&self, channel: &str, value: &[f32]) {
        self.inner.send_float_list(channel, value);
    }

    pub fn put_integer_list(&self, channel: &str, value: &[i32]) {
        self.inner.send_integer_list(channel, value);
    }

    pub fn put_long_list(&self, channel: &str, value: &[i64]) {
        self.inner.send_long_list(channel, value);
    }

    pub fn put_boolean_list(&self, channel: &str, value: &[bool]) {
        self.inner.send_bool_list(channel, value);
    }

    pub fn put_coordinates(&self, channel: &str, value: &[(f64, f64)]) {
        self.inner.send_coordinates(channel, value);
    }

    pub fn put_pose2d(&self, channel: &str, x: f64, y: f64, rotation: f64) {
        self.inner.send_pose2d_struct(channel, x, y, rotation);
    }

    pub fn put_pose3d(&self, channel: &str, fields: [f64; 7]) {
        let [x, y, z, qw, qx, qy, qz] = fields;
        self.inner
            .send_pose3d_struct(channel, x, y, z, qw, qx, qy, qz);
    }

    pub fn put_bezier_curve(&self, channel: &str, value: &[Point]) {
        self.inner.send_bezier_curve(channel, curve_from(value));
    }

    /// `value` is an encoded protobuf `BezierCurves`; false when it is not.
    pub fn put_bezier_curves(&self, channel: &str, value: &[u8]) -> bool {
        let Ok(curves) = BezierCurves::decode(value) else {
            return false;
        };
        self.inner.send_bezier_curves(channel, curves);
        true
    }

    /// `value` is an encoded protobuf `BezierCurvesList`; false when it is not.
    pub fn put_bezier_curves_list(&self, channel: &str, value: &[u8]) -> bool {
        let Ok(list) = BezierCurvesList::decode(value) else {
            return false;
        };
        self.inner.send_bezier_curves_list(channel, list.values);
        true
    }

    pub fn put_typed_bytes(&self, channel: &str, tarwyn_type: i32, value: &[u8]) -> bool {
        self.inner.send_typed_bytes(channel, tarwyn_type, value)
    }

    pub fn put_unknown_bytes(&self, channel: &str, value: &[u8]) {
        self.inner.send_unknown_bytes(channel, value);
    }

    /// Publish `packed` as a struct topic of `type_name`, announcing
    /// each `(name, schema)` once so dashboards can decode it.
    pub fn put_struct(
        &self,
        channel: &str,
        type_name: &str,
        schemas: &[(&str, &str)],
        packed: &[u8],
    ) {
        self.inner
            .send_struct(channel, type_name, schemas, packed.to_vec());
    }

    pub fn get_string(&self, channel: &str) -> Option<String> {
        match self.inner.get(channel)? {
            Value::String(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_integer(&self, channel: &str) -> Option<i32> {
        match self.inner.get(channel)? {
            Value::Int32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_long(&self, channel: &str) -> Option<i64> {
        match self.inner.get(channel)? {
            Value::Int64(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_double(&self, channel: &str) -> Option<f64> {
        match self.inner.get(channel)? {
            Value::Double(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_float(&self, channel: &str) -> Option<f32> {
        match self.inner.get(channel)? {
            Value::Float(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_boolean(&self, channel: &str) -> Option<bool> {
        match self.inner.get(channel)? {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_bytes(&self, channel: &str) -> Option<Vec<u8>> {
        match self.inner.get(channel)? {
            Value::Bytes(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_string_list(&self, channel: &str) -> Option<Vec<String>> {
        match self.inner.get(channel)? {
            Value::StringArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_bytes_list(&self, channel: &str) -> Option<Vec<Vec<u8>>> {
        match self.inner.get(channel)? {
            Value::BytesList(bytes) => BytesList::decode(bytes.as_slice()).ok().map(|l| l.values),
            _ => None,
        }
    }

    pub fn get_double_list(&self, channel: &str) -> Option<Vec<f64>> {
        match self.inner.get(channel)? {
            Value::DoubleArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_float_list(&self, channel: &str) -> Option<Vec<f32>> {
        match self.inner.get(channel)? {
            Value::FloatArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_integer_list(&self, channel: &str) -> Option<Vec<i32>> {
        match self.inner.get(channel)? {
            Value::Int32Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_long_list(&self, channel: &str) -> Option<Vec<i64>> {
        match self.inner.get(channel)? {
            Value::Int64Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_boolean_list(&self, channel: &str) -> Option<Vec<bool>> {
        match self.inner.get(channel)? {
            Value::BoolArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_coordinates(&self, channel: &str) -> Option<Vec<(f64, f64)>> {
        self.inner.get_coordinates(channel)
    }

    pub fn get_bezier_curve(&self, channel: &str) -> Option<Vec<Point>> {
        Some(curve_into(self.inner.get_bezier_curve(channel)?))
    }

    /// The encoded protobuf `BezierCurves` on `channel`.
    pub fn get_bezier_curves(&self, channel: &str) -> Option<Vec<u8>> {
        Some(self.inner.get_bezier_curves(channel)?.encode_to_vec())
    }

    /// The encoded protobuf `BezierCurvesList` on `channel`.
    pub fn get_bezier_curves_list(&self, channel: &str) -> Option<Vec<u8>> {
        let values = self.inner.get_bezier_curves_list(channel)?;
        Some(BezierCurvesList { values }.encode_to_vec())
    }

    /// `[x, y, rotation]`, rotation in radians.
    pub fn get_pose2d(&self, channel: &str) -> Option<[f64; 3]> {
        unpack_le_doubles(self.inner.get(channel)?)
    }

    /// `[x, y, z, qw, qx, qy, qz]`.
    pub fn get_pose3d(&self, channel: &str) -> Option<[f64; 7]> {
        unpack_le_doubles(self.inner.get(channel)?)
    }

    pub fn get_unknown_bytes(&self, channel: &str) -> Option<Vec<u8>> {
        self.inner.get_unknown_bytes(channel)
    }

    pub fn delete(&self, channel: &str) -> u32 {
        self.inner.delete(channel)
    }

    pub fn delete_all(&self) -> u32 {
        self.inner.delete_all()
    }

    pub fn get_tables(&self, prefix: &str) -> Vec<String> {
        self.inner.tables(prefix)
    }

    /// Round trip to the server in nanoseconds.
    pub fn get_ping(&self) -> Option<u64> {
        Some(self.inner.ping()?.as_nanos() as u64)
    }

    pub fn get_server_statistics(&self) -> Option<ServerStatistics> {
        let r = self.inner.statistics()?;
        Some(ServerStatistics {
            channels: r.channels,
            values: r.values,
            telemetry_subscribers: r.telemetry_subscribers,
            uptime_seconds: r.uptime_seconds,
            dropped_publishes: r.dropped_publishes,
            dropped_logs: r.dropped_logs,
            version: r.version,
        })
    }

    pub fn get_raw_json(&self, prefix: &str) -> String {
        self.inner.raw_json(prefix)
    }

    pub fn compare_and_set_absent_string(&self, channel: &str, value: &str) -> bool {
        self.inner
            .compare_and_set(channel, None, Value::String(value.to_string()))
    }

    pub fn compare_and_set_string(&self, channel: &str, expected: &str, value: &str) -> bool {
        self.inner.compare_and_set(
            channel,
            Some(Value::String(expected.to_string())),
            Value::String(value.to_string()),
        )
    }

    pub fn compare_and_set_double(&self, channel: &str, expected: f64, value: f64) -> bool {
        self.inner
            .compare_and_set(channel, Some(Value::Double(expected)), Value::Double(value))
    }

    pub fn compare_and_set_long(&self, channel: &str, expected: i64, value: i64) -> bool {
        self.inner
            .compare_and_set(channel, Some(Value::Int64(expected)), Value::Int64(value))
    }

    pub fn compare_and_set_boolean(&self, channel: &str, expected: bool, value: bool) -> bool {
        self.inner
            .compare_and_set(channel, Some(Value::Bool(expected)), Value::Bool(value))
    }

    pub fn publish_telemetry(&self, channel: &str, payload: &[u8]) {
        self.inner.publish_telemetry(channel, payload);
    }

    pub fn log_to(&self, path: &str) -> bool {
        self.inner.log_to(path).is_ok()
    }

    /// The path the log landed at.
    pub fn log_to_drive(&self, filename: &str) -> Option<String> {
        self.inner
            .log_to_drive(filename)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    pub fn dropped_log_records(&self) -> u64 {
        self.inner.log_dropped()
    }

    pub fn logging_healthy(&self) -> bool {
        self.inner.logging_healthy()
    }

    pub fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }

    /// Run `callback(channel, value)` for every value on `channel`, `value`
    /// being the protobuf `SupportedValues` encoding. False when the channel
    /// already has a subscription.
    pub fn subscribe<F>(&self, channel: &str, callback: F) -> bool
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        let key = format!("value:{channel}");
        let echo = channel.to_string();
        let cancel = self.inner.subscribe(channel, move |value| {
            callback(&echo, &encode_value(value));
        });
        self.register(key, Box::new(cancel))
    }

    pub fn unsubscribe(&self, channel: &str) -> bool {
        self.cancel(&format!("value:{channel}"))
    }

    /// Run `callback(timestamp_micros, payload)` for every telemetry sample on
    /// `channel`. False when the channel already has a subscription or the
    /// telemetry plane refused it.
    pub fn subscribe_telemetry<F>(&self, channel: &str, callback: F) -> bool
    where
        F: Fn(u64, &[u8]) + Send + Sync + 'static,
    {
        let key = format!("telemetry:{channel}");
        let Some(cancel) =
            self.inner
                .subscribe_telemetry_timestamped(channel, move |timestamp, payload| {
                    callback(timestamp, payload);
                })
        else {
            return false;
        };
        self.register(key, Box::new(cancel))
    }

    pub fn unsubscribe_telemetry(&self, channel: &str) -> bool {
        self.cancel(&format!("telemetry:{channel}"))
    }

    /// Run `callback(LOGS_CHANNEL, line)` for every log line the server emits.
    pub fn subscribe_to_logs<F>(&self, callback: F) -> bool
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        let cancel = self.inner.subscribe_to_logs(move |line| {
            callback(LOGS_CHANNEL, line.as_bytes());
        });
        self.register(LOGS_CHANNEL.into(), Box::new(cancel))
    }

    pub fn unsubscribe_from_logs(&self) -> bool {
        self.cancel(LOGS_CHANNEL)
    }
}

impl Default for TarwynClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offline() -> TarwynClient {
        TarwynClient::with_ports("127.0.0.1", 26783, 26784, 150, 500, 0, 0)
    }

    #[test]
    fn a_value_of_the_wrong_width_is_refused_rather_than_misread() {
        assert!(unpack_le_doubles::<3>(Value::Bytes(vec![0; 16])).is_none());
        assert!(unpack_le_doubles::<3>(Value::Bytes(vec![0; 24])).is_some());
        assert!(unpack_le_doubles::<3>(Value::String("nope".into())).is_none());
    }

    #[test]
    fn a_read_reports_absence_rather_than_inventing_a_value() {
        let client = offline();
        assert!(client.get_pose2d("absent").is_none());
        assert!(client.get_string("absent").is_none());
        assert!(client.get_ping().is_none());
        assert!(client.get_server_statistics().is_none());
        assert_eq!(client.get_raw_json(""), "{}");
        assert!(client.get_tables("").is_empty());
    }

    #[test]
    fn a_second_subscription_reports_the_first_until_it_is_cancelled() {
        let client = offline();
        assert!(client.subscribe("pose", |_, _| {}));
        assert!(!client.subscribe("pose", |_, _| {}));
        assert!(client.unsubscribe("pose"));
        assert!(!client.unsubscribe("pose"));
        assert!(client.subscribe("pose", |_, _| {}));

        assert!(client.subscribe_to_logs(|_, _| {}));
        assert!(!client.subscribe_to_logs(|_, _| {}));
        assert!(client.unsubscribe_from_logs());
        assert!(!client.unsubscribe_from_logs());
    }

    #[test]
    fn a_typed_put_rejects_bytes_that_are_not_that_type() {
        let client = offline();
        assert!(!client.put_typed_bytes("pose", 2, &[1, 2, 3]));
        assert!(client.put_typed_bytes("pose", 9999, &[1, 2, 3]));
    }
}
