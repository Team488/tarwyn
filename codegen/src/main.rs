use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    scalar: Vec<Scalar>,
    #[allow(dead_code)]
    list: Vec<ListType>,
    packed: Vec<Packed>,
}

#[derive(Deserialize)]
struct Scalar {
    name: String,
    java: String,
    rust: String,
    #[allow(dead_code)]
    c: String,
    kind: String,
}

#[derive(Deserialize)]
struct ListType {
    name: String,
    java: String,
    element_java: String,
    element_rust: String,
    kind: String,
    field: String,
    packed: bool,
}

#[derive(Deserialize)]
struct Packed {
    name: String,
    java: String,
    fields: Vec<String>,
    wpilib: String,
    rotation: String,
    translation: usize,
}

fn boxed(java: &str) -> &str {
    match java {
        "int" => "Integer",
        "long" => "Long",
        "double" => "Double",
        "float" => "Float",
        "boolean" => "Boolean",
        other => other,
    }
}

fn layout(java: &str) -> String {
    format!("ValueLayout.JAVA_{}", java.to_uppercase())
}

trait UpperFirst {
    fn to_uppercase_first(&self) -> String;
}

impl UpperFirst for String {
    fn to_uppercase_first(&self) -> String {
        let mut characters = self.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => String::new(),
        }
    }
}

fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn banner(tool: &str) -> String {
    format!("// Generated from clients/api.toml by {tool}. Do not edit.\n")
}

fn ffi_list(list: &ListType) -> String {
    let ListType {
        name, kind, field, ..
    } = list;

    if list.packed {
        let collect = if list.element_rust == "String" {
            "let Some(decoded) = items\n            .into_iter()\n            .map(|item| String::from_utf8(item).ok())\n            .collect::<Option<Vec<_>>>()"
        } else {
            "let Some(decoded) = Some(items)"
        };
        let encode = if list.element_rust == "String" {
            "list.values.iter().map(|value| value.as_bytes())"
        } else {
            "list.values.iter().map(|value| value.as_slice())"
        };

        return format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_{name}(
    handle: *const Handle,
    channel: *const c_char,
    packed: *const u8,
    packed_len: usize,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel), false) =
            (unsafe {{ handle.as_ref() }}, to_str(channel), packed.is_null())
        else {{
            return XT_ERR_NULL;
        }};
        let buffer = unsafe {{ std::slice::from_raw_parts(packed, packed_len) }};
        let Some(items) = decode_packed(buffer) else {{
            return XT_ERR_WRONG_TYPE;
        }};
        {collect} else {{
            return XT_ERR_UTF8;
        }};
        handle
            .client
            .send_message_public(channel, Kind::{kind}({kind} {{ {field}: decoded }}));
        XT_OK
    }})
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_{name}(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, to_str(channel)) else {{
            return XT_ERR_NULL;
        }};
        match handle.client.get(channel) {{
            Some(Kind::{kind}(list)) => {{
                let buffer = encode_packed({encode});
                copy_out(&buffer, out, capacity, out_len);
                XT_OK
            }}
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }}
    }})
}}

"#
        );
    }

    let element = &list.element_rust;
    format!(
        r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_put_{name}(
    handle: *const Handle,
    channel: *const c_char,
    values: *const {element},
    count: usize,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel), false) =
            (unsafe {{ handle.as_ref() }}, to_str(channel), values.is_null())
        else {{
            return XT_ERR_NULL;
        }};
        let values = unsafe {{ std::slice::from_raw_parts(values, count) }};
        handle.client.send_message_public(
            channel,
            Kind::{kind}({kind} {{
                {field}: values.to_vec(),
            }}),
        );
        XT_OK
    }})
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_get_{name}(
    handle: *const Handle,
    channel: *const c_char,
    out: *mut {element},
    capacity: usize,
    out_len: *mut usize,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, to_str(channel)) else {{
            return XT_ERR_NULL;
        }};
        match handle.client.get(channel) {{
            Some(Kind::{kind}(list)) => {{
                copy_out(&list.{field}, out, capacity, out_len);
                XT_OK
            }}
            Some(_) => XT_ERR_WRONG_TYPE,
            None => XT_ERR_NO_VALUE,
        }}
    }})
}}

"#
    )
}

fn ffi(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str(
        "\nuse std::ffi::{c_char, c_int};\n\nuse tarwyn_protobuf::protobuf::supported_values::Kind;\nuse tarwyn_protobuf::protobuf::{\n    BoolList, BytesList, DoubleList, FloatList, IntegerList, LongList, StringList,\n};\n\n\
         use crate::{\n    \
             Handle, XT_ERR_NO_VALUE, XT_ERR_NULL, XT_ERR_UTF8, XT_ERR_WRONG_TYPE, XT_OK, copy_out,\n    decode_packed, encode_packed, guard, to_str,\n\
         };\n\n",
    );

    for scalar in &spec.scalar {
        let function = format!("xt_put_{}", scalar.name);
        let (parameter, build) = if scalar.name == "string" {
            (
                "value: *const c_char".to_string(),
                "let Some(value) = to_str(value) else {\n            return XT_ERR_UTF8;\n        };\n        \
                 let kind = Kind::String(value.to_string());"
                    .to_string(),
            )
        } else {
            (
                format!("value: {}", scalar.rust),
                format!("let kind = Kind::{}(value);", scalar.kind),
            )
        };

        let _ = write!(
            out,
            "#[unsafe(no_mangle)]\n\
             pub unsafe extern \"C\" fn {function}(\n    \
                 handle: *const Handle,\n    channel: *const c_char,\n    {parameter},\n\
             ) -> c_int {{\n    \
                 guard(|| {{\n        \
                     let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, to_str(channel)) else {{\n            \
                         return XT_ERR_NULL;\n        \
                     }};\n        \
                     {build}\n        \
                     handle.client.send_message_public(channel, kind);\n        \
                     XT_OK\n    \
                 }})\n\
             }}\n\n"
        );
    }

    for scalar in &spec.scalar {
        let function = format!("xt_get_{}", scalar.name);
        if scalar.name == "string" {
            let _ = write!(
                out,
                "#[unsafe(no_mangle)]\n\
                 pub unsafe extern \"C\" fn {function}(\n    \
                     handle: *const Handle,\n    channel: *const c_char,\n    \
                     out: *mut c_char,\n    out_len: usize,\n\
                 ) -> c_int {{\n    \
                     guard(|| {{\n        \
                         let (Some(handle), Some(channel), false) =\n            \
                             (unsafe {{ handle.as_ref() }}, to_str(channel), out.is_null())\n        \
                         else {{\n            return XT_ERR_NULL;\n        }};\n        \
                         if out_len == 0 {{\n            return XT_ERR_NULL;\n        }}\n        \
                         match handle.client.get(channel) {{\n            \
                             Some(Kind::String(value)) => {{\n                \
                                 let bytes = value.as_bytes();\n                \
                                 let copied = bytes.len().min(out_len - 1);\n                \
                                 unsafe {{\n                    \
                                     std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, copied);\n                    \
                                     *out.add(copied) = 0;\n                \
                                 }}\n                \
                                 XT_OK\n            \
                             }}\n            \
                             Some(_) => XT_ERR_WRONG_TYPE,\n            \
                             None => XT_ERR_NO_VALUE,\n        \
                         }}\n    \
                     }})\n\
                 }}\n\n"
            );
        } else {
            let _ = write!(
                out,
                "#[unsafe(no_mangle)]\n\
                 pub unsafe extern \"C\" fn {function}(\n    \
                     handle: *const Handle,\n    channel: *const c_char,\n    out: *mut {},\n\
                 ) -> c_int {{\n    \
                     guard(|| {{\n        \
                         let (Some(handle), false) = (unsafe {{ handle.as_ref() }}, out.is_null()) else {{\n            \
                             return XT_ERR_NULL;\n        \
                         }};\n        \
                         let Some(channel) = to_str(channel) else {{\n            \
                             return XT_ERR_UTF8;\n        \
                         }};\n        \
                         match handle.client.get(channel) {{\n            \
                             Some(Kind::{}(value)) => {{\n                \
                                 unsafe {{ *out = value }};\n                \
                                 XT_OK\n            \
                             }}\n            \
                             Some(_) => XT_ERR_WRONG_TYPE,\n            \
                             None => XT_ERR_NO_VALUE,\n        \
                         }}\n    \
                     }})\n\
                 }}\n\n",
                scalar.rust, scalar.kind
            );
        }
    }

    for scalar in &spec.scalar {
        let (parameters, build_expected, build_value) = if scalar.name == "string" {
            (
                "expected: *const c_char,\n    has_expected: bool,\n    value: *const c_char",
                "let expected = if has_expected {\n            let Some(expected) = to_str(expected) else {\n                return XT_ERR_UTF8;\n            };\n            Some(Kind::String(expected.to_string()))\n        } else {\n            None\n        };",
                "let Some(value) = to_str(value) else {\n            return XT_ERR_UTF8;\n        };\n        let value = Kind::String(value.to_string());",
            )
        } else {
            (
                "expected: PLACEHOLDER_TYPE,\n    has_expected: bool,\n    value: PLACEHOLDER_TYPE",
                "let expected = if has_expected {\n            Some(Kind::PLACEHOLDER_KIND(expected))\n        } else {\n            None\n        };",
                "let value = Kind::PLACEHOLDER_KIND(value);",
            )
        };
        let parameters = parameters.replace("PLACEHOLDER_TYPE", &scalar.rust);
        let build_expected = build_expected.replace("PLACEHOLDER_KIND", &scalar.kind);
        let build_value = build_value.replace("PLACEHOLDER_KIND", &scalar.kind);

        let _ = write!(
            out,
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn xt_compare_and_set_{}(
    handle: *const Handle,
    channel: *const c_char,
    {parameters},
    out_swapped: *mut bool,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, to_str(channel)) else {{
            return XT_ERR_NULL;
        }};
        {build_expected}
        {build_value}
        let swapped = handle.client.compare_and_set(channel, expected, value);
        if !out_swapped.is_null() {{
            unsafe {{ *out_swapped = swapped }};
        }}
        XT_OK
    }})
}}

"#,
            scalar.name
        );
    }

    for list in &spec.list {
        out.push_str(&ffi_list(list));
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let _ = write!(
            out,
            "#[unsafe(no_mangle)]\n\
             pub unsafe extern \"C\" fn xt_get_{}(\n    \
                 handle: *const Handle,\n    channel: *const c_char,\n    out: *mut f64,\n\
             ) -> c_int {{\n    \
                 guard(|| {{\n        \
                     let (Some(handle), Some(channel), false) =\n            \
                         (unsafe {{ handle.as_ref() }}, to_str(channel), out.is_null())\n        \
                     else {{\n            return XT_ERR_NULL;\n        }};\n        \
                     match handle.client.get(channel) {{\n            \
                         Some(Kind::Bytes(bytes)) if bytes.len() == {count} * 8 => {{\n                \
                             for index in 0..{count} {{\n                    \
                                 let mut field = [0u8; 8];\n                    \
                                 field.copy_from_slice(&bytes[index * 8..index * 8 + 8]);\n                    \
                                 unsafe {{ *out.add(index) = f64::from_le_bytes(field) }};\n                \
                             }}\n                \
                             XT_OK\n            \
                         }}\n            \
                         Some(_) => XT_ERR_WRONG_TYPE,\n            \
                         None => XT_ERR_NO_VALUE,\n        \
                     }}\n    \
                 }})\n\
             }}\n\n",
            packed.name
        );
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let _ = write!(
            out,
            "#[unsafe(no_mangle)]\n\
             pub unsafe extern \"C\" fn xt_put_{}(\n    \
                 handle: *const Handle,\n    channel: *const c_char,\n    values: *const f64,\n\
             ) -> c_int {{\n    \
                 guard(|| {{\n        \
                     let (Some(handle), Some(channel), false) =\n            \
                         (unsafe {{ handle.as_ref() }}, to_str(channel), values.is_null())\n        \
                     else {{\n            return XT_ERR_NULL;\n        }};\n        \
                     let fields = unsafe {{ std::slice::from_raw_parts(values, {count}) }};\n        \
                     let mut packed = Vec::with_capacity({count} * 8);\n        \
                     for field in fields {{\n            \
                         packed.extend_from_slice(&field.to_le_bytes());\n        \
                     }}\n        \
                     handle.client.send_message_public(channel, Kind::Bytes(packed));\n        \
                     XT_OK\n    \
                 }})\n\
             }}\n\n",
            packed.name
        );
    }

    out
}

fn java_list(list: &ListType) -> String {
    let ListType {
        name,
        java,
        element_java,
        ..
    } = list;

    if list.packed {
        let (encode, decode) = if list.element_java == "String" {
            (
                "byte[] item = values[index].getBytes(java.nio.charset.StandardCharsets.UTF_8);",
                "items[index] = new String(item, java.nio.charset.StandardCharsets.UTF_8);",
            )
        } else {
            ("byte[] item = values[index];", "items[index] = item;")
        };

        let allocate = if list.element_java == "byte[]" {
            "new byte[buffer.getInt()][]"
        } else {
            "new String[buffer.getInt()]"
        };

        return format!(
            r#"    public void put{java}(String channel, {element_java}[] values) {{
        int total = 4;
        byte[][] encoded = new byte[values.length][];
        for (int index = 0; index < values.length; index++) {{
            {encode}
            encoded[index] = item;
            total += 4 + item.length;
        }}
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer.allocate(total)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
        buffer.putInt(values.length);
        for (byte[] item : encoded) {{
            buffer.putInt(item.length);
            buffer.put(item);
        }}
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, buffer.array());
            check(xt_put_{name}(handle, channel(channel), body, (long) total), "put{java}");
        }}
    }}

    public {element_java}[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate(needed);
                check(xt_get_{name}(handle, channel(channel), out, needed, size), "get{java}");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }}
            java.nio.ByteBuffer buffer = out.asSlice(0, needed).asByteBuffer()
                .order(java.nio.ByteOrder.LITTLE_ENDIAN);
            {element_java}[] items = {allocate};
            for (int index = 0; index < items.length; index++) {{
                byte[] item = new byte[buffer.getInt()];
                buffer.get(item);
                {decode}
            }}
            return items;
        }}
    }}

"#
        );
    }

    let element_layout = layout(element_java);

    if list.element_java == "boolean" {
        return format!(
            r#"    public void put{java}(String channel, boolean[] values) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment body = call.allocate(ValueLayout.JAVA_BOOLEAN, values.length);
            for (int index = 0; index < values.length; index++) {{
                body.setAtIndex(ValueLayout.JAVA_BOOLEAN, index, values[index]);
            }}
            check(
                xt_put_{name}(handle, channel(channel), body, (long) values.length),
                "put{java}");
        }}
    }}

    public boolean[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN, capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate(ValueLayout.JAVA_BOOLEAN, needed);
                check(xt_get_{name}(handle, channel(channel), out, needed, size), "get{java}");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }}
            boolean[] items = new boolean[(int) needed];
            for (int index = 0; index < items.length; index++) {{
                items[index] = out.getAtIndex(ValueLayout.JAVA_BOOLEAN, index);
            }}
            return items;
        }}
    }}

"#
        );
    }

    format!(
        r#"    public void put{java}(String channel, {element_java}[] values) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment body = call.allocateFrom({element_layout}, values);
            check(
                xt_put_{name}(handle, channel(channel), body, (long) values.length),
                "put{java}");
        }}
    }}

    public {element_java}[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate({element_layout}, capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate({element_layout}, needed);
                check(xt_get_{name}(handle, channel(channel), out, needed, size), "get{java}");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }}
            return out.asSlice(0, needed * {element_layout}.byteSize()).toArray({element_layout});
        }}
    }}

"#
    )
}

fn java(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str(
        r#"
import static tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.concurrent.ConcurrentHashMap;

public abstract class TarwynApi {
    protected Arena arena;
    protected MemorySegment handle;

    private final ConcurrentHashMap<String, MemorySegment> channels = new ConcurrentHashMap<>();

    protected abstract void check(int code, String what);

    protected MemorySegment channel(String name) {
        return channels.computeIfAbsent(name, key -> arena.allocateFrom(key));
    }

"#,
    );

    for scalar in &spec.scalar {
        let name = format!("put{}", camel(&scalar.name.to_uppercase_first()));
        if scalar.name == "string" {
            let _ = write!(
                out,
                r#"    public void {name}(String channel, String value) {{
        try (Arena call = Arena.ofConfined()) {{
            check(xt_put_string(handle, channel(channel), call.allocateFrom(value)), "{name}");
        }}
    }}

"#
            );
        } else {
            let _ = write!(
                out,
                r#"    public void {name}(String channel, {} value) {{
        check(xt_put_{}(handle, channel(channel), value), "{name}");
    }}

"#,
                scalar.java, scalar.name
            );
        }
    }

    for scalar in &spec.scalar {
        let name = format!("get{}", camel(&scalar.name.to_uppercase_first()));
        if scalar.name == "string" {
            let _ = write!(
                out,
                r#"    public String {name}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment out = call.allocate(4096);
            int code = xt_get_string(handle, channel(channel), out, 4096);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "{name}");
            return out.getString(0);
        }}
    }}

"#
            );
        } else {
            let _ = write!(
                out,
                r#"    public {} {name}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment out = call.allocate({});
            int code = xt_get_{}(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "{name}");
            return out.get({}, 0);
        }}
    }}

"#,
                boxed(&scalar.java),
                layout(&scalar.java),
                scalar.name,
                layout(&scalar.java)
            );
        }
    }

    for scalar in &spec.scalar {
        let name = format!("compareAndSet{}", camel(&scalar.name.to_uppercase_first()));
        if scalar.name == "string" {
            let _ = write!(
                out,
                r#"    public boolean {name}(String channel, String expected, String value) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            MemorySegment previous = expected == null
                ? MemorySegment.NULL
                : call.allocateFrom(expected);
            check(
                xt_compare_and_set_string(handle, channel(channel), previous, expected != null,
                    call.allocateFrom(value), out),
                "{name}");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }}
    }}

"#
            );
        } else {
            let _ = write!(
                out,
                r#"    public boolean {name}(String channel, {} expected, {} value) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_{}(handle, channel(channel),
                    expected == null ? {} : expected, expected != null, value, out),
                "{name}");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }}
    }}

"#,
                boxed(&scalar.java),
                scalar.java,
                scalar.name,
                if scalar.java == "boolean" {
                    "false"
                } else {
                    "0"
                }
            );
        }
    }

    for list in &spec.list {
        out.push_str(&java_list(list));
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let arguments: Vec<String> = packed
            .fields
            .iter()
            .map(|field| format!("double {field}"))
            .collect();
        let name = format!("put{}", packed.java);
        let mut sets = String::new();
        for (index, field) in packed.fields.iter().enumerate() {
            let _ = writeln!(
                sets,
                "            values.setAtIndex(ValueLayout.JAVA_DOUBLE, {index}, {field});"
            );
        }
        let _ = write!(
            out,
            r#"    public void {name}(String channel, {}) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment values = call.allocate(ValueLayout.JAVA_DOUBLE, {count});
{sets}            check(xt_put_{}(handle, channel(channel), values), "{name}");
        }}
    }}

"#,
            arguments.join(", "),
            packed.name
        );
    }

    for packed in &spec.packed {
        let arguments: Vec<String> = packed
            .fields
            .iter()
            .map(|field| {
                if field == "rotation" {
                    "value.getRotation().getRadians()".to_string()
                } else if packed.fields.len() > 3
                    && matches!(field.as_str(), "roll" | "pitch" | "yaw")
                {
                    let accessor = match field.as_str() {
                        "roll" => "getX",
                        "pitch" => "getY",
                        _ => "getZ",
                    };
                    format!("value.getRotation().{accessor}()")
                } else {
                    format!("value.get{}()", field.to_uppercase())
                }
            })
            .collect();
        let _ = write!(
            out,
            r#"    public void put{}(String channel, {} value) {{
        put{}(channel, {});
    }}

"#,
            packed.java,
            packed.wpilib,
            packed.java,
            arguments.join(", ")
        );
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let translation: Vec<String> = (0..packed.translation)
            .map(|index| format!("fields[{index}]"))
            .collect();
        let rotation: Vec<String> = (packed.translation..count)
            .map(|index| format!("fields[{index}]"))
            .collect();
        let name = format!("get{}", packed.java);

        let _ = write!(
            out,
            r#"    public {} {name}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, {count});
            int code = xt_get_{}(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "{name}");
            double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
            return new {}({}, new {}({}));
        }}
    }}

"#,
            packed.wpilib,
            packed.name,
            packed.wpilib,
            translation.join(", "),
            packed.rotation,
            rotation.join(", ")
        );
    }

    out.push_str("}\n");
    out
}

fn python(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str("#![allow(clippy::too_many_arguments)]\n\nuse pyo3::prelude::*;\n\nuse tarwyn_protobuf::protobuf::supported_values::Kind;\nuse tarwyn_protobuf::protobuf::{\n    BoolList, BytesList, DoubleList, FloatList, IntegerList, LongList, StringList,\n};\n\nuse crate::PyTarwynClient;\n\n#[pymethods]\nimpl PyTarwynClient {\n");

    for scalar in &spec.scalar {
        let (parameter, build) = if scalar.name == "string" {
            (
                "value: &str".to_string(),
                "Kind::String(value.to_string())".to_string(),
            )
        } else {
            (
                format!("value: {}", scalar.rust),
                format!("Kind::{}(value)", scalar.kind),
            )
        };
        let _ = write!(
            out,
            "    fn put_{}(&self, python: Python<'_>, channel: &str, {parameter}) {{\n        \
                 python.detach(|| self.inner.send_message_public(channel, {build}));\n    }}\n\n",
            scalar.name
        );
    }

    for scalar in &spec.scalar {
        let returns = if scalar.name == "string" {
            "String".to_string()
        } else {
            scalar.rust.to_string()
        };
        let _ = write!(
            out,
            "    fn get_{}(&self, python: Python<'_>, channel: &str) -> Option<{returns}> {{\n        \
                 match python.detach(|| self.inner.get(channel)) {{\n            \
                     Some(Kind::{}(value)) => Some(value),\n            \
                     _ => None,\n        \
                 }}\n    \
             }}\n\n",
            scalar.name, scalar.kind
        );
    }

    for list in &spec.list {
        let element = if list.element_rust == "String" {
            "String".to_string()
        } else if list.element_rust == "Vec<u8>" {
            "Vec<u8>".to_string()
        } else {
            list.element_rust.clone()
        };
        let _ = write!(
            out,
            "    fn put_{}(&self, python: Python<'_>, channel: &str, items: Vec<{element}>) {{\n        \
                 python.detach(|| {{\n            \
                     self.inner.send_message_public(\n                \
                         channel,\n                \
                         Kind::{}({} {{ {}: items }}),\n            \
                     )\n        \
                 }});\n    \
             }}\n\n\
                 fn get_{}(&self, python: Python<'_>, channel: &str) -> Option<Vec<{element}>> {{\n        \
                 match python.detach(|| self.inner.get(channel)) {{\n            \
                     Some(Kind::{}(list)) => Some(list.{}),\n            \
                     _ => None,\n        \
                 }}\n    \
             }}\n\n",
            list.name, list.kind, list.kind, list.field, list.name, list.kind, list.field
        );
    }

    for scalar in &spec.scalar {
        let (parameter, expected_kind, value_kind) = if scalar.name == "string" {
            (
                "expected: Option<String>, value: &str".to_string(),
                "expected.map(Kind::String)".to_string(),
                "Kind::String(value.to_string())".to_string(),
            )
        } else {
            (
                format!("expected: Option<{}>, value: {}", scalar.rust, scalar.rust),
                format!("expected.map(Kind::{})", scalar.kind),
                format!("Kind::{}(value)", scalar.kind),
            )
        };
        let _ = write!(
            out,
            "    fn compare_and_set_{}(&self, python: Python<'_>, channel: &str, {parameter}) -> bool {{\n        \
                 python.detach(|| {{\n            \
                     self.inner\n                \
                         .compare_and_set(channel, {expected_kind}, {value_kind})\n        \
                 }})\n    \
             }}\n\n",
            scalar.name
        );
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let _ = write!(
            out,
            "    fn get_{}(&self, python: Python<'_>, channel: &str) -> Option<Vec<f64>> {{\n        \
                 let Some(Kind::Bytes(bytes)) = python.detach(|| self.inner.get(channel)) else {{\n            \
                     return None;\n        \
                 }};\n        \
                 if bytes.len() != {count} * 8 {{\n            \
                     return None;\n        \
                 }}\n        \
                 let (fields, _) = bytes.as_chunks::<8>();\n        \
                 Some(fields.iter().copied().map(f64::from_le_bytes).collect())\n    \
             }}\n\n",
            packed.name
        );
    }

    for packed in &spec.packed {
        let parameters: Vec<String> = packed
            .fields
            .iter()
            .map(|field| format!("{field}: f64"))
            .collect();
        let values: Vec<String> = packed.fields.clone();
        let _ = write!(
            out,
            "    fn put_{}(&self, python: Python<'_>, channel: &str, {}) {{\n        \
                 let fields = [{}];\n        \
                 let mut packed = Vec::with_capacity(fields.len() * 8);\n        \
                 for field in fields {{\n            \
                     packed.extend_from_slice(&field.to_le_bytes());\n        \
                 }}\n        \
                 python.detach(|| self.inner.send_message_public(channel, Kind::Bytes(packed)));\n    \
             }}\n\n",
            packed.name,
            parameters.join(", "),
            values.join(", ")
        );
    }

    out.push_str("}\n");
    out
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    println!("wrote {}", path.display());
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let spec: Spec = toml::from_str(&fs::read_to_string(root.join("clients/api.toml")).unwrap())
        .expect("clients/api.toml is not valid");

    write(&root.join("clients/ffi/src/generated.rs"), &ffi(&spec));
    write(
        &root.join("clients/java-client/src/TarwynApi.java"),
        &java(&spec),
    );
    write(
        &root.join("clients/python-client/src/generated.rs"),
        &python(&spec),
    );
}
