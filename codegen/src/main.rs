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
#[allow(dead_code)]
struct ListType {
    name: String,
    java: String,
    element: String,
    kind: String,
}

#[derive(Deserialize)]
struct Packed {
    name: String,
    java: String,
    fields: Vec<String>,
    wpilib: String,
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

fn ffi(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str(
        "\nuse std::ffi::{c_char, c_int};\n\nuse tarwyn_protobuf::protobuf::supported_values::Kind;\n\n\
         use crate::{Handle, XT_ERR_NULL, XT_ERR_UTF8, XT_OK, guard, to_str};\n\n",
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

fn java(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str(
        "\nimport static tarwyn.ffi.tarwyn_h.*;\n\n\
         import java.lang.foreign.Arena;\nimport java.lang.foreign.MemorySegment;\n\
         import java.lang.foreign.ValueLayout;\n\n\
         public abstract class TarwynApi {\n    \
             protected Arena arena;\n    protected MemorySegment handle;\n\n    \
             protected abstract void check(int code, String what);\n\n",
    );

    for scalar in &spec.scalar {
        let name = format!("put{}", camel(&scalar.name.to_uppercase_first()));
        let argument = if scalar.name == "string" {
            "arena.allocateFrom(value)"
        } else {
            "value"
        };
        let _ = write!(
            out,
            "    public void {name}(String channel, {} value) {{\n        \
                 check(xt_put_{}(handle, arena.allocateFrom(channel), {argument}), \"{name}\");\n    \
             }}\n\n",
            scalar.java, scalar.name
        );
    }

    for packed in &spec.packed {
        let count = packed.fields.len();
        let arguments: Vec<String> = packed
            .fields
            .iter()
            .map(|field| format!("double {field}"))
            .collect();
        let name = format!("put{}", packed.java);
        let _ = write!(
            out,
            "    public void {name}(String channel, {}) {{\n        \
                 MemorySegment values = arena.allocate(ValueLayout.JAVA_DOUBLE, {count});\n",
            arguments.join(", ")
        );
        for (index, field) in packed.fields.iter().enumerate() {
            let _ = writeln!(
                out,
                "        values.setAtIndex(ValueLayout.JAVA_DOUBLE, {index}, {field});"
            );
        }
        let _ = write!(
            out,
            "        check(xt_put_{}(handle, arena.allocateFrom(channel), values), \"{name}\");\n    }}\n\n",
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
        let _ = writeln!(
            out,
            "    public void put{}(String channel, {} value) {{\n        \
                 put{}(channel, {});\n    }}\n",
            packed.java,
            packed.wpilib,
            packed.java,
            arguments.join(", ")
        );
    }

    out.push_str("}\n");
    out
}

fn python(spec: &Spec) -> String {
    let mut out = banner("codegen");
    out.push_str("#![allow(clippy::too_many_arguments)]\n\nuse pyo3::prelude::*;\n\nuse tarwyn_protobuf::protobuf::supported_values::Kind;\n\nuse crate::PyTarwynClient;\n\n#[pymethods]\nimpl PyTarwynClient {\n");

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
