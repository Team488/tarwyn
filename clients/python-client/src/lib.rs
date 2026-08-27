//! Python bindings for the TARWYN client, built with PyO3.
//!
//! Compiles to a `tarwyn` extension module whose method names match the Rust
//! client's. The generated half of the surface comes from `clients/api.toml`; the
//! rest — subscriptions, logging and the control plane — is here.
//!
//! ```python
//! import tarwyn
//!
//! client = tarwyn.TarwynClient()
//! client.start()
//! client.send_double("pose", 1.5)
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod generated;

use pyo3::exceptions::PyOSError;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use tarwyn_client::tarwyn_client::{TarwynClient, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values::Kind;

/// A bounded buffer of the values a subscription has seen.
///
/// Handed out by `TarwynClient.subscribe_buffered`. Oldest values are evicted
/// once it is full. Also usable as a context manager, which unsubscribes on exit.
#[pyclass(name = "Subscription")]
struct PySubscription {
    values: Arc<Mutex<Vec<Kind>>>,
    unsubscribe: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[pymethods]
impl PySubscription {
    /// Take every value buffered since the last drain, leaving the buffer empty.
    fn drain(&self, python: Python<'_>) -> Vec<Py<PyAny>> {
        let drained = match self.values.lock() {
            Ok(mut values) => values.drain(..).collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        drained
            .into_iter()
            .map(|kind| kind_to_python(python, kind))
            .collect()
    }

    /// How many values are buffered.
    fn __len__(&self) -> usize {
        self.values.lock().map(|v| v.len()).unwrap_or(0)
    }

    /// Unsubscribe. Further drains return nothing.
    fn close(&self) {
        let taken = self
            .unsubscribe
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(unsubscribe) = taken {
            unsubscribe();
        }
    }

    /// Enter a `with` block; returns the subscription itself.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Leave a `with` block, unsubscribing on the way out.
    fn __exit__(
        slf: PyRef<'_, Self>,
        _type: Option<Py<PyAny>>,
        _value: Option<Py<PyAny>>,
        _traceback: Option<Py<PyAny>>,
    ) -> bool {
        slf.close();
        false
    }
}

fn curve_to_python(
    python: Python<'_>,
    curve: &tarwyn_protobuf::protobuf::BezierCurve,
) -> Py<PyAny> {
    let points: Vec<Py<PyAny>> = curve
        .control_points
        .iter()
        .map(|point| {
            let entry = pyo3::types::PyDict::new(python);
            let _ = entry.set_item("x", point.x);
            let _ = entry.set_item("y", point.y);
            let _ = entry.set_item("rotationDegrees", point.rotation_degrees);
            entry.into_any().unbind()
        })
        .collect();
    points.into_pyobject(python).unwrap().into_any().unbind()
}

fn curves_to_python(
    python: Python<'_>,
    curves: &tarwyn_protobuf::protobuf::BezierCurves,
) -> Py<PyAny> {
    let entry = pyo3::types::PyDict::new(python);
    let list: Vec<Py<PyAny>> = curves
        .curves
        .iter()
        .map(|curve| curve_to_python(python, curve))
        .collect();
    let _ = entry.set_item("curves", list);
    let _ = entry.set_item(
        "options",
        curves.options.as_ref().map(|options| {
            let fields = pyo3::types::PyDict::new(python);
            let _ = fields.set_item("metersPerSecond", options.meters_per_second);
            let _ = fields.set_item("finalRotationDegrees", options.final_rotation_degrees);
            let _ = fields.set_item(
                "accelerationMetersPerSecond",
                options.acceleration_meters_per_second,
            );
            let _ = fields.set_item(
                "faceNearestReefAprilTag",
                options.face_nearest_reef_april_tag,
            );
            let _ = fields.set_item(
                "endFaceNearestReefAprilTagPathThresholdPercentage",
                options.end_face_nearest_reef_april_tag_path_threshold_percentage,
            );
            let _ = fields.set_item(
                "faceNearestReefAprilTagDirection",
                options.face_nearest_reef_april_tag_direction,
            );
            let _ = fields.set_item(
                "finalRotationTurnSpeedFactor",
                options.final_rotation_turn_speed_factor,
            );
            let _ = fields.set_item(
                "startFaceNearestReefAprilTagPathThresholdPercentage",
                options.start_face_nearest_reef_april_tag_path_threshold_percentage,
            );
            let _ = fields.set_item("snapToNearestAprilTag", options.snap_to_nearest_april_tag);
            let _ = fields.set_item(
                "aprilTagRotationDegreesTurnSpeedFactorPerStep",
                options.april_tag_rotation_degrees_turn_speed_factor_per_step,
            );
            fields.into_any().unbind()
        }),
    );
    entry.into_any().unbind()
}

fn kind_to_python(python: Python<'_>, kind: Kind) -> Py<PyAny> {
    match kind {
        Kind::String(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Int32(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Int64(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Uint32(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Uint64(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Bool(value) => value
            .into_pyobject(python)
            .unwrap()
            .to_owned()
            .into_any()
            .unbind(),
        Kind::Double(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Float(value) => value.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::Bytes(value) => PyBytes::new(python, &value).into_any().unbind(),
        Kind::StringList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::FloatList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::BoolList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::BytesList(list) => list
            .values
            .into_iter()
            .map(|value| PyBytes::new(python, &value).unbind())
            .collect::<Vec<_>>()
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::DoubleList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::IntegerList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::LongList(list) => list
            .values
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::CoordinateList(list) => list
            .coordinates
            .into_iter()
            .map(|coordinate| (coordinate.x, coordinate.y))
            .collect::<Vec<_>>()
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
        Kind::BezierCurve(curve) => curve_to_python(python, &curve),
        Kind::BezierCurves(curves) => curves_to_python(python, &curves),
        Kind::BezierCurvesList(list) => list
            .values
            .iter()
            .map(|curves| curves_to_python(python, curves))
            .collect::<Vec<_>>()
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
    }
}

type CallbackKey = (String, usize);
type Unsubscribe = Box<dyn FnOnce() + Send>;
type CallbackRegistry = Arc<Mutex<HashMap<CallbackKey, Unsubscribe>>>;

/// A connection to an TARWYN server.
///
/// Method names match the original TARWYN client, so `put`/`get` call sites port
/// across unchanged. Constructing it never blocks - ZeroMQ dials in the
/// background - and nothing is received until `start()`.
///
/// ```python
/// import tarwyn
///
/// client = tarwyn.TarwynClient()
/// client.start()
/// client.put_double("pose", 1.5)
/// ```
#[pyclass(name = "TarwynClient")]
struct PyTarwynClient {
    inner: Arc<TarwynClient>,
    callbacks: CallbackRegistry,
}

#[pymethods]
impl PyTarwynClient {
    #[new]
    #[pyo3(signature = (host="127.0.0.1", push_port=5557, req_port=5556, sub_port=5555,
                        request_timeout_ms=500, send_high_water_mark=500))]
    /// Connect to a server.
    ///
    /// Never blocks: ZeroMQ dials in the background, so this succeeds before the
    /// server exists. Nothing is received until `start()`.
    fn new(
        host: &str,
        push_port: u16,
        req_port: u16,
        sub_port: u16,
        request_timeout_ms: u64,
        send_high_water_mark: i32,
    ) -> Self {
        PyTarwynClient {
            inner: Arc::new(TarwynClient::with_config(TarwynConfig {
                host: host.to_string(),
                push_port,
                req_port,
                sub_port,
                request_timeout: Duration::from_millis(request_timeout_ms),
                send_high_water_mark,
                telemetry_port: tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT,
            })),
            callbacks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[pyo3(name = "subscribe_callback")]
    /// Call `callback` with every value published to `channel`.
    ///
    /// Values arrive only once `start()` has been called. The callback runs on the
    /// receive thread, so it should return quickly.
    fn subscribe_callback(&self, channel: &str, callback: Py<PyAny>) -> PyResult<()> {
        let key: CallbackKey = (channel.to_string(), callback.as_ptr() as usize);
        let handle = self.inner.subscribe(channel, move |value| {
            Python::attach(|python| {
                let argument = kind_to_python(python, value.clone());
                if let Err(error) = callback.call1(python, (argument,)) {
                    error.print(python);
                }
            });
        });
        match self.callbacks.lock() {
            Ok(mut callbacks) => {
                callbacks.insert(key, Box::new(handle));
                Ok(())
            }
            Err(_) => Err(PyRuntimeError::new_err(
                "subscription registry was poisoned",
            )),
        }
    }

    #[pyo3(name = "unsubscribe")]
    /// Remove a callback previously registered for `channel`. Returns whether one was
    /// found.
    fn unsubscribe(&self, channel: &str, callback: Py<PyAny>) -> PyResult<bool> {
        let key: CallbackKey = (channel.to_string(), callback.as_ptr() as usize);
        let taken = self
            .callbacks
            .lock()
            .map_err(|_| PyRuntimeError::new_err("subscription registry was poisoned"))?
            .remove(&key);
        match taken {
            Some(unsubscribe) => {
                unsubscribe();
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Start the receive threads, so subscriptions begin delivering.
    fn start(&self) {
        self.inner.start();
    }

    /// Stop the receive threads. Subscriptions resume on the next `start()`.
    fn stop(&self) {
        self.inner.stop();
    }

    /// How many publishes were dropped rather than queued, across both transports.
    fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }

    /// Mirror every published value into a WPILOG file, which AdvantageScope, Elastic
    /// and the WPILib DataLogTool open directly.
    ///
    /// Records cross a bounded queue and are flushed every 250 ms, so a publish never
    /// waits on the filesystem. Raises if logging has already started.
    fn log_to(&self, python: Python<'_>, path: &str) -> PyResult<()> {
        python
            .detach(|| self.inner.log_to(path))
            .map_err(|error| PyOSError::new_err(error.to_string()))
    }

    /// As `log_to`, but onto the first writable removable drive that accepts the file.
    /// Returns the path chosen.
    fn log_to_drive(&self, python: Python<'_>, filename: &str) -> PyResult<String> {
        python
            .detach(|| self.inner.log_to_drive(filename))
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| PyOSError::new_err(error.to_string()))
    }

    /// How many log records were dropped because the writer queue was full.
    fn log_dropped(&self) -> u64 {
        self.inner.log_dropped()
    }

    /// Whether the log writer is still succeeding. An I/O error latches it off rather
    /// than raising into a publish, so this is the only way to notice.
    fn logging_healthy(&self) -> bool {
        self.inner.logging_healthy()
    }

    /// Delete `channel`. Returns how many were removed - 0 or 1.
    fn delete(&self, python: Python<'_>, channel: &str) -> u32 {
        python.detach(|| self.inner.delete(channel))
    }

    /// Delete every channel. Returns how many were removed.
    fn delete_all(&self, python: Python<'_>) -> u32 {
        python.detach(|| self.inner.delete_all())
    }

    #[pyo3(signature = (prefix=""))]
    /// List the channel names beginning with `prefix`. Pass `""` for all of them.
    fn get_tables(&self, python: Python<'_>, prefix: &str) -> Vec<String> {
        python.detach(|| self.inner.tables(prefix))
    }

    /// Round-trip time to the server in seconds, or `None` if it does not answer.
    fn get_ping(&self, python: Python<'_>) -> Option<f64> {
        python.detach(|| self.inner.ping().map(|elapsed| elapsed.as_secs_f64()))
    }

    /// Server counters as `(channels, messages, subscribers, uptime_nanos, version)`,
    /// or `None` if the server does not answer.
    fn get_server_statistics(&self, python: Python<'_>) -> Option<(u64, u64, u64, u64, String)> {
        python.detach(|| self.inner.statistics()).map(|statistics| {
            (
                statistics.channels,
                statistics.values,
                statistics.telemetry_subscribers,
                statistics.uptime_seconds,
                statistics.version,
            )
        })
    }

    #[pyo3(signature = (prefix=""))]
    /// The channels beginning with `prefix`, as a JSON string.
    fn get_raw_json(&self, python: Python<'_>, prefix: &str) -> String {
        python.detach(|| self.inner.raw_json(prefix))
    }

    /// Publish a list of `(x, y)` coordinates to `channel`.
    fn put_coordinates(&self, python: Python<'_>, channel: &str, values: Vec<(f64, f64)>) {
        python.detach(|| self.inner.send_coordinates(channel, &values));
    }

    /// Read the coordinate list on `channel`, or `None` if it is unset or holds
    /// another type.
    fn get_coordinates(&self, python: Python<'_>, channel: &str) -> Option<Vec<(f64, f64)>> {
        python.detach(|| self.inner.get_coordinates(channel))
    }

    /// Publish bytes whose type is not known. Matches TARWYN' `putUnknownBytes`.
    fn put_unknown_bytes(&self, python: Python<'_>, channel: &str, value: &[u8]) {
        python.detach(|| self.inner.send_unknown_bytes(channel, value));
    }

    /// Read raw bytes from `channel`, or `None` if it is unset or holds another type.
    fn get_unknown_bytes(&self, python: Python<'_>, channel: &str) -> Option<Vec<u8>> {
        python.detach(|| self.inner.get_unknown_bytes(channel))
    }

    /// Publish a value already encoded in TARWYN' own byte layout.
    ///
    /// `tarwyn_type` is TARWYN' type tag. Returns `False`, publishing nothing, if
    /// the tag is unknown or the bytes do not decode as that type.
    fn put_typed_bytes(
        &self,
        python: Python<'_>,
        channel: &str,
        tarwyn_type: i32,
        value: &[u8],
    ) -> bool {
        python.detach(|| self.inner.send_typed_bytes(channel, tarwyn_type, value))
    }

    /// Publish raw bytes to `channel`.
    fn put_bytes(&self, python: Python<'_>, channel: &str, value: &[u8]) {
        python.detach(|| self.inner.send_bytes(channel, value));
    }

    /// Read whatever `channel` holds, converted to the matching Python type.
    ///
    /// `None` if the channel is unset or the server does not answer within the
    /// request timeout.
    fn get(&self, python: Python<'_>, channel: &str) -> Option<Py<PyAny>> {
        let value = python.detach(|| self.inner.get(channel))?;
        Some(kind_to_python(python, value))
    }

    #[pyo3(name = "subscribe_buffered", signature = (channel, depth=64))]
    /// Subscribe into a bounded buffer instead of a callback, for code that polls.
    ///
    /// Returns a `Subscription`, which is also a context manager.
    fn subscribe_buffered(&self, channel: &str, depth: usize) -> PyResult<PySubscription> {
        if depth == 0 {
            return Err(PyRuntimeError::new_err("depth must be greater than zero"));
        }
        let values = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&values);
        let unsubscribe = self.inner.subscribe(channel, move |value| {
            if let Ok(mut buffered) = sink.lock() {
                if buffered.len() == depth {
                    buffered.remove(0);
                }
                buffered.push(value.clone());
            }
        });
        Ok(PySubscription {
            values,
            unsubscribe: Mutex::new(Some(Box::new(unsubscribe))),
        })
    }
}

#[pymodule]
fn tarwyn(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyTarwynClient>()?;
    module.add_class::<PySubscription>()?;
    install_tarwyn_compat_aliases(module)?;
    Ok(())
}

fn install_tarwyn_compat_aliases(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let client = module.getattr("TarwynClient")?;
    for (camel, snake) in [
        ("putString", "put_string"),
        ("putInteger", "put_integer"),
        ("putDouble", "put_double"),
        ("putBoolean", "put_boolean"),
        ("putBytes", "put_bytes"),
        ("putStringList", "put_string_list"),
        ("putFloatList", "put_float_list"),
        ("putBytesList", "put_bytes_list"),
        ("putBooleanList", "put_boolean_list"),
        ("getDouble", "get_double"),
        ("getStringList", "get_string_list"),
        ("droppedPublishes", "dropped_publishes"),
        ("subscribe", "subscribe_callback"),
    ] {
        let target = client.getattr(snake)?;
        client.setattr(camel, target)?;
    }
    Ok(())
}
