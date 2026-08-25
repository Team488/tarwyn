use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use tarwyn_client::tarwyn_client::{TarwynClient, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values::Kind;

#[pyclass(name = "Subscription")]
struct PySubscription {
    values: Arc<Mutex<Vec<Kind>>>,
    unsubscribe: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[pymethods]
impl PySubscription {
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

    fn __len__(&self) -> usize {
        self.values.lock().map(|v| v.len()).unwrap_or(0)
    }

    fn close(&self) {
        let taken = self.unsubscribe.lock().ok().and_then(|mut slot| slot.take());
        if let Some(unsubscribe) = taken {
            unsubscribe();
        }
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

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
        Kind::StringList(list) => list.values.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::FloatList(list) => list.values.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::BoolList(list) => list.values.into_pyobject(python).unwrap().into_any().unbind(),
        Kind::BytesList(list) => list
            .values
            .into_iter()
            .map(|value| PyBytes::new(python, &value).unbind())
            .collect::<Vec<_>>()
            .into_pyobject(python)
            .unwrap()
            .into_any()
            .unbind(),
    }
}

type CallbackKey = (String, usize);

#[pyclass(name = "TarwynClient")]
struct PyTarwynClient {
    inner: Arc<TarwynClient>,
    callbacks: Arc<Mutex<HashMap<CallbackKey, Box<dyn FnOnce() + Send>>>>,
}

#[pymethods]
impl PyTarwynClient {
    #[new]
    #[pyo3(signature = (host="127.0.0.1", push_port=5557, req_port=5556, sub_port=5555,
                        request_timeout_ms=500, send_high_water_mark=500))]
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
            Err(_) => Err(PyRuntimeError::new_err("subscription registry was poisoned")),
        }
    }

    #[pyo3(name = "unsubscribe")]
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

    fn start(&self) {
        self.inner.start();
    }

    fn stop(&self) {
        self.inner.stop();
    }

    fn dropped_publishes(&self) -> u64 {
        self.inner.dropped_publishes()
    }

    fn put_double(&self, python: Python<'_>, channel: &str, value: f64) {
        python.detach(|| self.inner.send_double(channel, value));
    }

    fn put_integer(&self, python: Python<'_>, channel: &str, value: i64) {
        python.detach(|| self.inner.send_i64(channel, value));
    }

    fn put_boolean(&self, python: Python<'_>, channel: &str, value: bool) {
        python.detach(|| self.inner.send_bool(channel, value));
    }

    fn put_string(&self, python: Python<'_>, channel: &str, value: &str) {
        python.detach(|| self.inner.send_string(channel, value));
    }

    fn put_bytes(&self, python: Python<'_>, channel: &str, value: &[u8]) {
        python.detach(|| self.inner.send_bytes(channel, value));
    }

    fn put_string_list(&self, python: Python<'_>, channel: &str, value: Vec<String>) {
        python.detach(|| self.inner.send_string_list(channel, &value));
    }

    fn put_float_list(&self, python: Python<'_>, channel: &str, value: Vec<f32>) {
        python.detach(|| self.inner.send_float_list(channel, &value));
    }

    fn put_boolean_list(&self, python: Python<'_>, channel: &str, value: Vec<bool>) {
        python.detach(|| self.inner.send_bool_list(channel, &value));
    }

    fn put_bytes_list(&self, python: Python<'_>, channel: &str, value: Vec<Vec<u8>>) {
        python.detach(|| self.inner.send_bytes_list(channel, &value));
    }

    fn get(&self, python: Python<'_>, channel: &str) -> Option<Py<PyAny>> {
        let value = python.detach(|| self.inner.get(channel))?;
        Some(kind_to_python(python, value))
    }

    fn get_double(&self, python: Python<'_>, channel: &str) -> Option<f64> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Double(value)) => Some(value),
            _ => None,
        }
    }

    fn get_string_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<String>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::StringList(list)) => Some(list.values),
            _ => None,
        }
    }

    #[pyo3(name = "subscribe_buffered", signature = (channel, depth=64))]
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
