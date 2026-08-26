// Generated from clients/api.toml by codegen. Do not edit.
#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

use tarwyn_protobuf::protobuf::supported_values::Kind;

use crate::PyTarwynClient;

#[pymethods]
impl PyTarwynClient {
    fn put_string(&self, python: Python<'_>, channel: &str, value: &str) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::String(value.to_string()))
        });
    }

    fn put_integer(&self, python: Python<'_>, channel: &str, value: i32) {
        python.detach(|| self.inner.send_message_public(channel, Kind::Int32(value)));
    }

    fn put_long(&self, python: Python<'_>, channel: &str, value: i64) {
        python.detach(|| self.inner.send_message_public(channel, Kind::Int64(value)));
    }

    fn put_double(&self, python: Python<'_>, channel: &str, value: f64) {
        python.detach(|| self.inner.send_message_public(channel, Kind::Double(value)));
    }

    fn put_float(&self, python: Python<'_>, channel: &str, value: f32) {
        python.detach(|| self.inner.send_message_public(channel, Kind::Float(value)));
    }

    fn put_boolean(&self, python: Python<'_>, channel: &str, value: bool) {
        python.detach(|| self.inner.send_message_public(channel, Kind::Bool(value)));
    }

    fn put_pose2d(&self, python: Python<'_>, channel: &str, x: f64, y: f64, rotation: f64) {
        let fields = [x, y, rotation];
        let mut packed = Vec::with_capacity(fields.len() * 8);
        for field in fields {
            packed.extend_from_slice(&field.to_le_bytes());
        }
        python.detach(|| self.inner.send_message_public(channel, Kind::Bytes(packed)));
    }

    fn put_pose3d(
        &self,
        python: Python<'_>,
        channel: &str,
        x: f64,
        y: f64,
        z: f64,
        roll: f64,
        pitch: f64,
        yaw: f64,
    ) {
        let fields = [x, y, z, roll, pitch, yaw];
        let mut packed = Vec::with_capacity(fields.len() * 8);
        for field in fields {
            packed.extend_from_slice(&field.to_le_bytes());
        }
        python.detach(|| self.inner.send_message_public(channel, Kind::Bytes(packed)));
    }
}
