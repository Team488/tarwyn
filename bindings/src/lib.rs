use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tarwyn_client::client::{TarwynClient as Inner, TarwynConfig};
use tarwyn_types::{
    Coordinate, Point, Pose2d, Pose3d, ServerStatistics, StructSchema, Telemetry, Update,
};

#[uniffi::export(callback_interface)]
pub trait Updater: Send + Sync {
    fn update(&self, update: Update);
}

#[uniffi::export(callback_interface)]
pub trait TelemetryUpdater: Send + Sync {
    fn update(&self, telemetry: Telemetry);
}
use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{BezierCurve, BezierCurves, BezierCurvesList, ControlPoint};

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

const LOGS_KEY: &str = "logs";

type Cancel = Box<dyn FnOnce() + Send>;

#[derive(uniffi::Object)]
pub struct TarwynClient {
    inner: Inner,
    cancels: Mutex<HashMap<String, Cancel>>,
}

impl TarwynClient {
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
}

fn encoded(value: &Kind) -> Vec<u8> {
    use prost::Message;
    tarwyn_protobuf::protobuf::SupportedValues {
        kind: Some(value.clone()),
    }
    .encode_to_vec()
}

#[uniffi::export]
impl TarwynClient {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Inner::new(),
            cancels: Mutex::new(HashMap::new()),
        })
    }

    #[uniffi::constructor]
    pub fn connect(host: String) -> Arc<Self> {
        Arc::new(Self {
            inner: Inner::connect(&host),
            cancels: Mutex::new(HashMap::new()),
        })
    }

    #[uniffi::constructor]
    pub fn with_ports(
        host: String,
        push_port: u16,
        req_port: u16,
        sub_port: u16,
        telemetry_port: u16,
        request_timeout_ms: u64,
        send_high_water_mark: i32,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Inner::with_config(TarwynConfig {
                host,
                push_port,
                req_port,
                sub_port,
                telemetry_port,
                request_timeout: std::time::Duration::from_millis(request_timeout_ms),
                send_high_water_mark,
            }),
            cancels: Mutex::new(HashMap::new()),
        })
    }

    pub fn start(&self) {
        self.inner.start();
    }

    pub fn stop(&self) {
        self.inner.stop();
    }

    pub fn put_string(&self, channel: String, value: String) {
        self.inner.send_string(&channel, &value);
    }

    pub fn put_integer(&self, channel: String, value: i32) {
        self.inner.send_i32(&channel, value);
    }

    pub fn put_long(&self, channel: String, value: i64) {
        self.inner.send_i64(&channel, value);
    }

    pub fn put_double(&self, channel: String, value: f64) {
        self.inner.send_double(&channel, value);
    }

    pub fn put_float(&self, channel: String, value: f32) {
        self.inner.send_float(&channel, value);
    }

    pub fn put_boolean(&self, channel: String, value: bool) {
        self.inner.send_bool(&channel, value);
    }

    pub fn put_bytes(&self, channel: String, value: Vec<u8>) {
        self.inner.send_bytes(&channel, &value);
    }

    pub fn put_string_list(&self, channel: String, value: Vec<String>) {
        self.inner.send_string_list(&channel, &value);
    }

    pub fn put_bytes_list(&self, channel: String, value: Vec<Vec<u8>>) {
        self.inner.send_bytes_list(&channel, &value);
    }

    pub fn put_double_list(&self, channel: String, value: Vec<f64>) {
        self.inner.send_double_list(&channel, &value);
    }

    pub fn put_float_list(&self, channel: String, value: Vec<f32>) {
        self.inner.send_float_list(&channel, &value);
    }

    pub fn put_integer_list(&self, channel: String, value: Vec<i32>) {
        self.inner.send_integer_list(&channel, &value);
    }

    pub fn put_long_list(&self, channel: String, value: Vec<i64>) {
        self.inner.send_long_list(&channel, &value);
    }

    pub fn put_boolean_list(&self, channel: String, value: Vec<bool>) {
        self.inner.send_bool_list(&channel, &value);
    }

    pub fn put_coordinates(&self, channel: String, value: Vec<Coordinate>) {
        let pairs: Vec<(f64, f64)> = value.iter().map(|p| (p.x, p.y)).collect();
        self.inner.send_coordinates(&channel, &pairs);
    }

    pub fn put_pose2d(&self, channel: String, value: Pose2d) {
        self.inner
            .send_pose2d_struct(&channel, value.x, value.y, value.rotation);
    }

    pub fn put_pose3d(&self, channel: String, value: Pose3d) {
        self.inner.send_pose3d_struct(
            &channel, value.x, value.y, value.z, value.qw, value.qx, value.qy, value.qz,
        );
    }

    pub fn put_bezier_curve(&self, channel: String, value: Vec<Point>) {
        self.inner.send_bezier_curve(&channel, curve_from(value));
    }

    pub fn put_bezier_curves(&self, channel: String, value: Vec<u8>) -> bool {
        use prost::Message;
        let Ok(curves) = BezierCurves::decode(value.as_slice()) else {
            return false;
        };
        self.inner.send_bezier_curves(&channel, curves);
        true
    }

    pub fn put_bezier_curves_list(&self, channel: String, value: Vec<u8>) -> bool {
        use prost::Message;
        let Ok(list) = BezierCurvesList::decode(value.as_slice()) else {
            return false;
        };
        self.inner.send_bezier_curves_list(&channel, list.values);
        true
    }

    pub fn put_typed_bytes(&self, channel: String, tarwyn_type: i32, value: Vec<u8>) -> bool {
        self.inner.send_typed_bytes(&channel, tarwyn_type, &value)
    }

    pub fn put_unknown_bytes(&self, channel: String, value: Vec<u8>) {
        self.inner.send_unknown_bytes(&channel, &value);
    }

    pub fn put_struct(
        &self,
        channel: String,
        type_name: String,
        schemas: Vec<StructSchema>,
        packed: Vec<u8>,
    ) {
        let _ = schemas
            .iter()
            .map(|s| (s.type_name.as_str(), s.schema.as_str()))
            .collect::<Vec<_>>();
        let _ = type_name;
        self.inner.send_unknown_bytes(&channel, &packed);
    }

    pub fn get_string(&self, channel: String) -> Option<String> {
        match self.inner.get(&channel)? {
            Kind::String(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_integer(&self, channel: String) -> Option<i32> {
        match self.inner.get(&channel)? {
            Kind::Int32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_long(&self, channel: String) -> Option<i64> {
        match self.inner.get(&channel)? {
            Kind::Int64(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_double(&self, channel: String) -> Option<f64> {
        match self.inner.get(&channel)? {
            Kind::Double(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_float(&self, channel: String) -> Option<f32> {
        match self.inner.get(&channel)? {
            Kind::Float(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_boolean(&self, channel: String) -> Option<bool> {
        match self.inner.get(&channel)? {
            Kind::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_bytes(&self, channel: String) -> Option<Vec<u8>> {
        match self.inner.get(&channel)? {
            Kind::Bytes(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_string_list(&self, channel: String) -> Option<Vec<String>> {
        match self.inner.get(&channel)? {
            Kind::StringList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_bytes_list(&self, channel: String) -> Option<Vec<Vec<u8>>> {
        match self.inner.get(&channel)? {
            Kind::BytesList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_double_list(&self, channel: String) -> Option<Vec<f64>> {
        match self.inner.get(&channel)? {
            Kind::DoubleList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_float_list(&self, channel: String) -> Option<Vec<f32>> {
        match self.inner.get(&channel)? {
            Kind::FloatList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_integer_list(&self, channel: String) -> Option<Vec<i32>> {
        match self.inner.get(&channel)? {
            Kind::IntegerList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_long_list(&self, channel: String) -> Option<Vec<i64>> {
        match self.inner.get(&channel)? {
            Kind::LongList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_boolean_list(&self, channel: String) -> Option<Vec<bool>> {
        match self.inner.get(&channel)? {
            Kind::BoolList(l) => Some(l.values),
            _ => None,
        }
    }

    pub fn get_coordinates(&self, channel: String) -> Option<Vec<Coordinate>> {
        Some(
            self.inner
                .get_coordinates(&channel)?
                .into_iter()
                .map(|(x, y)| Coordinate { x, y })
                .collect(),
        )
    }

    pub fn get_bezier_curve(&self, channel: String) -> Option<Vec<Point>> {
        Some(curve_into(self.inner.get_bezier_curve(&channel)?))
    }

    pub fn get_bezier_curves(&self, channel: String) -> Option<Vec<u8>> {
        use prost::Message;
        Some(self.inner.get_bezier_curves(&channel)?.encode_to_vec())
    }

    pub fn get_bezier_curves_list(&self, channel: String) -> Option<Vec<u8>> {
        use prost::Message;
        let v = self.inner.get_bezier_curves_list(&channel)?;
        Some(BezierCurvesList { values: v }.encode_to_vec())
    }

    pub fn get_pose2d(&self, channel: String) -> Option<Pose2d> {
        let f = unpack_le_doubles::<3>(self.inner.get(&channel)?)?;
        Some(Pose2d {
            x: f[0],
            y: f[1],
            rotation: f[2],
        })
    }

    pub fn get_pose3d(&self, channel: String) -> Option<Pose3d> {
        let f = unpack_le_doubles::<7>(self.inner.get(&channel)?)?;
        Some(Pose3d {
            x: f[0],
            y: f[1],
            z: f[2],
            qw: f[3],
            qx: f[4],
            qy: f[5],
            qz: f[6],
        })
    }

    pub fn get_unknown_bytes(&self, channel: String) -> Option<Vec<u8>> {
        self.inner.get_unknown_bytes(&channel)
    }

    pub fn delete(&self, channel: String) -> u32 {
        self.inner.delete(&channel)
    }

    pub fn delete_all(&self) -> u32 {
        self.inner.delete_all()
    }

    pub fn get_tables(&self, prefix: String) -> Vec<String> {
        self.inner.tables(&prefix)
    }

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

    pub fn get_raw_json(&self, prefix: String) -> String {
        self.inner.raw_json(&prefix)
    }

    pub fn compare_and_set_absent_string(&self, channel: String, value: String) -> bool {
        self.inner
            .compare_and_set(&channel, None, Kind::String(value))
    }
    pub fn compare_and_set_string(&self, channel: String, expected: String, value: String) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::String(expected)), Kind::String(value))
    }
    pub fn compare_and_set_double(&self, channel: String, expected: f64, value: f64) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Double(expected)), Kind::Double(value))
    }
    pub fn compare_and_set_long(&self, channel: String, expected: i64, value: i64) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Int64(expected)), Kind::Int64(value))
    }
    pub fn compare_and_set_boolean(&self, channel: String, expected: bool, value: bool) -> bool {
        self.inner
            .compare_and_set(&channel, Some(Kind::Bool(expected)), Kind::Bool(value))
    }

    pub fn publish_telemetry(&self, channel: String, payload: Vec<u8>) {
        self.inner.publish_telemetry(&channel, &payload);
    }
    pub fn log_to_drive(&self, filename: String) -> Option<String> {
        self.inner
            .log_to_drive(&filename)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }
    pub fn dropped_log_records(&self) -> u64 {
        self.inner.log_dropped()
    }
    pub fn logging_healthy(&self) -> bool {
        self.inner.logging_healthy()
    }

    pub fn subscribe(&self, channel: String, callback: Box<dyn Updater>) -> bool {
        let key = format!("value:{channel}");
        let echo = channel.clone();
        let cancel = self.inner.subscribe(&channel, move |kind| {
            callback.update(Update {
                channel: echo.clone(),
                value: encoded(kind),
            });
        });
        self.register(key, Box::new(cancel))
    }

    pub fn unsubscribe(&self, channel: String) -> bool {
        self.cancel(&format!("value:{channel}"))
    }

    pub fn subscribe_telemetry(
        &self,
        channel: String,
        callback: Box<dyn TelemetryUpdater>,
    ) -> bool {
        let key = format!("telemetry:{channel}");
        let Some(cancel) =
            self.inner
                .subscribe_telemetry_timestamped(&channel, move |timestamp, payload| {
                    callback.update(Telemetry {
                        timestamp_micros: timestamp,
                        payload: payload.to_vec(),
                    });
                })
        else {
            return false;
        };
        self.register(key, Box::new(cancel))
    }

    pub fn unsubscribe_telemetry(&self, channel: String) -> bool {
        self.cancel(&format!("telemetry:{channel}"))
    }

    pub fn subscribe_to_logs(&self, callback: Box<dyn Updater>) -> bool {
        let cancel = self.inner.subscribe_to_logs(move |line| {
            callback.update(Update {
                channel: LOGS_KEY.into(),
                value: line.clone().into_bytes(),
            });
        });
        self.register(LOGS_KEY.into(), Box::new(cancel))
    }

    pub fn unsubscribe_from_logs(&self) -> bool {
        self.cancel(LOGS_KEY)
    }

    pub fn log_to(&self, path: String) -> bool {
        self.inner.log_to(path).is_ok()
    }

    pub fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }
}

uniffi::setup_scaffolding!("tarwyn");

#[cfg(test)]
mod unpack_tests {
    use super::{unpack_le_doubles, Kind};

    #[test]
    fn a_value_of_the_wrong_width_is_refused_rather_than_misread() {
        assert!(unpack_le_doubles::<3>(Kind::Bytes(vec![0; 16])).is_none());
        assert!(unpack_le_doubles::<3>(Kind::Bytes(vec![0; 24])).is_some());
    }

    #[test]
    fn a_non_byte_value_is_refused() {
        assert!(unpack_le_doubles::<3>(Kind::String("nope".into())).is_none());
    }
}
