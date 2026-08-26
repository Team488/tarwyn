// Generated from clients/api.toml by codegen. Do not edit.
#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{
    BoolList, BytesList, DoubleList, FloatList, IntegerList, LongList, StringList,
};

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

    fn get_string(&self, python: Python<'_>, channel: &str) -> Option<String> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::String(value)) => Some(value),
            _ => None,
        }
    }

    fn get_integer(&self, python: Python<'_>, channel: &str) -> Option<i32> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Int32(value)) => Some(value),
            _ => None,
        }
    }

    fn get_long(&self, python: Python<'_>, channel: &str) -> Option<i64> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Int64(value)) => Some(value),
            _ => None,
        }
    }

    fn get_double(&self, python: Python<'_>, channel: &str) -> Option<f64> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Double(value)) => Some(value),
            _ => None,
        }
    }

    fn get_float(&self, python: Python<'_>, channel: &str) -> Option<f32> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Float(value)) => Some(value),
            _ => None,
        }
    }

    fn get_boolean(&self, python: Python<'_>, channel: &str) -> Option<bool> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::Bool(value)) => Some(value),
            _ => None,
        }
    }

    fn put_string_list(&self, python: Python<'_>, channel: &str, items: Vec<String>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::StringList(StringList { values: items }))
        });
    }

    fn get_string_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<String>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::StringList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_bytes_list(&self, python: Python<'_>, channel: &str, items: Vec<Vec<u8>>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::BytesList(BytesList { values: items }))
        });
    }

    fn get_bytes_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<Vec<u8>>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::BytesList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_double_list(&self, python: Python<'_>, channel: &str, items: Vec<f64>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::DoubleList(DoubleList { values: items }))
        });
    }

    fn get_double_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<f64>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::DoubleList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_float_list(&self, python: Python<'_>, channel: &str, items: Vec<f32>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::FloatList(FloatList { values: items }))
        });
    }

    fn get_float_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<f32>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::FloatList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_integer_list(&self, python: Python<'_>, channel: &str, items: Vec<i32>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::IntegerList(IntegerList { values: items }))
        });
    }

    fn get_integer_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<i32>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::IntegerList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_long_list(&self, python: Python<'_>, channel: &str, items: Vec<i64>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::LongList(LongList { values: items }))
        });
    }

    fn get_long_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<i64>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::LongList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn put_boolean_list(&self, python: Python<'_>, channel: &str, items: Vec<bool>) {
        python.detach(|| {
            self.inner
                .send_message_public(channel, Kind::BoolList(BoolList { values: items }))
        });
    }

    fn get_boolean_list(&self, python: Python<'_>, channel: &str) -> Option<Vec<bool>> {
        match python.detach(|| self.inner.get(channel)) {
            Some(Kind::BoolList(list)) => Some(list.values),
            _ => None,
        }
    }

    fn get_pose2d(&self, python: Python<'_>, channel: &str) -> Option<Vec<f64>> {
        let Some(Kind::Bytes(bytes)) = python.detach(|| self.inner.get(channel)) else {
            return None;
        };
        if bytes.len() != 3 * 8 {
            return None;
        }
        let (fields, _) = bytes.as_chunks::<8>();
        Some(fields.iter().copied().map(f64::from_le_bytes).collect())
    }

    fn get_pose3d(&self, python: Python<'_>, channel: &str) -> Option<Vec<f64>> {
        let Some(Kind::Bytes(bytes)) = python.detach(|| self.inner.get(channel)) else {
            return None;
        };
        if bytes.len() != 6 * 8 {
            return None;
        }
        let (fields, _) = bytes.as_chunks::<8>();
        Some(fields.iter().copied().map(f64::from_le_bytes).collect())
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
