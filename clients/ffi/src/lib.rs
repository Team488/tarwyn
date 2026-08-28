//! The C ABI for the TARWYN client.
//!
//! Every function here is `extern "C"` and safe to call from any language with an
//! FFI. `clients/ffi/include/tarwyn.h` is generated from this file by cbindgen,
//! and the Java client's bindings are generated from that header, so this is the
//! single definition all three follow.
//!
//! # Conventions
//!
//! A client is created by [`xt_client_new`] and must be released with
//! [`xt_client_free`]. Every other function takes that handle.
//!
//! Calls return [`XT_OK`] or one of the `XT_ERR_*` codes. A Rust panic is caught
//! at the boundary and reported as [`XT_ERR_PANIC`] rather than unwound into C,
//! which would be undefined behaviour.
//!
//! Functions that return variable-length data take `out`, `capacity` and
//! `out_len`. `out_len` always receives the full length the value needs, even when
//! `out` is null or too small to hold it, so calling once with a null `out` sizes
//! the buffer and calling again fills it. Only `min(length, capacity)` is ever
//! written.
//!
//! Lists of variable-width items — [`xt_tables`] and the string list — are packed
//! into one buffer as a little-endian `u32` count, then for each item a
//! little-endian `u32` length followed by its bytes. Fixed-width lists are passed
//! flat instead, with no framing.

#![warn(missing_docs)]

mod generated;

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message as _;
use tarwyn_client::tarwyn_client::{TarwynClient, TarwynConfig};
use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{BezierCurve, BezierCurves, BezierCurvesList};

/// The call succeeded.
pub const XT_OK: c_int = 0;
/// A required pointer was null, or an argument was out of range.
pub const XT_ERR_NULL: c_int = -1;
/// A string argument was not valid UTF-8.
pub const XT_ERR_UTF8: c_int = -2;
/// The channel holds nothing, or the server did not answer.
pub const XT_ERR_NO_VALUE: c_int = -3;
/// The channel holds a value of a different type.
pub const XT_ERR_WRONG_TYPE: c_int = -4;
/// Rust panicked. The panic was caught at the boundary, not unwound into C.
pub const XT_ERR_PANIC: c_int = -5;
/// A filesystem operation failed.
pub const XT_ERR_IO: c_int = -6;

/// An opaque client, owned by the caller between [`xt_client_new`] and
/// [`xt_client_free`].
pub struct Handle {
    client: TarwynClient,
    subscriptions: Mutex<HashMap<u64, Box<dyn FnOnce() + Send>>>,
    next_id: AtomicU64,
    rings: Mutex<HashMap<u64, Arc<Ring>>>,
}

/// The shared buffer a subscription writes into.
///
/// Created by [`xt_subscribe_ring`]. The caller reads the bytes directly through
/// the pointer from [`xt_ring_base`], using [`xt_ring_write_index`] to learn how
/// far the writer has reached.
pub struct Ring {
    slots: Mutex<Vec<u8>>,
    write_index: AtomicU64,
    capacity: usize,
    record: usize,
}

impl Ring {
    fn new(records: usize, record: usize) -> Self {
        Ring {
            slots: Mutex::new(vec![0u8; records * record]),
            write_index: AtomicU64::new(0),
            capacity: records,
            record,
        }
    }

    fn push(&self, payload: &[u8]) {
        let Ok(mut slots) = self.slots.lock() else {
            return;
        };
        let sequence = self.write_index.load(Ordering::Relaxed);
        let start = (sequence as usize % self.capacity) * self.record;
        let len = payload.len().min(self.record - 8);
        slots[start..start + 8].copy_from_slice(&(len as u64).to_le_bytes());
        slots[start + 8..start + 8 + len].copy_from_slice(&payload[..len]);
        self.write_index.store(sequence + 1, Ordering::Release);
    }
}

pub(crate) fn to_str<'a>(pointer: *const c_char) -> Option<&'a str> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }.to_str().ok()
}

pub(crate) fn guard<F: FnOnce() -> c_int>(body: F) -> c_int {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(XT_ERR_PANIC)
}

pub(crate) fn decode_packed(buffer: &[u8]) -> Option<Vec<Vec<u8>>> {
    let count = u32::from_le_bytes(buffer.get(0..4)?.try_into().ok()?) as usize;
    let mut items = Vec::with_capacity(count.min(1024));
    let mut cursor = 4;
    for _ in 0..count {
        let len = u32::from_le_bytes(buffer.get(cursor..cursor + 4)?.try_into().ok()?) as usize;
        cursor += 4;
        items.push(buffer.get(cursor..cursor + len)?.to_vec());
        cursor += len;
    }
    Some(items)
}

pub(crate) fn encode_packed<'a, I>(items: I) -> Vec<u8>
where
    I: IntoIterator<Item = &'a [u8]>,
    I::IntoIter: ExactSizeIterator,
{
    let items = items.into_iter();
    let mut out = Vec::with_capacity(4 + items.len() * 8);
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(&(item.len() as u32).to_le_bytes());
        out.extend_from_slice(item);
    }
    out
}

pub(crate) fn copy_out<T: Copy>(source: &[T], out: *mut T, capacity: usize, out_len: *mut usize) {
    if !out_len.is_null() {
        unsafe { *out_len = source.len() };
    }
    if out.is_null() {
        return;
    }
    let copied = source.len().min(capacity);
    unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), out, copied) };
}

/// Construct a client. Returns null if `host` is null or not UTF-8.
///
/// Connecting never blocks: ZeroMQ dials in the background, so this succeeds
/// before the server exists. Nothing is received until [`xt_client_start`].
/// The result must be released with [`xt_client_free`].
///
/// # Safety
///
/// `host` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_client_new(
    host: *const c_char,
    push_port: u16,
    req_port: u16,
    sub_port: u16,
    request_timeout_ms: u64,
    send_high_water_mark: c_int,
) -> *mut Handle {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let host = to_str(host)?;
        let client = TarwynClient::with_config(TarwynConfig {
            host: host.to_string(),
            push_port,
            req_port,
            sub_port,
            request_timeout: Duration::from_millis(request_timeout_ms),
            send_high_water_mark,
            telemetry_port: tarwyn_protobuf::telemetry::DEFAULT_TELEMETRY_PORT,
        });
        Some(Box::into_raw(Box::new(Handle {
            client,
            subscriptions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            rings: Mutex::new(HashMap::new()),
        })))
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

/// Start the receive threads, so subscriptions begin delivering.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_client_start(handle: *mut Handle) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        handle.client.start();
        XT_OK
    })
}

/// Stop the client, drop every subscription, and release the handle.
///
/// Null is accepted and ignored. Any ring pointer from [`xt_ring_base`] dangles
/// after this returns.
///
/// # Safety
///
/// `handle` must be null, or a handle from [`xt_client_new`] that has not already
/// been freed. It must not be used again afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_client_free(handle: *mut Handle) {
    if handle.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let handle = unsafe { Box::from_raw(handle) };
        handle.client.stop();
        if let Ok(mut subscriptions) = handle.subscriptions.lock() {
            for (_, unsubscribe) in subscriptions.drain() {
                unsubscribe();
            }
        }
    }));
}

/// Write out how many publishes were dropped rather than queued.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_dropped_publishes(handle: *const Handle, out: *mut u64) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        unsafe { *out = handle.client.dropped_publishes() };
        XT_OK
    })
}

/// Begin mirroring published values into a WPILOG file at `path`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `path` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_log_to(handle: *const Handle, path: *const c_char) -> c_int {
    guard(|| {
        let (Some(handle), Some(path)) = (unsafe { handle.as_ref() }, to_str(path)) else {
            return XT_ERR_NULL;
        };
        match handle.client.log_to(path) {
            Ok(()) => XT_OK,
            Err(_) => XT_ERR_IO,
        }
    })
}

/// Begin logging onto the first writable removable drive that accepts the file,
/// writing the chosen path into `out_path` as a NUL-terminated string.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `filename` must point at a NUL-terminated UTF-8 string, and `out_path` must be
/// writable for `out_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_log_to_drive(
    handle: *const Handle,
    filename: *const c_char,
    out_path: *mut c_char,
    out_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(filename)) = (unsafe { handle.as_ref() }, to_str(filename)) else {
            return XT_ERR_NULL;
        };
        let Ok(path) = handle.client.log_to_drive(filename) else {
            return XT_ERR_IO;
        };
        if out_path.is_null() || out_len == 0 {
            return XT_OK;
        }
        let text = path.to_string_lossy();
        let bytes = text.as_bytes();
        let room = out_len - 1;
        let copied = bytes.len().min(room);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_path.cast::<u8>(), copied);
            *out_path.add(copied) = 0;
        }
        XT_OK
    })
}

/// Write out how many log records were dropped because the queue was full.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_log_dropped(handle: *const Handle, out: *mut u64) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        unsafe { *out = handle.client.log_dropped() };
        XT_OK
    })
}

/// Write out whether the log writer is still succeeding. `true` when logging was
/// never started.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_logging_healthy(handle: *const Handle, out: *mut bool) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        unsafe { *out = handle.client.logging_healthy() };
        XT_OK
    })
}

/// Publish a double to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_double(
    handle: *const Handle,
    channel: *const c_char,
    value: f64,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::Double(value));
        XT_OK
    })
}

/// Publish a float to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_float(
    handle: *const Handle,
    channel: *const c_char,
    value: f32,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::Float(value));
        XT_OK
    })
}

/// Publish a 32-bit integer to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_int32(
    handle: *const Handle,
    channel: *const c_char,
    value: i32,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::Int32(value));
        XT_OK
    })
}

/// Publish a 64-bit integer to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_int64(
    handle: *const Handle,
    channel: *const c_char,
    value: i64,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::Int64(value));
        XT_OK
    })
}

/// Publish a boolean to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_bool(
    handle: *const Handle,
    channel: *const c_char,
    value: bool,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::Bool(value));
        XT_OK
    })
}

/// Publish a string to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must point at a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_string(
    handle: *const Handle,
    channel: *const c_char,
    value: *const c_char,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let (Some(channel), Some(value)) = (to_str(channel), to_str(value)) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::String(value.to_string()));
        XT_OK
    })
}

/// Publish raw bytes to `channel`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must be readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_publish_bytes(
    handle: *const Handle,
    channel: *const c_char,
    value: *const u8,
    len: usize,
) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        if value.is_null() {
            return XT_ERR_NULL;
        }
        let bytes = unsafe { std::slice::from_raw_parts(value, len) };
        handle
            .client
            .send_message_public(channel, Kind::Bytes(bytes.to_vec()));
        XT_OK
    })
}

/// Read the bytes on `channel` into `out`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_bytes(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::Bytes(value)) => {
                copy_out(&value, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Publish a list of `(x, y)` coordinates to `channel`.
///
/// `values` is flat: `count` pairs, so `count * 2` doubles.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `values` must be readable for `count * 2` doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_coordinates(
    handle: *const Handle,
    channel: *const c_char,
    values: *const f64,
    count: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            values.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        if !count.is_multiple_of(2) {
            return XT_ERR_WRONG_TYPE;
        }
        let flat = unsafe { std::slice::from_raw_parts(values, count) };
        let pairs: Vec<(f64, f64)> = flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (pair[0], pair[1]))
            .collect();
        handle.client.send_coordinates(channel, &pairs);
        XT_OK
    })
}

/// Read the coordinate list on `channel` into `out`, flat — `x`, `y`, `x`, `y`.
///
/// `out_len` receives the number of doubles, which is twice the number of pairs.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable for `capacity` doubles, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_coordinates(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f64,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get_coordinates(channel) {
            Some(pairs) => {
                let flat: Vec<f64> = pairs.iter().flat_map(|(x, y)| [*x, *y]).collect();
                copy_out(&flat, out, capacity, out_len);
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Publish a bezier path to `channel`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must be readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_bezier_curves(
    handle: *const Handle,
    channel: *const c_char,
    encoded: *const u8,
    encoded_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            encoded.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let bytes = unsafe { std::slice::from_raw_parts(encoded, encoded_len) };
        let Ok(curves) = BezierCurves::decode(bytes) else {
            return XT_ERR_WRONG_TYPE;
        };
        handle.client.send_bezier_curves(channel, curves);
        XT_OK
    })
}

/// Read the bezier path on `channel` into `out`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_bezier_curves(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get_bezier_curves(channel) {
            Some(curves) => {
                copy_out(&curves.encode_to_vec(), out, capacity, out_len);
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Publish one bezier curve to `channel`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must be readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_bezier_curve(
    handle: *const Handle,
    channel: *const c_char,
    encoded: *const u8,
    encoded_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            encoded.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let bytes = unsafe { std::slice::from_raw_parts(encoded, encoded_len) };
        let Ok(curve) = BezierCurve::decode(bytes) else {
            return XT_ERR_WRONG_TYPE;
        };
        handle.client.send_bezier_curve(channel, curve);
        XT_OK
    })
}

/// Read the bezier curve on `channel` into `out`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_bezier_curve(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get_bezier_curve(channel) {
            Some(curve) => {
                copy_out(&curve.encode_to_vec(), out, capacity, out_len);
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Publish a list of bezier paths to `channel`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must be readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_bezier_curves_list(
    handle: *const Handle,
    channel: *const c_char,
    encoded: *const u8,
    encoded_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            encoded.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let bytes = unsafe { std::slice::from_raw_parts(encoded, encoded_len) };
        let Ok(list) = BezierCurvesList::decode(bytes) else {
            return XT_ERR_WRONG_TYPE;
        };
        handle.client.send_bezier_curves_list(channel, list.values);
        XT_OK
    })
}

/// Read the list of bezier paths on `channel` into `out`, as encoded protobuf.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_bezier_curves_list(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get_bezier_curves_list(channel) {
            Some(values) => {
                copy_out(
                    &BezierCurvesList { values }.encode_to_vec(),
                    out,
                    capacity,
                    out_len,
                );
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Publish a value already encoded in TARWYN' own byte layout.
///
/// `tarwyn_type` is TARWYN' type tag. An unrecognised tag is published as raw
/// bytes. Returns [`XT_ERR_WRONG_TYPE`], publishing nothing, only when a
/// recognised tag comes with bytes that are not a valid value of that type.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `value` must be readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_typed_bytes(
    handle: *const Handle,
    channel: *const c_char,
    tarwyn_type: c_int,
    value: *const u8,
    len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) =
            (unsafe { handle.as_ref() }, to_str(channel), value.is_null())
        else {
            return XT_ERR_NULL;
        };
        let bytes = unsafe { std::slice::from_raw_parts(value, len) };
        if handle.client.send_typed_bytes(channel, tarwyn_type, bytes) {
            XT_OK
        } else {
            XT_ERR_WRONG_TYPE
        }
    })
}

/// Delete `channel`, writing out how many were removed. Pass `""` to delete all.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_delete(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u32,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let deleted = handle.client.delete(channel);
        if !out.is_null() {
            unsafe { *out = deleted };
        }
        XT_OK
    })
}

/// Write the channel names beginning with `prefix` into `out`, packed.
///
/// Pass `""` for all of them. See the module docs for the packed layout.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `prefix` must point at a NUL-terminated UTF-8 string, `out` must be null or
/// writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_tables(
    handle: *const Handle,
    prefix: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(prefix)) = (unsafe { handle.as_ref() }, to_str(prefix)) else {
            return XT_ERR_NULL;
        };
        let channels = handle.client.tables(prefix);
        let buffer = encode_packed(channels.iter().map(|channel| channel.as_bytes()));
        copy_out(&buffer, out, capacity, out_len);
        XT_OK
    })
}

/// Write out the round-trip time to the server, in nanoseconds.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out_nanos` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_ping(handle: *const Handle, out_nanos: *mut u64) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out_nanos.is_null()) else {
            return XT_ERR_NULL;
        };
        match handle.client.ping() {
            Some(elapsed) => {
                unsafe { *out_nanos = elapsed.as_nanos() as u64 };
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Write the server's counters into `out`, and its version into `version` as a
/// NUL-terminated string.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out` must be writable for `capacity` values, and `version` null or writable
/// for `version_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_statistics(
    handle: *const Handle,
    out: *mut u64,
    capacity: usize,
    version: *mut c_char,
    version_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(statistics) = handle.client.statistics() else {
            return XT_ERR_NO_VALUE;
        };
        let fields = [
            statistics.channels,
            statistics.values,
            statistics.telemetry_subscribers,
            statistics.uptime_seconds,
        ];
        copy_out(&fields, out, capacity, std::ptr::null_mut());
        if !version.is_null() && version_len > 0 {
            let bytes = statistics.version.as_bytes();
            let copied = bytes.len().min(version_len - 1);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), version.cast::<u8>(), copied);
                *version.add(copied) = 0;
            }
        }
        XT_OK
    })
}

/// Write the channels beginning with `prefix` into `out` as a NUL-terminated JSON
/// document.
///
/// `out_len` receives the length including the terminator, so a null `out` sizes
/// the buffer.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `prefix` must point at a NUL-terminated UTF-8 string, `out` must be null or
/// writable for `capacity` bytes, and `out_len` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_raw_json(
    handle: *const Handle,
    prefix: *const c_char,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(prefix)) = (unsafe { handle.as_ref() }, to_str(prefix)) else {
            return XT_ERR_NULL;
        };
        let json = handle.client.raw_json(prefix);
        let bytes = json.as_bytes();
        if !out_len.is_null() {
            unsafe { *out_len = bytes.len() + 1 };
        }
        if !out.is_null() && capacity > 0 {
            let copied = bytes.len().min(capacity - 1);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), copied);
                *out.add(copied) = 0;
            }
        }
        XT_OK
    })
}

/// Subscribe to `channel`, delivering payloads into a ring the caller reads directly.
///
/// Writes the subscription id into `out_id`. `records` must be non-zero and
/// `record_bytes` greater than 8, since each slot carries an 8-byte length ahead
/// of its payload. Read the bytes through [`xt_ring_base`], bounded by
/// [`xt_ring_write_index`].
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `channel` must point at a NUL-terminated UTF-8 string.
/// `out_id` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_subscribe_ring(
    handle: *mut Handle,
    channel: *const c_char,
    records: usize,
    record_bytes: usize,
    out_id: *mut u64,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out_id.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        if records == 0 || record_bytes <= 8 {
            return XT_ERR_NULL;
        }

        let ring = Arc::new(Ring::new(records, record_bytes));
        let sink = Arc::clone(&ring);
        let unsubscribe = handle.client.subscribe(channel, move |value| {
            if let Kind::Bytes(bytes) = value {
                sink.push(bytes);
            }
        });

        let id = handle.next_id.fetch_add(1, Ordering::Relaxed);
        let (Ok(mut subscriptions), Ok(mut rings)) =
            (handle.subscriptions.lock(), handle.rings.lock())
        else {
            return XT_ERR_NULL;
        };
        subscriptions.insert(id, Box::new(unsubscribe));
        rings.insert(id, ring);
        unsafe { *out_id = id };
        XT_OK
    })
}

/// Cancel a subscription and release its ring, invalidating any pointer
/// [`xt_ring_base`] returned for it.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// No ring pointer for `id` may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_unsubscribe(handle: *mut Handle, id: u64) -> c_int {
    guard(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return XT_ERR_NULL;
        };
        let (Ok(mut subscriptions), Ok(mut rings)) =
            (handle.subscriptions.lock(), handle.rings.lock())
        else {
            return XT_ERR_NULL;
        };
        rings.remove(&id);
        match subscriptions.remove(&id) {
            Some(unsubscribe) => {
                unsubscribe();
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// The base address of a subscription's ring, or null if `id` is unknown.
///
/// Valid until the subscription is cancelled or the client freed. Slot `n` starts
/// at `(n % records) * record_bytes` and begins with its payload length as a
/// little-endian `u64`.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// The returned pointer is valid for `records * record_bytes` bytes, and only
/// until [`xt_unsubscribe`] or [`xt_client_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_ring_base(handle: *const Handle, id: u64) -> *mut c_void {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let handle = unsafe { handle.as_ref() }?;
        let rings = handle.rings.lock().ok()?;
        let ring = rings.get(&id)?;
        let mut slots = ring.slots.lock().ok()?;
        Some(slots.as_mut_ptr() as *mut c_void)
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

/// Push a payload into a subscription's ring as though it had arrived on the
/// channel.
///
/// The ring is otherwise fed only by the subscribe callback, which needs a
/// server publishing on the other end. This lets a caller drive it directly, so
/// the layout and the lap guard can be exercised from the reading side without a
/// server in the loop.
///
/// # Safety
///
/// `handle` must be a live handle from [`xt_client_new`], and `value` must be
/// readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_ring_push(
    handle: *const Handle,
    id: u64,
    value: *const u8,
    len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, value.is_null()) else {
            return XT_ERR_NULL;
        };
        let Ok(rings) = handle.rings.lock() else {
            return XT_ERR_NULL;
        };
        match rings.get(&id) {
            Some(ring) => {
                ring.push(unsafe { std::slice::from_raw_parts(value, len) });
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

/// Write out how many records have been pushed to a subscription's ring.
///
/// Loaded with `Acquire`, so every slot below the returned index is fully written.
/// An index more than `records` ahead of what the reader last saw means the writer
/// lapped it and those slots were overwritten.
///
/// # Safety
///
/// `handle` must be a live handle returned by [`xt_client_new`] and not yet
/// passed to [`xt_client_free`].
/// `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_ring_write_index(
    handle: *const Handle,
    id: u64,
    out: *mut u64,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Ok(rings) = handle.rings.lock() else {
            return XT_ERR_NULL;
        };
        match rings.get(&id) {
            Some(ring) => {
                unsafe { *out = ring.write_index.load(Ordering::Acquire) };
                XT_OK
            }
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn offline_client() -> *mut Handle {
        let host = CString::new("127.0.0.1").unwrap();
        unsafe { xt_client_new(host.as_ptr(), 47931, 47932, 47933, 150, 500) }
    }

    #[test]
    fn concurrent_pushes_do_not_share_a_slot() {
        use std::sync::Barrier;

        const THREADS: usize = 4;
        const EACH: usize = 256;

        let ring = Arc::new(Ring::new(THREADS * EACH, 32));
        let barrier = Arc::new(Barrier::new(THREADS));

        let workers: Vec<_> = (0..THREADS)
            .map(|thread| {
                let ring = Arc::clone(&ring);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    for step in 0..EACH {
                        let value = (thread * EACH + step) as u64;
                        ring.push(&value.to_le_bytes());
                    }
                })
            })
            .collect();

        for worker in workers {
            worker.join().unwrap();
        }

        assert_eq!(
            ring.write_index.load(Ordering::Acquire),
            (THREADS * EACH) as u64
        );

        let slots = ring.slots.lock().unwrap();
        let mut seen = vec![false; THREADS * EACH];
        for index in 0..THREADS * EACH {
            let start = index * 32;
            let len = u64::from_le_bytes(slots[start..start + 8].try_into().unwrap());
            assert_eq!(len, 8, "slot {index} was never written or was torn");
            let value =
                u64::from_le_bytes(slots[start + 8..start + 16].try_into().unwrap()) as usize;
            assert!(!seen[value], "value {value} landed in two slots");
            seen[value] = true;
        }
        assert!(seen.iter().all(|hit| *hit), "a push was lost");
    }

    #[test]
    fn null_pointers_are_rejected_not_dereferenced() {
        assert_eq!(
            unsafe { xt_client_start(std::ptr::null_mut()) },
            XT_ERR_NULL
        );
        assert_eq!(
            unsafe { xt_publish_double(std::ptr::null(), std::ptr::null(), 1.0) },
            XT_ERR_NULL
        );
        assert_eq!(
            unsafe { xt_unsubscribe(std::ptr::null_mut(), 1) },
            XT_ERR_NULL
        );
        unsafe { xt_client_free(std::ptr::null_mut()) };
    }

    #[test]
    fn client_lifecycle_and_publish() {
        let handle = offline_client();
        assert!(!handle.is_null());
        let channel = CString::new("bench").unwrap();
        assert_eq!(
            unsafe { xt_publish_double(handle, channel.as_ptr(), 1.5) },
            XT_OK
        );
        assert_eq!(
            unsafe { xt_publish_bool(handle, channel.as_ptr(), true) },
            XT_OK
        );
        let mut dropped = 0u64;
        assert_eq!(unsafe { xt_dropped_publishes(handle, &mut dropped) }, XT_OK);
        unsafe { xt_client_free(handle) };
    }

    #[test]
    fn get_reports_missing_value_rather_than_blocking() {
        let handle = offline_client();
        let channel = CString::new("absent").unwrap();
        let mut value = 0.0f64;
        assert_eq!(
            unsafe { crate::generated::xt_get_double(handle, channel.as_ptr(), &mut value) },
            XT_ERR_NO_VALUE
        );
        unsafe { xt_client_free(handle) };
    }

    #[test]
    fn ring_subscription_exposes_base_and_index() {
        let handle = offline_client();
        let channel = CString::new("ring").unwrap();
        let mut id = 0u64;
        assert_eq!(
            unsafe { xt_subscribe_ring(handle, channel.as_ptr(), 64, 128, &mut id) },
            XT_OK
        );
        assert!(id > 0);
        assert!(!unsafe { xt_ring_base(handle, id) }.is_null());

        let mut index = u64::MAX;
        assert_eq!(
            unsafe { xt_ring_write_index(handle, id, &mut index) },
            XT_OK
        );
        assert_eq!(index, 0);

        assert_eq!(unsafe { xt_unsubscribe(handle, id) }, XT_OK);
        assert_eq!(unsafe { xt_unsubscribe(handle, id) }, XT_ERR_NO_VALUE);
        unsafe { xt_client_free(handle) };
    }

    #[test]
    fn ring_records_advance_the_write_index() {
        let ring = Ring::new(4, 64);
        assert_eq!(ring.write_index.load(Ordering::Acquire), 0);
        ring.push(b"hello");
        assert_eq!(ring.write_index.load(Ordering::Acquire), 1);

        let slots = ring.slots.lock().unwrap();
        let len = u64::from_le_bytes(slots[0..8].try_into().unwrap()) as usize;
        assert_eq!(len, 5);
        assert_eq!(&slots[8..8 + len], b"hello");
    }

    #[test]
    fn ring_wraps_without_growing() {
        let ring = Ring::new(2, 64);
        for _ in 0..10 {
            ring.push(b"x");
        }
        assert_eq!(ring.write_index.load(Ordering::Acquire), 10);
        assert_eq!(ring.slots.lock().unwrap().len(), 2 * 64);
    }
}
