//! The C ABI of the tarwyn client.
//!
//! # Conventions
//!
//! - A `TarwynClient*` from `tarwyn_client_*` is valid until `tarwyn_client_free`,
//!   and every function may be called from any thread.
//! - Text is `(ptr, len)` UTF-8 without a terminator; invalid bytes are
//!   replaced. Nothing passed in is kept after the call.
//! - Returned `(uint8_t*, size_t)` belongs to the caller: free it with
//!   `tarwyn_bytes_free`. No value is `NULL`, or `false` for a scalar read.
//! - Number lists are packed native-endian arrays. String and byte lists are
//!   frames of native-endian `uint32_t` length plus bytes.
//! - Coordinates are `x, y`. Bezier points are `x, y, rotation_degrees`, `NaN`
//!   for none. A 2d pose is `x, y, rotation` in radians, a 3d pose
//!   `x, y, z, qw, qx, qy, qz`.
//! - Callbacks run on receive threads, and none starts after cancel. `drop`
//!   runs exactly once, possibly before `tarwyn_subscribe*` returns.
//!
//! # Safety
//!
//! Every pointer must be valid for its length, and a client must not be used
//! after `tarwyn_client_free`. Nothing is checked.

#![expect(
    clippy::missing_safety_doc,
    reason = "the crate doc's Safety section is the contract for every function here"
)]

use std::borrow::Cow;
use std::ffi::c_void;
use std::ptr;
use std::slice;

use tarwyn_client::ffi::Point;
use tarwyn_client::ffi::TarwynClient as Inner;

/// Bumped whenever a signature or encoding in this header changes. A wrapper
/// compares it against `tarwyn_abi_version()` before using anything else.
pub const TARWYN_ABI_VERSION: u32 = 1;

/// An opaque client handle.
#[derive(Debug)]
pub struct TarwynClient(Inner);

/// What the server reports about itself. See `tarwyn_get_server_statistics`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TarwynStatistics {
    pub channels: u64,
    pub values: u64,
    pub telemetry_subscribers: u64,
    pub uptime_seconds: u64,
    pub dropped_publishes: u64,
    pub dropped_logs: u64,
}

/// Receives a value or log line: the channel it arrived on and, for values,
/// the protobuf `SupportedValues` encoding of the value. For log lines, the
/// line.
pub type TarwynSampleFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
);

/// Receives a telemetry sample and the publisher's timestamp in microseconds
/// since the Unix epoch.
pub type TarwynTelemetryFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    timestamp_micros: u64,
    payload: *const u8,
    payload_len: usize,
);

/// Releases a callback's `ctx`. The function pointer itself may be `NULL`.
pub type TarwynDropFn = Option<unsafe extern "C" fn(ctx: *mut c_void)>;

unsafe fn text<'a>(ptr: *const u8, len: usize) -> Cow<'a, str> {
    if ptr.is_null() || len == 0 {
        return Cow::Borrowed("");
    }
    String::from_utf8_lossy(unsafe { slice::from_raw_parts(ptr, len) })
}

unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }
    unsafe { slice::from_raw_parts(ptr, len) }
}

unsafe fn array<'a, T>(ptr: *const T, count: usize) -> &'a [T] {
    if ptr.is_null() || count == 0 {
        return &[];
    }
    unsafe { slice::from_raw_parts(ptr, count) }
}

/// Hands `value` to the caller: writes its length and returns a pointer they
/// release with `tarwyn_bytes_free`.
unsafe fn give(value: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    let boxed = value.into_boxed_slice();
    unsafe { *out_len = boxed.len() };
    Box::into_raw(boxed).cast()
}

unsafe fn give_option(value: Option<Vec<u8>>, out_len: *mut usize) -> *mut u8 {
    match value {
        Some(value) => unsafe { give(value, out_len) },
        None => ptr::null_mut(),
    }
}

unsafe fn give_text(value: Option<String>, out_len: *mut usize) -> *mut u8 {
    unsafe { give_option(value.map(String::into_bytes), out_len) }
}

fn packed<T: Copy>(values: &[T]) -> Vec<u8> {
    let bytes = unsafe { slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) };
    bytes.to_vec()
}

fn frame<'a>(items: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        out.extend_from_slice(&(item.len() as u32).to_ne_bytes());
        out.extend_from_slice(item);
    }
    out
}

fn unframe(mut bytes: &[u8]) -> Vec<&[u8]> {
    let mut items = Vec::new();
    while bytes.len() >= 4 {
        let len = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let Some(item) = bytes.get(4..4 + len) else {
            break;
        };
        items.push(item);
        bytes = &bytes[4 + len..];
    }
    items
}

fn unframe_text(bytes: &[u8]) -> Vec<String> {
    unframe(bytes)
        .into_iter()
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect()
}

fn coordinates_from(xy: &[f64]) -> Vec<(f64, f64)> {
    xy.as_chunks::<2>()
        .0
        .iter()
        .map(|[x, y]| (*x, *y))
        .collect()
}

fn coordinates_into(pairs: &[(f64, f64)]) -> Vec<f64> {
    pairs.iter().flat_map(|(x, y)| [*x, *y]).collect()
}

fn points_from(xyr: &[f64]) -> Vec<Point> {
    xyr.as_chunks::<3>()
        .0
        .iter()
        .map(|[x, y, rotation]| Point {
            x: *x,
            y: *y,
            rotation_degrees: (!rotation.is_nan()).then_some(*rotation),
        })
        .collect()
}

fn points_into(points: &[Point]) -> Vec<f64> {
    points
        .iter()
        .flat_map(|point| [point.x, point.y, point.rotation_degrees.unwrap_or(f64::NAN)])
        .collect()
}

/// A callback and the foreign context it runs with, released when the
/// subscription is.
struct Foreign<F> {
    call: F,
    ctx: *mut c_void,
    drop: TarwynDropFn,
}

unsafe impl<F> Send for Foreign<F> {}
unsafe impl<F> Sync for Foreign<F> {}

impl<F> Drop for Foreign<F> {
    fn drop(&mut self) {
        if let Some(release) = self.drop {
            unsafe { release(self.ctx) };
        }
    }
}

impl Foreign<TarwynSampleFn> {
    fn sample(&self, channel: &str, value: &[u8]) {
        unsafe {
            (self.call)(
                self.ctx,
                channel.as_ptr(),
                channel.len(),
                value.as_ptr(),
                value.len(),
            )
        }
    }
}

impl Foreign<TarwynTelemetryFn> {
    fn telemetry(&self, timestamp: u64, payload: &[u8]) {
        unsafe { (self.call)(self.ctx, timestamp, payload.as_ptr(), payload.len()) }
    }
}

/// The ABI this library was built with. Compare with `TARWYN_ABI_VERSION`.
#[unsafe(no_mangle)]
pub extern "C" fn tarwyn_abi_version() -> u32 {
    TARWYN_ABI_VERSION
}

/// Boxes a constructed client, or hands back `NULL` for one that could not be
/// built.
fn give_client(client: Result<Inner, tarwyn_client::ConnectError>) -> *mut TarwynClient {
    match client {
        Ok(client) => Box::into_raw(Box::new(TarwynClient(client))),
        Err(_) => ptr::null_mut(),
    }
}

/// A client for a server on this machine, or `NULL` when no socket could be
/// bound.
#[unsafe(no_mangle)]
pub extern "C" fn tarwyn_client_new() -> *mut TarwynClient {
    give_client(Inner::new())
}

/// A client for the server on `host`, an address, not a URL.
///
/// `NULL` when `host` does not resolve or no socket could be bound. The
/// server being absent is not an error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_client_connect(
    host: *const u8,
    host_len: usize,
) -> *mut TarwynClient {
    let host = unsafe { text(host, host_len) };
    give_client(Inner::connect(&host))
}

/// A client with every port, timeout and window spelled out, or `NULL` as for
/// `tarwyn_client_connect`.
///
/// `busy_poll_micros` is how long the reader spins before each blocking read,
/// and 0 blocks right away. `predict_micros` is how long it spins around a
/// predicted arrival, where 0 turns prediction off and the usual value comes
/// from `tarwyn_default_predict_micros()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_client_with_ports(
    host: *const u8,
    host_len: usize,
    port: u16,
    telemetry_port: u16,
    request_timeout_ms: u64,
    send_high_water_mark: i32,
    busy_poll_micros: u64,
    predict_micros: u64,
) -> *mut TarwynClient {
    let host = unsafe { text(host, host_len) };
    give_client(Inner::with_ports(
        &host,
        port,
        telemetry_port,
        request_timeout_ms,
        send_high_water_mark,
        busy_poll_micros,
        predict_micros,
    ))
}

/// The default `predict_micros`, so wrappers never copy the number.
#[unsafe(no_mangle)]
pub extern "C" fn tarwyn_default_predict_micros() -> u64 {
    u64::try_from(tarwyn_client::DEFAULT_PREDICT.as_micros()).unwrap_or(u64::MAX)
}

/// Stops the client, cancels its subscriptions and releases it. `NULL` is fine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_client_free(client: *mut TarwynClient) {
    if !client.is_null() {
        drop(unsafe { Box::from_raw(client) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_client_start(client: *const TarwynClient) {
    unsafe { client_ref(client) }.start();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_client_stop(client: *const TarwynClient) {
    unsafe { client_ref(client) }.stop();
}

/// Releases bytes the library handed out. `NULL` is fine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_bytes_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

unsafe fn client_ref<'a>(client: *const TarwynClient) -> &'a Inner {
    &unsafe { &*client }.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_string(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) {
    unsafe { client_ref(client).put_string(&text(channel, channel_len), &text(value, value_len)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_integer(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: i32,
) {
    unsafe { client_ref(client).put_integer(&text(channel, channel_len), value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_long(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: i64,
) {
    unsafe { client_ref(client).put_long(&text(channel, channel_len), value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_double(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: f64,
) {
    unsafe { client_ref(client).put_double(&text(channel, channel_len), value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_float(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: f32,
) {
    unsafe { client_ref(client).put_float(&text(channel, channel_len), value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_boolean(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: bool,
) {
    unsafe { client_ref(client).put_boolean(&text(channel, channel_len), value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_bytes(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) {
    unsafe { client_ref(client).put_bytes(&text(channel, channel_len), bytes(value, value_len)) }
}

/// `value` is a frame of strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_string_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) {
    let items = unframe_text(unsafe { bytes(value, value_len) });
    unsafe { client_ref(client).put_string_list(&text(channel, channel_len), &items) }
}

/// `value` is a frame of byte strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_bytes_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) {
    let items: Vec<Vec<u8>> = unframe(unsafe { bytes(value, value_len) })
        .into_iter()
        .map(<[u8]>::to_vec)
        .collect();
    unsafe { client_ref(client).put_bytes_list(&text(channel, channel_len), &items) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_double_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const f64,
    count: usize,
) {
    unsafe { client_ref(client).put_double_list(&text(channel, channel_len), array(value, count)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_float_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const f32,
    count: usize,
) {
    unsafe { client_ref(client).put_float_list(&text(channel, channel_len), array(value, count)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_integer_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const i32,
    count: usize,
) {
    unsafe { client_ref(client).put_integer_list(&text(channel, channel_len), array(value, count)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_long_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const i64,
    count: usize,
) {
    unsafe { client_ref(client).put_long_list(&text(channel, channel_len), array(value, count)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_boolean_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const bool,
    count: usize,
) {
    unsafe { client_ref(client).put_boolean_list(&text(channel, channel_len), array(value, count)) }
}

/// `xy` holds `count` points as `x, y` pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_coordinates(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    xy: *const f64,
    count: usize,
) {
    let pairs = coordinates_from(unsafe { array(xy, count * 2) });
    unsafe { client_ref(client).put_coordinates(&text(channel, channel_len), &pairs) }
}

/// `rotation` is in radians.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_pose2d(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    x: f64,
    y: f64,
    rotation: f64,
) {
    unsafe { client_ref(client).put_pose2d(&text(channel, channel_len), x, y, rotation) }
}

/// The rotation is a quaternion, `w` first.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_pose3d(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    x: f64,
    y: f64,
    z: f64,
    qw: f64,
    qx: f64,
    qy: f64,
    qz: f64,
) {
    unsafe { client_ref(client).put_pose3d(&text(channel, channel_len), [x, y, z, qw, qx, qy, qz]) }
}

/// `xyr` holds `count` control points as `x, y, rotation_degrees` triples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_bezier_curve(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    xyr: *const f64,
    count: usize,
) {
    let points = points_from(unsafe { array(xyr, count * 3) });
    unsafe { client_ref(client).put_bezier_curve(&text(channel, channel_len), &points) }
}

/// `value` is an encoded protobuf `BezierCurves`. False when it is not.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_bezier_curves(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe {
        client_ref(client).put_bezier_curves(&text(channel, channel_len), bytes(value, value_len))
    }
}

/// `value` is an encoded protobuf `BezierCurvesList`. False when it is not.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_bezier_curves_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe {
        client_ref(client)
            .put_bezier_curves_list(&text(channel, channel_len), bytes(value, value_len))
    }
}

/// False when `value` does not decode as `tarwyn_type`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_typed_bytes(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    tarwyn_type: i32,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe {
        client_ref(client).put_typed_bytes(
            &text(channel, channel_len),
            tarwyn_type,
            bytes(value, value_len),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_unknown_bytes(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) {
    unsafe {
        client_ref(client).put_unknown_bytes(&text(channel, channel_len), bytes(value, value_len))
    }
}

/// Publishes `packed` as a struct topic of `type_name`. `schemas` is a
/// frame of alternating struct names and their schemas, each announced once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_put_struct(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    type_name: *const u8,
    type_name_len: usize,
    schemas: *const u8,
    schemas_len: usize,
    packed: *const u8,
    packed_len: usize,
) {
    let entries = unframe_text(unsafe { bytes(schemas, schemas_len) });
    let pairs: Vec<(&str, &str)> = entries
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[name, schema]| (name.as_str(), schema.as_str()))
        .collect();
    unsafe {
        client_ref(client).put_struct(
            &text(channel, channel_len),
            &text(type_name, type_name_len),
            &pairs,
            bytes(packed, packed_len),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_string(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_text(
            client_ref(client).get_string(&text(channel, channel_len)),
            out_len,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_integer(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut i32,
) -> bool {
    unsafe {
        scalar(
            client_ref(client).get_integer(&text(channel, channel_len)),
            out,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_long(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut i64,
) -> bool {
    unsafe {
        scalar(
            client_ref(client).get_long(&text(channel, channel_len)),
            out,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_double(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut f64,
) -> bool {
    unsafe {
        scalar(
            client_ref(client).get_double(&text(channel, channel_len)),
            out,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_float(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut f32,
) -> bool {
    unsafe {
        scalar(
            client_ref(client).get_float(&text(channel, channel_len)),
            out,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_boolean(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut bool,
) -> bool {
    unsafe {
        scalar(
            client_ref(client).get_boolean(&text(channel, channel_len)),
            out,
        )
    }
}

unsafe fn scalar<T>(value: Option<T>, out: *mut T) -> bool {
    match value {
        Some(value) => {
            unsafe { *out = value };
            true
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_bytes(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_option(
            client_ref(client).get_bytes(&text(channel, channel_len)),
            out_len,
        )
    }
}

/// A frame of strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_string_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_string_list(&text(channel, channel_len)) };
    unsafe {
        give_option(
            list.map(|items| frame(items.iter().map(String::as_bytes))),
            out_len,
        )
    }
}

/// A frame of byte strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_bytes_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_bytes_list(&text(channel, channel_len)) };
    unsafe {
        give_option(
            list.map(|items| frame(items.iter().map(Vec::as_slice))),
            out_len,
        )
    }
}

/// Packed doubles. `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_double_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_double_list(&text(channel, channel_len)) };
    unsafe { give_option(list.as_deref().map(packed), out_len) }
}

/// Packed floats. `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_float_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_float_list(&text(channel, channel_len)) };
    unsafe { give_option(list.as_deref().map(packed), out_len) }
}

/// Packed 32-bit integers. `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_integer_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_integer_list(&text(channel, channel_len)) };
    unsafe { give_option(list.as_deref().map(packed), out_len) }
}

/// Packed 64-bit integers. `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_long_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_long_list(&text(channel, channel_len)) };
    unsafe { give_option(list.as_deref().map(packed), out_len) }
}

/// One byte per boolean.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_boolean_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let list = unsafe { client_ref(client).get_boolean_list(&text(channel, channel_len)) };
    unsafe { give_option(list.as_deref().map(packed), out_len) }
}

/// Packed doubles as `x, y` pairs. `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_coordinates(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let pairs = unsafe { client_ref(client).get_coordinates(&text(channel, channel_len)) };
    unsafe {
        give_option(
            pairs.map(|pairs| packed(&coordinates_into(&pairs))),
            out_len,
        )
    }
}

/// Packed doubles as `x, y, rotation_degrees` triples, `NaN` for no rotation;
/// `out_len` is in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_bezier_curve(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let points = unsafe { client_ref(client).get_bezier_curve(&text(channel, channel_len)) };
    unsafe { give_option(points.map(|points| packed(&points_into(&points))), out_len) }
}

/// An encoded protobuf `BezierCurves`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_bezier_curves(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_option(
            client_ref(client).get_bezier_curves(&text(channel, channel_len)),
            out_len,
        )
    }
}

/// An encoded protobuf `BezierCurvesList`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_bezier_curves_list(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_option(
            client_ref(client).get_bezier_curves_list(&text(channel, channel_len)),
            out_len,
        )
    }
}

/// Writes `x, y, rotation` (radians) to `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_pose2d(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut f64,
) -> bool {
    match unsafe { client_ref(client).get_pose2d(&text(channel, channel_len)) } {
        Some(fields) => {
            unsafe { ptr::copy_nonoverlapping(fields.as_ptr(), out, fields.len()) };
            true
        }
        None => false,
    }
}

/// Writes `x, y, z, qw, qx, qy, qz` to `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_pose3d(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out: *mut f64,
) -> bool {
    match unsafe { client_ref(client).get_pose3d(&text(channel, channel_len)) } {
        Some(fields) => {
            unsafe { ptr::copy_nonoverlapping(fields.as_ptr(), out, fields.len()) };
            true
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_unknown_bytes(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_option(
            client_ref(client).get_unknown_bytes(&text(channel, channel_len)),
            out_len,
        )
    }
}

/// How many channels were removed: 0 or 1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_delete(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
) -> u32 {
    unsafe { client_ref(client).delete(&text(channel, channel_len)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_delete_all(client: *const TarwynClient) -> u32 {
    unsafe { client_ref(client).delete_all() }
}

/// A frame of channel names under `prefix`. Empty, never `NULL`, when there
/// are none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_tables(
    client: *const TarwynClient,
    prefix: *const u8,
    prefix_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let tables = unsafe { client_ref(client).get_tables(&text(prefix, prefix_len)) };
    unsafe { give(frame(tables.iter().map(String::as_bytes)), out_len) }
}

/// The round trip to the server in nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_ping(client: *const TarwynClient, out_nanos: *mut u64) -> bool {
    unsafe { scalar(client_ref(client).get_ping(), out_nanos) }
}

/// Fills `out` and hands back the server's version string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_server_statistics(
    client: *const TarwynClient,
    out: *mut TarwynStatistics,
    out_version: *mut *mut u8,
    out_version_len: *mut usize,
) -> bool {
    let Some(statistics) = (unsafe { client_ref(client).get_server_statistics() }) else {
        return false;
    };
    unsafe {
        *out = TarwynStatistics {
            channels: statistics.channels,
            values: statistics.values,
            telemetry_subscribers: statistics.telemetry_subscribers,
            uptime_seconds: statistics.uptime_seconds,
            dropped_publishes: statistics.dropped_publishes,
            dropped_logs: statistics.dropped_logs,
        };
        *out_version = give(statistics.version.into_bytes(), out_version_len);
    }
    true
}

/// The JSON of everything under `prefix`. `{}`, never `NULL`, when the
/// server is absent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_get_raw_json(
    client: *const TarwynClient,
    prefix: *const u8,
    prefix_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give(
            client_ref(client)
                .get_raw_json(&text(prefix, prefix_len))
                .into_bytes(),
            out_len,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_compare_and_set_absent_string(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe {
        client_ref(client)
            .compare_and_set_absent_string(&text(channel, channel_len), &text(value, value_len))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_compare_and_set_string(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    expected: *const u8,
    expected_len: usize,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe {
        client_ref(client).compare_and_set_string(
            &text(channel, channel_len),
            &text(expected, expected_len),
            &text(value, value_len),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_compare_and_set_double(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    expected: f64,
    value: f64,
) -> bool {
    unsafe {
        client_ref(client).compare_and_set_double(&text(channel, channel_len), expected, value)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_compare_and_set_long(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    expected: i64,
    value: i64,
) -> bool {
    unsafe { client_ref(client).compare_and_set_long(&text(channel, channel_len), expected, value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_compare_and_set_boolean(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    expected: bool,
    value: bool,
) -> bool {
    unsafe {
        client_ref(client).compare_and_set_boolean(&text(channel, channel_len), expected, value)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_publish_telemetry(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    payload: *const u8,
    payload_len: usize,
) {
    unsafe {
        client_ref(client)
            .publish_telemetry(&text(channel, channel_len), bytes(payload, payload_len))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_log_to(
    client: *const TarwynClient,
    path: *const u8,
    path_len: usize,
) -> bool {
    unsafe { client_ref(client).log_to(&text(path, path_len)) }
}

/// The path the log landed at, or `NULL` when it could not be opened.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_log_to_drive(
    client: *const TarwynClient,
    filename: *const u8,
    filename_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    unsafe {
        give_text(
            client_ref(client).log_to_drive(&text(filename, filename_len)),
            out_len,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_dropped_log_records(client: *const TarwynClient) -> u64 {
    unsafe { client_ref(client).dropped_log_records() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_logging_healthy(client: *const TarwynClient) -> bool {
    unsafe { client_ref(client).logging_healthy() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_dropped_publishes(client: *const TarwynClient) -> u64 {
    unsafe { client_ref(client).dropped_publishes() }
}

/// False when `channel` already has a subscription, in which case `drop` has
/// already run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_subscribe(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    callback: TarwynSampleFn,
    ctx: *mut c_void,
    drop: TarwynDropFn,
) -> bool {
    let foreign = Foreign {
        call: callback,
        ctx,
        drop,
    };
    unsafe {
        client_ref(client).subscribe(&text(channel, channel_len), move |channel, value| {
            foreign.sample(channel, value)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_unsubscribe(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
) -> bool {
    unsafe { client_ref(client).unsubscribe(&text(channel, channel_len)) }
}

/// False when `channel` already has a subscription or the telemetry plane
/// refused it, in which case `drop` has already run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_subscribe_telemetry(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
    callback: TarwynTelemetryFn,
    ctx: *mut c_void,
    drop: TarwynDropFn,
) -> bool {
    let foreign = Foreign {
        call: callback,
        ctx,
        drop,
    };
    unsafe {
        client_ref(client)
            .subscribe_telemetry(&text(channel, channel_len), move |timestamp, payload| {
                foreign.telemetry(timestamp, payload)
            })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_unsubscribe_telemetry(
    client: *const TarwynClient,
    channel: *const u8,
    channel_len: usize,
) -> bool {
    unsafe { client_ref(client).unsubscribe_telemetry(&text(channel, channel_len)) }
}

/// Lines arrive on the channel `logs`. False when logs are already subscribed,
/// in which case `drop` has already run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_subscribe_to_logs(
    client: *const TarwynClient,
    callback: TarwynSampleFn,
    ctx: *mut c_void,
    drop: TarwynDropFn,
) -> bool {
    let foreign = Foreign {
        call: callback,
        ctx,
        drop,
    };
    unsafe {
        client_ref(client).subscribe_to_logs(move |channel, line| foreign.sample(channel, line))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tarwyn_unsubscribe_from_logs(client: *const TarwynClient) -> bool {
    unsafe { client_ref(client).unsubscribe_from_logs() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips_and_stops_at_a_truncated_item() {
        let framed = frame([b"ab".as_slice(), b"".as_slice(), b"xyz".as_slice()]);
        assert_eq!(unframe(&framed), vec![b"ab".as_slice(), b"", b"xyz"]);
        assert_eq!(
            unframe(&framed[..framed.len() - 1]),
            vec![b"ab".as_slice(), b""]
        );
        assert!(unframe(&[1, 2]).is_empty());
    }

    #[test]
    fn numbers_pack_as_their_native_bytes() {
        let values = [1.5f64, -2.0, 0.0];
        assert_eq!(packed(&values).len(), 24);
        assert_eq!(packed(&values)[..8], 1.5f64.to_ne_bytes());
        assert_eq!(packed(&[true, false]), vec![1, 0]);
    }

    #[test]
    fn a_nan_rotation_means_no_rotation() {
        let points = points_from(&[1.0, 2.0, f64::NAN, 3.0, 4.0, 90.0]);
        assert_eq!(points[0].rotation_degrees, None);
        assert_eq!(points[1].rotation_degrees, Some(90.0));
        let back = points_into(&points);
        assert!(back[2].is_nan());
        assert_eq!(back[5], 90.0);
    }

    #[test]
    fn bytes_handed_out_can_be_handed_back() {
        let mut len = 0usize;
        let ptr = unsafe { give(vec![1, 2, 3], &mut len) };
        assert_eq!(len, 3);
        unsafe { tarwyn_bytes_free(ptr, len) };
        let empty = unsafe { give(Vec::new(), &mut len) };
        assert!(!empty.is_null());
        assert_eq!(len, 0);
        unsafe { tarwyn_bytes_free(empty, len) };
        unsafe { tarwyn_bytes_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn a_refused_subscription_releases_its_context_before_returning() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn update(
            _: *mut c_void,
            _: *const u8,
            _: usize,
            _: *const u8,
            _: usize,
        ) {
        }
        unsafe extern "C" fn release(_: *mut c_void) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }

        let client = unsafe {
            tarwyn_client_with_ports(b"127.0.0.1".as_ptr(), 9, 26683, 26684, 150, 500, 0, 0)
        };
        let channel = b"pose";
        unsafe {
            assert!(tarwyn_subscribe(
                client,
                channel.as_ptr(),
                4,
                update,
                ptr::null_mut(),
                Some(release)
            ));
            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
            assert!(!tarwyn_subscribe(
                client,
                channel.as_ptr(),
                4,
                update,
                ptr::null_mut(),
                Some(release)
            ));
            assert_eq!(DROPS.load(Ordering::SeqCst), 1);
            assert!(tarwyn_unsubscribe(client, channel.as_ptr(), 4));
            tarwyn_client_free(client);
        }
        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }
}
