//! Publishing and reading values by their concrete type.

use serde_json::Map;

use prost::Message;
use tarwyn_protobuf::protobuf::{
    BezierCurve, BezierCurves, BezierCurvesList, BoolList, BytesList, Coordinate, CoordinateList,
    DoubleList, FloatList, IntegerList, LongList, StringList, supported_values,
};

use tarwyn_server::value::Value;
use tarwyn_server::websocket::protocol::encode_once;

use crate::client::{Client, decode_tarwyn_type};
use crate::connection::now_micros;

/// Packs doubles into WPILib's struct layout: little-endian, no padding.
pub(crate) fn pack_le_doubles(fields: &[f64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(fields.len() * 8);
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes
}

impl Client {
    /// Publishes a value under a WPILib struct type string, with its schemas.
    ///
    /// The bytes already match WPILib's packed layout; naming the type is what
    /// lets a dashboard decode them instead of showing raw bytes.
    ///
    /// A schema is published once per channel rather than alongside every
    /// value: the bytes never change, and re-sending them would put three or
    /// four extra frames on the wire for every pose. They are replayed with the
    /// rest of the session if the connection is remade.
    pub fn send_struct(
        &self,
        channel: &str,
        type_name: &str,
        schemas: &[(&str, &str)],
        packed: Vec<u8>,
    ) {
        for (name, schema) in schemas {
            let schema_channel = format!("/.schema/{name}");
            if self
                .pubuids
                .lock()
                .is_ok_and(|pubuids| pubuids.contains_key(&schema_channel))
            {
                continue;
            }
            let mut retained = Map::new();
            retained.insert("retained".into(), serde_json::Value::Bool(true));
            let frame = self.publish_typed(
                &schema_channel,
                supported_values::Kind::Bytes(schema.as_bytes().to_vec()),
                Some("structschema"),
                retained,
            );
            self.remember(format!("schema:{schema_channel}"), frame);
        }
        self.publish_typed(
            channel,
            supported_values::Kind::Bytes(packed),
            Some(type_name),
            Map::new(),
        );
    }

    /// Publish a pose on the field plane as a WPILib `struct:Pose2d` topic.
    ///
    /// `rotation` is in radians, matching WPILib's `Rotation2d`.
    pub fn send_pose2d_struct(&self, channel: &str, x: f64, y: f64, rotation: f64) {
        let packed = pack_le_doubles(&[x, y, rotation]);
        self.send_struct(channel, "struct:Pose2d", Self::POSE2D_SCHEMAS, packed);
    }

    /// Publish a pose in space as a WPILib `struct:Pose3d` topic.
    ///
    /// Rotation is a quaternion written `w` first, matching WPILib's layout.
    #[expect(clippy::too_many_arguments)]
    pub fn send_pose3d_struct(
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
        let packed = pack_le_doubles(&[x, y, z, qw, qx, qy, qz]);
        self.send_struct(channel, "struct:Pose3d", Self::POSE3D_SCHEMAS, packed);
    }

    /// Publishes a value under a declared type, returning the frame it sent.
    fn publish_typed(
        &self,
        channel: &str,
        kind: supported_values::Kind,
        declared_type: Option<&str>,
        properties: Map<String, serde_json::Value>,
    ) -> Vec<u8> {
        self.ensure_reader();
        let value = Value::from(kind);
        let pubuid = self.ensure_pubuid_typed(channel, &value, declared_type, properties);
        let frame = encode_once(&value, now_micros(), pubuid).to_vec();
        self.dispatch_frame(frame.clone());
        frame
    }

    /// Publish a string.
    pub fn send_string(&self, channel: &str, data: &str) {
        self.send_message(channel, supported_values::Kind::String(data.to_string()));
    }

    /// Publish a 32-bit signed integer.
    pub fn send_i32(&self, channel: &str, data: i32) {
        self.send_message(channel, supported_values::Kind::Int32(data));
    }

    /// Publish a 64-bit signed integer.
    pub fn send_i64(&self, channel: &str, data: i64) {
        self.send_message(channel, supported_values::Kind::Int64(data));
    }

    /// Publish a 32-bit unsigned integer.
    pub fn send_u32(&self, channel: &str, data: u32) {
        self.send_message(channel, supported_values::Kind::Uint32(data));
    }

    /// Publish a 64-bit unsigned integer.
    pub fn send_u64(&self, channel: &str, data: u64) {
        self.send_message(channel, supported_values::Kind::Uint64(data));
    }

    /// Publish a boolean.
    pub fn send_bool(&self, channel: &str, data: bool) {
        self.send_message(channel, supported_values::Kind::Bool(data));
    }

    /// Publish a double.
    pub fn send_double(&self, channel: &str, data: f64) {
        self.send_message(channel, supported_values::Kind::Double(data));
    }

    /// Publish a float. TARWYN has no `putFloat`; this is an addition.
    pub fn send_float(&self, channel: &str, data: f32) {
        self.send_message(channel, supported_values::Kind::Float(data));
    }

    /// Publish raw bytes.
    pub fn send_bytes(&self, channel: &str, data: &[u8]) {
        self.send_message(channel, supported_values::Kind::Bytes(data.to_vec()));
    }

    /// Publish a list of strings.
    pub fn send_string_list(&self, channel: &str, data: &[String]) {
        self.send_message(
            channel,
            supported_values::Kind::StringList(StringList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of floats.
    pub fn send_float_list(&self, channel: &str, data: &[f32]) {
        self.send_message(
            channel,
            supported_values::Kind::FloatList(FloatList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of byte strings.
    pub fn send_bytes_list(&self, channel: &str, data: &[Vec<u8>]) {
        self.send_message(
            channel,
            supported_values::Kind::BytesList(BytesList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of booleans.
    pub fn send_bool_list(&self, channel: &str, data: &[bool]) {
        self.send_message(
            channel,
            supported_values::Kind::BoolList(BoolList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of doubles.
    pub fn send_double_list(&self, channel: &str, data: &[f64]) {
        self.send_message(
            channel,
            supported_values::Kind::DoubleList(DoubleList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of 32-bit integers.
    pub fn send_integer_list(&self, channel: &str, data: &[i32]) {
        self.send_message(
            channel,
            supported_values::Kind::IntegerList(IntegerList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of 64-bit integers.
    pub fn send_long_list(&self, channel: &str, data: &[i64]) {
        self.send_message(
            channel,
            supported_values::Kind::LongList(LongList {
                values: data.to_vec(),
            }),
        );
    }

    /// Publish a list of `(x, y)` coordinates.
    pub fn send_coordinates(&self, channel: &str, data: &[(f64, f64)]) {
        self.send_message(
            channel,
            supported_values::Kind::CoordinateList(CoordinateList {
                coordinates: data
                    .iter()
                    .map(|(x, y)| Coordinate { x: *x, y: *y })
                    .collect(),
            }),
        );
    }

    /// Publish one bezier curve.
    pub fn send_bezier_curve(&self, channel: &str, curve: BezierCurve) {
        self.send_message(channel, supported_values::Kind::BezierCurve(curve));
    }

    /// Publish a bezier path: a set of curves plus its traversal options.
    pub fn send_bezier_curves(&self, channel: &str, curves: BezierCurves) {
        self.send_message(channel, supported_values::Kind::BezierCurves(curves));
    }

    /// Publish several bezier paths as one value.
    pub fn send_bezier_curves_list(&self, channel: &str, values: Vec<BezierCurves>) {
        self.send_message(
            channel,
            supported_values::Kind::BezierCurvesList(BezierCurvesList { values }),
        );
    }

    /// Publish bytes whose type the caller does not know. Equivalent to
    /// [`send_bytes`](Self::send_bytes); present to match TARWYN' `putUnknownBytes`.
    pub fn send_unknown_bytes(&self, channel: &str, data: &[u8]) {
        self.send_bytes(channel, data);
    }

    /// Read a channel holding raw bytes. `None` if it is absent or holds another type.
    pub fn get_unknown_bytes(&self, channel: &str) -> Option<Vec<u8>> {
        match self.get(channel)? {
            Value::Bytes(value) => Some(value),
            _ => None,
        }
    }

    /// Publish a value that is already encoded in TARWYN' byte layout.
    ///
    /// `tarwyn_type` is TARWYN' own type tag. An unrecognised tag is published as
    /// raw bytes. Returns `false`, publishing nothing, only when a recognised tag
    /// comes with bytes that are not a valid value of that type.
    pub fn send_typed_bytes(&self, channel: &str, tarwyn_type: i32, data: &[u8]) -> bool {
        let Some(kind) = decode_tarwyn_type(tarwyn_type, data) else {
            return false;
        };
        self.send_message(channel, kind);
        true
    }

    /// Read a coordinate list. `None` if the channel is absent or holds another type.
    pub fn get_coordinates(&self, channel: &str) -> Option<Vec<(f64, f64)>> {
        match self.get(channel)? {
            Value::Coordinate(bytes) => Some(
                CoordinateList::decode(bytes.as_slice())
                    .ok()?
                    .coordinates
                    .into_iter()
                    .map(|coordinate| (coordinate.x, coordinate.y))
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Read one bezier curve. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curve(&self, channel: &str) -> Option<BezierCurve> {
        match self.get(channel)? {
            Value::Bezier(bytes) => BezierCurve::decode(bytes.as_slice()).ok(),
            _ => None,
        }
    }

    /// Read a bezier path. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curves(&self, channel: &str) -> Option<BezierCurves> {
        match self.get(channel)? {
            Value::Bezier(bytes) => BezierCurves::decode(bytes.as_slice()).ok(),
            _ => None,
        }
    }

    /// Read a list of bezier paths. `None` if the channel is absent or holds another type.
    pub fn get_bezier_curves_list(&self, channel: &str) -> Option<Vec<BezierCurves>> {
        match self.get(channel)? {
            Value::Bezier(bytes) => Some(BezierCurvesList::decode(bytes.as_slice()).ok()?.values),
            _ => None,
        }
    }
}
