// Generated from clients/api.toml by codegen. Do not edit.

use std::ffi::{c_char, c_int};

use tarwyn_protobuf::protobuf::supported_values::Kind;

use crate::{
    Handle, XT_ERR_NO_VALUE, XT_ERR_NULL, XT_ERR_UTF8, XT_ERR_WRONG_TYPE, XT_OK, guard, to_str,
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
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, copied);
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
