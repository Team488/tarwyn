//! The client in plain Python types: a 2d pose is `(x, y, rotation)` with the
//! rotation in radians, a 3d pose `(x, y, z, qw, qx, qy, qz)`, a coordinate
//! `(x, y)`, and a bezier control point `(x, y, rotation_degrees | None)`.
//!
//! Every call that waits on the server releases the GIL while it does, and
//! subscription callbacks run on the client's receive threads with the GIL
//! taken for them.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

use tarwyn_client::ffi::Point;

/// What the server reports about itself.
#[pyclass(frozen, get_all, skip_from_py_object, module = "tarwyn")]
#[derive(Clone, Debug)]
struct ServerStatistics {
    channels: u64,
    values: u64,
    telemetry_subscribers: u64,
    uptime_seconds: u64,
    dropped_publishes: u64,
    dropped_logs: u64,
    version: String,
}

#[pymethods]
impl ServerStatistics {
    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

type Pose2d = (f64, f64, f64);
type Pose3d = (f64, f64, f64, f64, f64, f64, f64);
type ControlPoint = (f64, f64, Option<f64>);

fn to_bytes(py: Python<'_>, value: Vec<u8>) -> Py<PyBytes> {
    PyBytes::new(py, &value).unbind()
}

fn to_bytes_list(py: Python<'_>, values: Vec<Vec<u8>>) -> Vec<Py<PyBytes>> {
    values
        .into_iter()
        .map(|value| to_bytes(py, value))
        .collect()
}

fn points_from(points: Vec<ControlPoint>) -> Vec<Point> {
    points
        .into_iter()
        .map(|(x, y, rotation_degrees)| Point {
            x,
            y,
            rotation_degrees,
        })
        .collect()
}

fn points_into(points: Vec<Point>) -> Vec<ControlPoint> {
    points
        .into_iter()
        .map(|point| (point.x, point.y, point.rotation_degrees))
        .collect()
}

/// Runs a Python callable on the client's receive thread, reporting rather
/// than swallowing anything it raises.
fn call(
    callback: &Py<PyAny>,
    args: impl for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>>,
) {
    Python::attach(|py| {
        if let Err(error) = args(py).and_then(|args| callback.call1(py, args)) {
            error.write_unraisable(py, Some(callback.bind(py)));
        }
    });
}

/// One connection: publishes, reads, control and subscriptions.
#[pyclass(frozen, subclass, module = "tarwyn")]
#[derive(Debug)]
struct TarwynClient {
    inner: tarwyn_client::ffi::TarwynClient,
}

#[pymethods]
impl TarwynClient {
    /// With no arguments, a client for a server on this machine; with only
    /// `host`, a client for the server there; with every port, a client with
    /// each port and timeout spelled out.
    #[new]
    #[pyo3(signature = (
        host = None, push_port = None, req_port = None, sub_port = None,
        telemetry_port = None, request_timeout_ms = None, send_high_water_mark = None,
    ))]
    fn new(
        host: Option<&str>,
        push_port: Option<u16>,
        req_port: Option<u16>,
        sub_port: Option<u16>,
        telemetry_port: Option<u16>,
        request_timeout_ms: Option<u64>,
        send_high_water_mark: Option<i32>,
    ) -> PyResult<Self> {
        let ports = (
            push_port,
            req_port,
            sub_port,
            telemetry_port,
            request_timeout_ms,
            send_high_water_mark,
        );
        let inner = match (host, ports) {
            (None, (None, None, None, None, None, None)) => tarwyn_client::ffi::TarwynClient::new(),
            (Some(host), (None, None, None, None, None, None)) => {
                tarwyn_client::ffi::TarwynClient::connect(host)
            }
            (
                Some(host),
                (Some(push), Some(req), Some(sub), Some(telemetry), Some(timeout), Some(hwm)),
            ) => tarwyn_client::ffi::TarwynClient::with_ports(
                host, push, req, sub, telemetry, timeout, hwm,
            ),
            _ => {
                return Err(PyValueError::new_err(
                    "give a host alone, or a host with every port, timeout and high water mark",
                ));
            }
        };
        Ok(Self { inner })
    }

    fn start(&self) {
        self.inner.start();
    }

    fn stop(&self, py: Python<'_>) {
        py.detach(|| self.inner.stop());
    }

    fn put_string(&self, channel: &str, value: &str) {
        self.inner.put_string(channel, value);
    }

    fn put_integer(&self, channel: &str, value: i32) {
        self.inner.put_integer(channel, value);
    }

    fn put_long(&self, channel: &str, value: i64) {
        self.inner.put_long(channel, value);
    }

    fn put_double(&self, channel: &str, value: f64) {
        self.inner.put_double(channel, value);
    }

    fn put_float(&self, channel: &str, value: f32) {
        self.inner.put_float(channel, value);
    }

    fn put_boolean(&self, channel: &str, value: bool) {
        self.inner.put_boolean(channel, value);
    }

    fn put_bytes(&self, channel: &str, value: &[u8]) {
        self.inner.put_bytes(channel, value);
    }

    fn put_string_list(&self, channel: &str, value: Vec<String>) {
        self.inner.put_string_list(channel, &value);
    }

    fn put_bytes_list(&self, channel: &str, value: Vec<Vec<u8>>) {
        self.inner.put_bytes_list(channel, &value);
    }

    fn put_double_list(&self, channel: &str, value: Vec<f64>) {
        self.inner.put_double_list(channel, &value);
    }

    fn put_float_list(&self, channel: &str, value: Vec<f32>) {
        self.inner.put_float_list(channel, &value);
    }

    fn put_integer_list(&self, channel: &str, value: Vec<i32>) {
        self.inner.put_integer_list(channel, &value);
    }

    fn put_long_list(&self, channel: &str, value: Vec<i64>) {
        self.inner.put_long_list(channel, &value);
    }

    fn put_boolean_list(&self, channel: &str, value: Vec<bool>) {
        self.inner.put_boolean_list(channel, &value);
    }

    fn put_coordinates(&self, channel: &str, value: Vec<(f64, f64)>) {
        self.inner.put_coordinates(channel, &value);
    }

    /// `rotation` is in radians.
    fn put_pose2d(&self, channel: &str, x: f64, y: f64, rotation: f64) {
        self.inner.put_pose2d(channel, x, y, rotation);
    }

    /// The rotation is a quaternion, `w` first.
    #[allow(clippy::too_many_arguments)]
    fn put_pose3d(
        &self,
        channel: &str,
        x: f64,
        y: f64,
        z: f64,
        qw: f64,
        qx: f64,
        qy: f64,
        qz: f64,
    ) {
        self.inner.put_pose3d(channel, [x, y, z, qw, qx, qy, qz]);
    }

    fn put_bezier_curve(&self, channel: &str, value: Vec<ControlPoint>) {
        self.inner.put_bezier_curve(channel, &points_from(value));
    }

    /// `value` is an encoded protobuf `BezierCurves`; False when it is not.
    fn put_bezier_curves(&self, channel: &str, value: &[u8]) -> bool {
        self.inner.put_bezier_curves(channel, value)
    }

    /// `value` is an encoded protobuf `BezierCurvesList`; False when it is not.
    fn put_bezier_curves_list(&self, channel: &str, value: &[u8]) -> bool {
        self.inner.put_bezier_curves_list(channel, value)
    }

    fn put_typed_bytes(&self, channel: &str, tarwyn_type: i32, value: &[u8]) -> bool {
        self.inner.put_typed_bytes(channel, tarwyn_type, value)
    }

    fn put_unknown_bytes(&self, channel: &str, value: &[u8]) {
        self.inner.put_unknown_bytes(channel, value);
    }

    /// Publishes `packed` as a struct topic of `type_name`, announcing
    /// each `(name, schema)` in `schemas` once.
    fn put_struct(
        &self,
        channel: &str,
        type_name: &str,
        schemas: Vec<(String, String)>,
        packed: &[u8],
    ) {
        let pairs: Vec<(&str, &str)> = schemas
            .iter()
            .map(|(name, schema)| (name.as_str(), schema.as_str()))
            .collect();
        self.inner.put_struct(channel, type_name, &pairs, packed);
    }

    fn get_string(&self, py: Python<'_>, channel: &str) -> Option<String> {
        py.detach(|| self.inner.get_string(channel))
    }

    fn get_integer(&self, py: Python<'_>, channel: &str) -> Option<i32> {
        py.detach(|| self.inner.get_integer(channel))
    }

    fn get_long(&self, py: Python<'_>, channel: &str) -> Option<i64> {
        py.detach(|| self.inner.get_long(channel))
    }

    fn get_double(&self, py: Python<'_>, channel: &str) -> Option<f64> {
        py.detach(|| self.inner.get_double(channel))
    }

    fn get_float(&self, py: Python<'_>, channel: &str) -> Option<f32> {
        py.detach(|| self.inner.get_float(channel))
    }

    fn get_boolean(&self, py: Python<'_>, channel: &str) -> Option<bool> {
        py.detach(|| self.inner.get_boolean(channel))
    }

    fn get_bytes(&self, py: Python<'_>, channel: &str) -> Option<Py<PyBytes>> {
        py.detach(|| self.inner.get_bytes(channel))
            .map(|value| to_bytes(py, value))
    }

    fn get_string_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<String>> {
        py.detach(|| self.inner.get_string_list(channel))
    }

    fn get_bytes_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<Py<PyBytes>>> {
        py.detach(|| self.inner.get_bytes_list(channel))
            .map(|values| to_bytes_list(py, values))
    }

    fn get_double_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<f64>> {
        py.detach(|| self.inner.get_double_list(channel))
    }

    fn get_float_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<f32>> {
        py.detach(|| self.inner.get_float_list(channel))
    }

    fn get_integer_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<i32>> {
        py.detach(|| self.inner.get_integer_list(channel))
    }

    fn get_long_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<i64>> {
        py.detach(|| self.inner.get_long_list(channel))
    }

    fn get_boolean_list(&self, py: Python<'_>, channel: &str) -> Option<Vec<bool>> {
        py.detach(|| self.inner.get_boolean_list(channel))
    }

    fn get_coordinates(&self, py: Python<'_>, channel: &str) -> Option<Vec<(f64, f64)>> {
        py.detach(|| self.inner.get_coordinates(channel))
    }

    fn get_bezier_curve(&self, py: Python<'_>, channel: &str) -> Option<Vec<ControlPoint>> {
        py.detach(|| self.inner.get_bezier_curve(channel))
            .map(points_into)
    }

    /// The encoded protobuf `BezierCurves` on `channel`.
    fn get_bezier_curves(&self, py: Python<'_>, channel: &str) -> Option<Py<PyBytes>> {
        py.detach(|| self.inner.get_bezier_curves(channel))
            .map(|value| to_bytes(py, value))
    }

    /// The encoded protobuf `BezierCurvesList` on `channel`.
    fn get_bezier_curves_list(&self, py: Python<'_>, channel: &str) -> Option<Py<PyBytes>> {
        py.detach(|| self.inner.get_bezier_curves_list(channel))
            .map(|value| to_bytes(py, value))
    }

    /// `(x, y, rotation)`, rotation in radians.
    fn get_pose2d(&self, py: Python<'_>, channel: &str) -> Option<Pose2d> {
        py.detach(|| self.inner.get_pose2d(channel))
            .map(|[x, y, rotation]| (x, y, rotation))
    }

    /// `(x, y, z, qw, qx, qy, qz)`.
    fn get_pose3d(&self, py: Python<'_>, channel: &str) -> Option<Pose3d> {
        py.detach(|| self.inner.get_pose3d(channel))
            .map(|[x, y, z, qw, qx, qy, qz]| (x, y, z, qw, qx, qy, qz))
    }

    fn get_unknown_bytes(&self, py: Python<'_>, channel: &str) -> Option<Py<PyBytes>> {
        py.detach(|| self.inner.get_unknown_bytes(channel))
            .map(|value| to_bytes(py, value))
    }

    /// How many channels were removed: 0 or 1.
    fn delete(&self, py: Python<'_>, channel: &str) -> u32 {
        py.detach(|| self.inner.delete(channel))
    }

    fn delete_all(&self, py: Python<'_>) -> u32 {
        py.detach(|| self.inner.delete_all())
    }

    fn get_tables(&self, py: Python<'_>, prefix: &str) -> Vec<String> {
        py.detach(|| self.inner.get_tables(prefix))
    }

    /// The round trip to the server in nanoseconds.
    fn get_ping(&self, py: Python<'_>) -> Option<u64> {
        py.detach(|| self.inner.get_ping())
    }

    fn get_server_statistics(&self, py: Python<'_>) -> Option<ServerStatistics> {
        let statistics = py.detach(|| self.inner.get_server_statistics())?;
        Some(ServerStatistics {
            channels: statistics.channels,
            values: statistics.values,
            telemetry_subscribers: statistics.telemetry_subscribers,
            uptime_seconds: statistics.uptime_seconds,
            dropped_publishes: statistics.dropped_publishes,
            dropped_logs: statistics.dropped_logs,
            version: statistics.version,
        })
    }

    /// The JSON of everything under `prefix`; `{}` when the server is absent.
    fn get_raw_json(&self, py: Python<'_>, prefix: &str) -> String {
        py.detach(|| self.inner.get_raw_json(prefix))
    }

    fn compare_and_set_absent_string(&self, py: Python<'_>, channel: &str, value: &str) -> bool {
        py.detach(|| self.inner.compare_and_set_absent_string(channel, value))
    }

    fn compare_and_set_string(
        &self,
        py: Python<'_>,
        channel: &str,
        expected: &str,
        value: &str,
    ) -> bool {
        py.detach(|| self.inner.compare_and_set_string(channel, expected, value))
    }

    fn compare_and_set_double(
        &self,
        py: Python<'_>,
        channel: &str,
        expected: f64,
        value: f64,
    ) -> bool {
        py.detach(|| self.inner.compare_and_set_double(channel, expected, value))
    }

    fn compare_and_set_long(
        &self,
        py: Python<'_>,
        channel: &str,
        expected: i64,
        value: i64,
    ) -> bool {
        py.detach(|| self.inner.compare_and_set_long(channel, expected, value))
    }

    fn compare_and_set_boolean(
        &self,
        py: Python<'_>,
        channel: &str,
        expected: bool,
        value: bool,
    ) -> bool {
        py.detach(|| self.inner.compare_and_set_boolean(channel, expected, value))
    }

    fn publish_telemetry(&self, channel: &str, payload: &[u8]) {
        self.inner.publish_telemetry(channel, payload);
    }

    fn log_to(&self, py: Python<'_>, path: &str) -> bool {
        py.detach(|| self.inner.log_to(path))
    }

    /// The path the log landed at, or None when it could not be opened.
    fn log_to_drive(&self, py: Python<'_>, filename: &str) -> Option<String> {
        py.detach(|| self.inner.log_to_drive(filename))
    }

    fn dropped_log_records(&self) -> u64 {
        self.inner.dropped_log_records()
    }

    fn logging_healthy(&self) -> bool {
        self.inner.logging_healthy()
    }

    fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }

    /// Calls `callback(channel, value)` for every value on `channel`, `value`
    /// being the protobuf `SupportedValues` encoding as bytes. False when the
    /// channel already has a subscription.
    fn subscribe(&self, channel: &str, callback: Py<PyAny>) -> bool {
        self.inner.subscribe(channel, move |channel, value| {
            call(&callback, |py| {
                (channel, PyBytes::new(py, value)).into_pyobject(py)
            });
        })
    }

    fn unsubscribe(&self, channel: &str) -> bool {
        self.inner.unsubscribe(channel)
    }

    /// Calls `callback(timestamp_micros, payload)` for every telemetry sample
    /// on `channel`. False when the channel already has a subscription or the
    /// telemetry plane refused it.
    fn subscribe_telemetry(&self, channel: &str, callback: Py<PyAny>) -> bool {
        self.inner
            .subscribe_telemetry(channel, move |timestamp, payload| {
                call(&callback, |py| {
                    (timestamp, PyBytes::new(py, payload)).into_pyobject(py)
                });
            })
    }

    fn unsubscribe_telemetry(&self, channel: &str) -> bool {
        self.inner.unsubscribe_telemetry(channel)
    }

    /// Calls `callback("logs", line)` for every log line the server emits.
    fn subscribe_to_logs(&self, callback: Py<PyAny>) -> bool {
        self.inner.subscribe_to_logs(move |channel, line| {
            call(&callback, |py| {
                (channel, PyBytes::new(py, line)).into_pyobject(py)
            });
        })
    }

    fn unsubscribe_from_logs(&self) -> bool {
        self.inner.unsubscribe_from_logs()
    }
}

#[pymodule]
fn _tarwyn(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TarwynClient>()?;
    m.add_class::<ServerStatistics>()?;
    m.add("LOGS_CHANNEL", tarwyn_client::ffi::LOGS_CHANNEL)?;
    Ok(())
}
