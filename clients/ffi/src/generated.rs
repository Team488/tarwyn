// Generated from clients/api.toml by codegen. Do not edit.

use std::ffi::{c_char, c_int};

use tarwyn_protobuf::protobuf::supported_values::Kind;
use tarwyn_protobuf::protobuf::{
    BoolList, BytesList, DoubleList, FloatList, IntegerList, LongList, StringList,
};

use crate::{
    Handle, XT_ERR_NO_VALUE, XT_ERR_NULL, XT_ERR_UTF8, XT_ERR_WRONG_TYPE, XT_OK, copy_out,
    decode_packed, encode_packed, guard, to_str,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_string(
    handle: *const Handle,
    channel: *const c_char,
    value: *const c_char,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let Some(value) = to_str(value) else {
            return XT_ERR_UTF8;
        };
        let kind = Kind::String(value.to_string());
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_integer(
    handle: *const Handle,
    channel: *const c_char,
    value: i32,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let kind = Kind::Int32(value);
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_long(
    handle: *const Handle,
    channel: *const c_char,
    value: i64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let kind = Kind::Int64(value);
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_double(
    handle: *const Handle,
    channel: *const c_char,
    value: f64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let kind = Kind::Double(value);
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_float(
    handle: *const Handle,
    channel: *const c_char,
    value: f32,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let kind = Kind::Float(value);
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_boolean(
    handle: *const Handle,
    channel: *const c_char,
    value: bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let kind = Kind::Bool(value);
        handle.client.send_message_public(channel, kind);
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_string(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut c_char,
    out_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) =
            (unsafe { handle.as_ref() }, to_str(channel), out.is_null())
        else {
            return XT_ERR_NULL;
        };
        if out_len == 0 {
            return XT_ERR_NULL;
        }
        match handle.client.get(channel) {
            Some(Kind::String(value)) => {
                let bytes = value.as_bytes();
                let copied = bytes.len().min(out_len - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), copied);
                    *out.add(copied) = 0;
                }
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_integer(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut i32,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        match handle.client.get(channel) {
            Some(Kind::Int32(value)) => {
                unsafe { *out = value };
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_long(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut i64,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        match handle.client.get(channel) {
            Some(Kind::Int64(value)) => {
                unsafe { *out = value };
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_double(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f64,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        match handle.client.get(channel) {
            Some(Kind::Double(value)) => {
                unsafe { *out = value };
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_float(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f32,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        match handle.client.get(channel) {
            Some(Kind::Float(value)) => {
                unsafe { *out = value };
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_boolean(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), false) = (unsafe { handle.as_ref() }, out.is_null()) else {
            return XT_ERR_NULL;
        };
        let Some(channel) = to_str(channel) else {
            return XT_ERR_UTF8;
        };
        match handle.client.get(channel) {
            Some(Kind::Bool(value)) => {
                unsafe { *out = value };
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_string(
    handle: *const Handle,
    channel: *const c_char,
    expected: *const c_char,
    has_expected: bool,
    value: *const c_char,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            let Some(expected) = to_str(expected) else {
                return XT_ERR_UTF8;
            };
            Some(Kind::String(expected.to_string()))
        } else {
            None
        };
        let Some(value) = to_str(value) else {
            return XT_ERR_UTF8;
        };
        let value = Kind::String(value.to_string());
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_integer(
    handle: *const Handle,
    channel: *const c_char,
    expected: i32,
    has_expected: bool,
    value: i32,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            Some(Kind::Int32(expected))
        } else {
            None
        };
        let value = Kind::Int32(value);
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_long(
    handle: *const Handle,
    channel: *const c_char,
    expected: i64,
    has_expected: bool,
    value: i64,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            Some(Kind::Int64(expected))
        } else {
            None
        };
        let value = Kind::Int64(value);
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_double(
    handle: *const Handle,
    channel: *const c_char,
    expected: f64,
    has_expected: bool,
    value: f64,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            Some(Kind::Double(expected))
        } else {
            None
        };
        let value = Kind::Double(value);
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_float(
    handle: *const Handle,
    channel: *const c_char,
    expected: f32,
    has_expected: bool,
    value: f32,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            Some(Kind::Float(expected))
        } else {
            None
        };
        let value = Kind::Float(value);
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_boolean(
    handle: *const Handle,
    channel: *const c_char,
    expected: bool,
    has_expected: bool,
    value: bool,
    out_swapped: *mut bool,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        let expected = if has_expected {
            Some(Kind::Bool(expected))
        } else {
            None
        };
        let value = Kind::Bool(value);
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {
            unsafe { *out_swapped = swapped };
        }
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_string_list(
    handle: *const Handle,
    channel: *const c_char,
    packed: *const u8,
    packed_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            packed.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let buffer = unsafe { std::slice::from_raw_parts(packed, packed_len) };
        let Some(items) = decode_packed(buffer) else {
            return XT_ERR_WRONG_TYPE;
        };
        let Some(decoded) = items
            .into_iter()
            .map(|item| String::from_utf8(item).ok())
            .collect::<Option<Vec<_>>>()
        else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::StringList(StringList { values: decoded }));
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_string_list(
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
            Some(Kind::StringList(list)) => {
                let buffer = encode_packed(list.values.iter().map(|value| value.as_bytes()));
                copy_out(&buffer, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_bytes_list(
    handle: *const Handle,
    channel: *const c_char,
    packed: *const u8,
    packed_len: usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            packed.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let buffer = unsafe { std::slice::from_raw_parts(packed, packed_len) };
        let Some(items) = decode_packed(buffer) else {
            return XT_ERR_WRONG_TYPE;
        };
        let Some(decoded) = Some(items) else {
            return XT_ERR_UTF8;
        };
        handle
            .client
            .send_message_public(channel, Kind::BytesList(BytesList { values: decoded }));
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_bytes_list(
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
            Some(Kind::BytesList(list)) => {
                let buffer = encode_packed(list.values.iter().map(|value| value.as_slice()));
                copy_out(&buffer, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_double_list(
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
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        handle.client.send_message_public(
            channel,
            Kind::DoubleList(DoubleList {
                values: values.to_vec(),
            }),
        );
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_double_list(
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
        match handle.client.get(channel) {
            Some(Kind::DoubleList(list)) => {
                copy_out(&list.values, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_float_list(
    handle: *const Handle,
    channel: *const c_char,
    values: *const f32,
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
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        handle.client.send_message_public(
            channel,
            Kind::FloatList(FloatList {
                values: values.to_vec(),
            }),
        );
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_float_list(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f32,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::FloatList(list)) => {
                copy_out(&list.values, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_integer_list(
    handle: *const Handle,
    channel: *const c_char,
    values: *const i32,
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
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        handle.client.send_message_public(
            channel,
            Kind::IntegerList(IntegerList {
                values: values.to_vec(),
            }),
        );
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_integer_list(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut i32,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::IntegerList(list)) => {
                copy_out(&list.values, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_long_list(
    handle: *const Handle,
    channel: *const c_char,
    values: *const i64,
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
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        handle.client.send_message_public(
            channel,
            Kind::LongList(LongList {
                values: values.to_vec(),
            }),
        );
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_long_list(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut i64,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::LongList(list)) => {
                copy_out(&list.values, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_boolean_list(
    handle: *const Handle,
    channel: *const c_char,
    values: *const bool,
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
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        handle.client.send_message_public(
            channel,
            Kind::BoolList(BoolList {
                values: values.to_vec(),
            }),
        );
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_boolean_list(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut bool,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel)) = (unsafe { handle.as_ref() }, to_str(channel)) else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::BoolList(list)) => {
                copy_out(&list.values, out, capacity, out_len);
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_pose2d(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) =
            (unsafe { handle.as_ref() }, to_str(channel), out.is_null())
        else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::Bytes(bytes)) if bytes.len() == 3 * 8 => {
                for index in 0..3 {
                    let mut field = [0u8; 8];
                    field.copy_from_slice(&bytes[index * 8..index * 8 + 8]);
                    unsafe { *out.add(index) = f64::from_le_bytes(field) };
                }
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_pose3d(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut f64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) =
            (unsafe { handle.as_ref() }, to_str(channel), out.is_null())
        else {
            return XT_ERR_NULL;
        };
        match handle.client.get(channel) {
            Some(Kind::Bytes(bytes)) if bytes.len() == 6 * 8 => {
                for index in 0..6 {
                    let mut field = [0u8; 8];
                    field.copy_from_slice(&bytes[index * 8..index * 8 + 8]);
                    unsafe { *out.add(index) = f64::from_le_bytes(field) };
                }
                XT_OK
            }
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_pose2d(
    handle: *const Handle,
    channel: *const c_char,
    values: *const f64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            values.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let fields = unsafe { std::slice::from_raw_parts(values, 3) };
        let mut packed = Vec::with_capacity(3 * 8);
        for field in fields {
            packed.extend_from_slice(&field.to_le_bytes());
        }
        handle
            .client
            .send_message_public(channel, Kind::Bytes(packed));
        XT_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_pose3d(
    handle: *const Handle,
    channel: *const c_char,
    values: *const f64,
) -> c_int {
    guard(|| {
        let (Some(handle), Some(channel), false) = (
            unsafe { handle.as_ref() },
            to_str(channel),
            values.is_null(),
        ) else {
            return XT_ERR_NULL;
        };
        let fields = unsafe { std::slice::from_raw_parts(values, 6) };
        let mut packed = Vec::with_capacity(6 * 8);
        for field in fields {
            packed.extend_from_slice(&field.to_le_bytes());
        }
        handle
            .client
            .send_message_public(channel, Kind::Bytes(packed));
        XT_OK
    })
}
