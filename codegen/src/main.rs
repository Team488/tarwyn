//! Generates the client API surfaces from `clients/api.toml`.
//!
//! One spec produces the C ABI functions in `ffi/src/generated.rs`, the
//! Java methods in `clients/java/src/org/tarwyn/BaseTarwynClient.java`,
//! and the Python
//! methods in `clients/python/src/generated.rs`, so the three cannot drift
//! apart when a type is added. Run with `cargo run -p codegen`; CI checks the
//! output matches what is committed.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    scalar: Vec<Scalar>,
    list: Vec<ListType>,
    packed: Vec<Packed>,
}

#[derive(Deserialize)]
struct Scalar {
    name: String,
    java: String,
    rust: String,
    #[expect(
        dead_code,
        reason = "read by the Java and C++ emitters, not the Rust one"
    )]
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

fn article(noun: &str) -> &'static str {
    match noun.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

fn plural(java: &str) -> String {
    match java {
        "int" => "integers".to_string(),
        "byte[]" => "byte arrays".to_string(),
        "String" => "strings".to_string(),
        other => format!("{}s", other.to_lowercase()),
    }
}

fn phrase(spec: &Spec, suffix: &str) -> String {
    if let Some(scalar) = spec.scalar.iter().find(|scalar| scalar.name == suffix) {
        return format!("{} {}", article(&scalar.name), scalar.name);
    }
    if let Some(list) = spec.list.iter().find(|list| list.name == suffix) {
        return format!("a list of {}", plural(&list.element_java));
    }
    if let Some(packed) = spec.packed.iter().find(|packed| packed.name == suffix) {
        return format!("{} {}", article(&packed.java), packed.java);
    }
    format!("{} {suffix}", article(suffix))
}

const CPP_PREAMBLE: &str = r##"
#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include <tarwyn.h>

namespace tarwyn {

/// Thrown when a call fails for a reason that is not simply an absent value.
class TarwynError : public std::runtime_error {
 public:
    TarwynError(const std::string& what, int code)
        : std::runtime_error(what + " failed: " + Describe(code)), code_(code) {}

    /// The `XT_ERR_*` code the C ABI returned.
    [[nodiscard]] int code() const noexcept { return code_; }

    /// A short description of an `XT_ERR_*` code.
    static const char* Describe(int code) {
        switch (code) {
            case XT_ERR_NULL: return "a required argument was null or out of range";
            case XT_ERR_UTF8: return "a string was not valid UTF-8";
            case XT_ERR_NO_VALUE: return "the channel holds nothing";
            case XT_ERR_WRONG_TYPE: return "the channel holds another type";
            case XT_ERR_PANIC: return "the native library panicked";
            case XT_ERR_IO: return "a filesystem operation failed";
            default: return "unknown error";
        }
    }

 private:
    int code_;
};

namespace detail {

/// An absent value is not a failure; a caller sees `std::nullopt` instead.
inline bool Absent(int code) {
    return code == XT_ERR_NO_VALUE || code == XT_ERR_WRONG_TYPE;
}

inline void Check(int code, const char* what) {
    if (code != XT_OK) {
        throw TarwynError(what, code);
    }
}

inline void AppendCount(std::vector<std::uint8_t>& out, std::size_t count) {
    const auto value = static_cast<std::uint32_t>(count);
    for (int shift = 0; shift < 32; shift += 8) {
        out.push_back(static_cast<std::uint8_t>((value >> shift) & 0xFF));
    }
}

inline void AppendPacked(std::vector<std::uint8_t>& out, const char* data, std::size_t length) {
    AppendCount(out, length);
    out.insert(out.end(), data, data + length);
}

inline std::uint32_t ReadCount(const std::uint8_t*& cursor, const std::uint8_t* end,
                               const char* what) {
    if (static_cast<std::size_t>(end - cursor) < 4) {
        throw TarwynError(std::string(what) + " read past the end of a packed list",
                           XT_ERR_WRONG_TYPE);
    }
    std::uint32_t value = 0;
    for (int index = 0; index < 4; ++index) {
        value |= static_cast<std::uint32_t>(cursor[index]) << (index * 8);
    }
    cursor += 4;
    return value;
}

/// Sizes a variable-length read, then fills it. Returns false when the channel
/// holds nothing of that type.
template <typename Call>
bool ReadInto(std::vector<std::uint8_t>& buffer, Call call, const char* what) {
    std::uint64_t needed = 0;
    const int sized = call(nullptr, 0, &needed);
    if (Absent(sized)) {
        return false;
    }
    Check(sized, what);
    buffer.resize(static_cast<std::size_t>(needed));
    Check(call(buffer.data(), static_cast<std::uint32_t>(buffer.size()), &needed), what);
    buffer.resize(static_cast<std::size_t>(needed));
    return true;
}

/// The generated half of the client: every put, get and compare-and-set the API
/// spec defines.
///
/// Generated from `clients/api.toml` alongside the C ABI and the Java and Python
/// clients, so the four cannot drift apart when a type is added.
/// `tarwyn::Client` derives from this and supplies the rest.
class Generated {
 protected:
    Handle* handle_ = nullptr;

 public:
"##;

fn cpp(spec: &Spec) -> String {
    let mut out = String::new();
    out.push_str("// Generated from clients/api.toml by codegen. Do not edit.\n");
    out.push_str(CPP_PREAMBLE);

    for scalar in &spec.scalar {
        let name = &scalar.name;
        let method = upper_camel(name);
        let article = article(name);
        let value = cpp_scalar(&scalar.rust);

        if scalar.name == "string" {
            let _ = write!(
                out,
                r#"    /// Publishes a string to `channel`.
    void Put{method}(std::string_view channel, std::string_view value) {{
        const std::string name(channel);
        const std::string owned(value);
        detail::Check(xt_put_{name}(handle_, name.c_str(), owned.c_str()), "Put{method}");
    }}

    /// Reads a string from `channel`, or `std::nullopt` when the channel holds
    /// nothing of that type.
    [[nodiscard]] std::optional<std::string> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        std::uint64_t needed = 0;
        const int sized = xt_get_{name}(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {{
            return std::nullopt;
        }}
        detail::Check(sized, "Get{method}");
        std::string buffer(static_cast<std::size_t>(needed), '\0');
        detail::Check(xt_get_{name}(handle_, name.c_str(), buffer.data(),
                                    static_cast<std::uint32_t>(buffer.size()), &needed),
                      "Get{method}");
        buffer.resize(std::strlen(buffer.c_str()));
        return buffer;
    }}

"#
            );
        } else {
            let _ = write!(
                out,
                r#"    /// Publishes {article} {name} to `channel`.
    void Put{method}(std::string_view channel, {value} value) {{
        const std::string name(channel);
        detail::Check(xt_put_{name}(handle_, name.c_str(), value), "Put{method}");
    }}

    /// Reads {article} {name} from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<{value}> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        {value} value{{}};
        const int code = xt_get_{name}(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {{
            return std::nullopt;
        }}
        detail::Check(code, "Get{method}");
        return value;
    }}

"#
            );
        }

        let (parameter, forward) = if scalar.name == "string" {
            ("std::string_view value", "owned.c_str()")
        } else {
            ("{value} value", "value")
        };
        let parameter = parameter.replace("{value}", value);
        let expected_type = if scalar.name == "string" {
            "std::optional<std::string_view>".to_string()
        } else {
            format!("std::optional<{value}>")
        };
        let expected_forward = if scalar.name == "string" {
            "expected ? std::string(*expected).c_str() : \"\""
        } else {
            "expected.value_or({})"
        };
        let zero = if value == "bool" {
            "false".to_string()
        } else {
            format!("static_cast<{value}>(0)")
        };
        let expected_forward = expected_forward.replace("{}", &zero);
        let owned = if scalar.name == "string" {
            "        const std::string owned(value);\n        const std::string expected_owned(expected.value_or(std::string_view{}));\n"
        } else {
            ""
        };
        let expected_forward = if scalar.name == "string" {
            "expected_owned.c_str()"
        } else {
            &expected_forward
        };

        let _ = write!(
            out,
            r#"    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSet{method}(std::string_view channel, {expected_type} expected,
                               {parameter}) {{
        const std::string name(channel);
{owned}        bool swapped = false;
        detail::Check(xt_compare_and_set_{name}(handle_, name.c_str(), {expected_forward},
                                                expected.has_value(), {forward}, &swapped),
                      "CompareAndSet{method}");
        return swapped;
    }}

"#
        );
    }

    for list in &spec.list {
        out.push_str(&if list.packed {
            cpp_packed_list(list)
        } else {
            cpp_flat_list(list)
        });
    }

    for packed in &spec.packed {
        let name = &packed.name;
        let method = upper_camel(name);
        let count = packed.fields.len();
        let parameters = packed
            .fields
            .iter()
            .map(|field| format!("double {field}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = packed.fields.join(", ");
        let _ = write!(
            out,
            r#"    /// Publishes a {java} to `channel`.
    void Put{method}(std::string_view channel, {parameters}) {{
        const std::string name(channel);
        const std::array<double, {count}> values = {{{arguments}}};
        detail::Check(xt_put_{name}(handle_, name.c_str(), values.data()), "Put{method}");
    }}

    /// Reads a {java} from `channel` as its {count} fields, or `std::nullopt` when
    /// the channel holds nothing of that type.
    [[nodiscard]] std::optional<std::array<double, {count}>> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        std::array<double, {count}> fields{{}};
        const int code = xt_get_{name}(handle_, name.c_str(), fields.data());
        if (detail::Absent(code)) {{
            return std::nullopt;
        }}
        detail::Check(code, "Get{method}");
        return fields;
    }}

"#,
            java = packed.java
        );
    }

    out.push_str("};\n\n}  // namespace detail\n}  // namespace tarwyn\n");
    out
}

/// The Rust spelling that makes cbindgen emit a C type whose width does not
/// change between LP64 and LLP64. `int64_t` resolves to `long` on Linux, which
/// jextract bakes into the bindings as a 4-byte layout on Windows.
fn ffi_scalar(rust: &str) -> &str {
    match rust {
        "i64" => "c_longlong",
        other => other,
    }
}

fn cpp_scalar(rust: &str) -> &str {
    match rust {
        "&str" => "std::string_view",
        "i32" => "std::int32_t",
        "i64" => "long long",
        "f64" => "double",
        "f32" => "float",
        other => other,
    }
}

fn cpp_element(rust: &str) -> &str {
    match rust {
        "String" => "std::string",
        "Vec<u8>" => "std::vector<std::uint8_t>",
        "i32" => "std::int32_t",
        "i64" => "std::int64_t",
        "f64" => "double",
        "f32" => "float",
        other => other,
    }
}

/// A variable-width list: framed into one buffer, the same packing the C ABI
/// documents.
fn cpp_packed_list(list: &ListType) -> String {
    let name = &list.name;
    let method = upper_camel(name);
    let element = cpp_element(&list.element_rust);
    let summary = plural(&list.element_java);
    let (append, read) = if list.element_rust == "String" {
        (
            "detail::AppendPacked(packed, item.data(), item.size());",
            "out.emplace_back(reinterpret_cast<const char*>(cursor), length);",
        )
    } else {
        (
            "detail::AppendPacked(packed, reinterpret_cast<const char*>(item.data()), item.size());",
            "out.emplace_back(cursor, cursor + length);",
        )
    };

    format!(
        r#"    /// Publishes a list of {summary} to `channel`.
    void Put{method}(std::string_view channel, const std::vector<{element}>& values) {{
        const std::string name(channel);
        std::vector<std::uint8_t> packed;
        detail::AppendCount(packed, values.size());
        for (const auto& item : values) {{
            {append}
        }}
        detail::Check(xt_put_{name}(handle_, name.c_str(), packed.data(),
                                    static_cast<std::uint32_t>(packed.size())),
                      "Put{method}");
    }}

    /// Reads a list of {summary} from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<{element}>> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        std::vector<std::uint8_t> buffer;
        if (!detail::ReadInto(buffer, [&](std::uint8_t* out, std::uint32_t capacity,
                                          std::uint64_t* needed) {{
                return xt_get_{name}(handle_, name.c_str(), out, capacity, needed);
            }},
            "Get{method}")) {{
            return std::nullopt;
        }}

        std::vector<{element}> out;
        const std::uint8_t* cursor = buffer.data();
        const std::uint8_t* end = cursor + buffer.size();
        const std::uint32_t count = detail::ReadCount(cursor, end, "Get{method}");
        out.reserve(count);
        for (std::uint32_t index = 0; index < count; ++index) {{
            const std::uint32_t length = detail::ReadCount(cursor, end, "Get{method}");
            if (static_cast<std::size_t>(end - cursor) < length) {{
                throw TarwynError("Get{method} read a truncated list", XT_ERR_WRONG_TYPE);
            }}
            {read}
            cursor += length;
        }}
        return out;
    }}

"#
    )
}

/// A fixed-width list: passed flat, with no framing.
fn cpp_flat_list(list: &ListType) -> String {
    let name = &list.name;
    let method = upper_camel(name);
    let element = cpp_element(&list.element_rust);
    let summary = plural(&list.element_java);

    // std::vector<bool> is a bitset, so its storage cannot be handed to a bool*.
    if list.element_rust == "bool" {
        return format!(
            r#"    /// Publishes a list of {summary} to `channel`.
    void Put{method}(std::string_view channel, const std::vector<bool>& values) {{
        const std::string name(channel);
        auto staging = std::make_unique<bool[]>(values.size());
        for (std::size_t index = 0; index < values.size(); ++index) {{
            staging[index] = values[index];
        }}
        detail::Check(xt_put_{name}(handle_, name.c_str(), staging.get(),
                                    static_cast<std::uint32_t>(values.size())),
                      "Put{method}");
    }}

    /// Reads a list of {summary} from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<bool>> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        std::uint64_t needed = 0;
        const int sized = xt_get_{name}(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {{
            return std::nullopt;
        }}
        detail::Check(sized, "Get{method}");
        auto staging = std::make_unique<bool[]>(static_cast<std::size_t>(needed));
        detail::Check(xt_get_{name}(handle_, name.c_str(), staging.get(),
                                    static_cast<std::uint32_t>(needed), &needed),
                      "Get{method}");
        return std::vector<bool>(staging.get(), staging.get() + needed);
    }}

"#
        );
    }

    format!(
        r#"    /// Publishes a list of {summary} to `channel`.
    void Put{method}(std::string_view channel, const std::vector<{element}>& values) {{
        const std::string name(channel);
        detail::Check(xt_put_{name}(handle_, name.c_str(), values.data(),
                                    static_cast<std::uint32_t>(values.size())),
                      "Put{method}");
    }}

    /// Reads a list of {summary} from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<{element}>> Get{method}(std::string_view channel) const {{
        const std::string name(channel);
        std::uint64_t needed = 0;
        const int sized = xt_get_{name}(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {{
            return std::nullopt;
        }}
        detail::Check(sized, "Get{method}");
        std::vector<{element}> out(static_cast<std::size_t>(needed));
        detail::Check(xt_get_{name}(handle_, name.c_str(), out.data(),
                                    static_cast<std::uint32_t>(out.size()), &needed),
                      "Get{method}");
        out.resize(static_cast<std::size_t>(needed));
        return out;
    }}

"#
    )
}

fn upper_camel(name: &str) -> String {
    name.split('_')
        .map(|part| part.to_string().to_uppercase_first())
        .collect()
}

/// The one sentence describing what an operation does, shared by all three clients
/// so their documentation cannot drift apart.
fn summary(spec: &Spec, snake: &str) -> Option<String> {
    if let Some(suffix) = snake.strip_prefix("compare_and_set_") {
        return Some(format!(
            "Set `channel` to `value` only if it currently holds `expected`, and report \
             whether it swapped. Takes {}.",
            phrase(spec, suffix)
        ));
    }
    if let Some(suffix) = snake.strip_prefix("put_") {
        return Some(format!("Publish {} to `channel`.", phrase(spec, suffix)));
    }
    if let Some(suffix) = snake.strip_prefix("get_") {
        return Some(format!("Read {} from `channel`.", phrase(spec, suffix)));
    }
    None
}

/// Every operation the spec produces, as its snake_case name.
fn operations(spec: &Spec) -> Vec<String> {
    let mut names = Vec::new();
    for scalar in &spec.scalar {
        for verb in ["put", "get", "compare_and_set"] {
            names.push(format!("{verb}_{}", scalar.name));
        }
    }
    for list in &spec.list {
        names.push(format!("put_{}", list.name));
        names.push(format!("get_{}", list.name));
    }
    for packed in &spec.packed {
        names.push(format!("put_{}", packed.name));
        names.push(format!("get_{}", packed.name));
    }
    names
}

/// Rewrites `source`, inserting a comment ahead of every declaration whose name
/// `name_of` recognises. Returns the source unchanged where it recognises nothing,
/// so an operation this does not know about is left undocumented rather than
/// documented wrongly.
fn annotate(
    source: &str,
    marker: &str,
    name_of: &dyn Fn(&str) -> Option<String>,
    comment: &dyn Fn(&str, &str, &str) -> String,
) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let mut rest = source;
    while let Some(at) = rest.find(marker) {
        let (before, indent) = {
            let head = &rest[..at];
            let line_start = head.rfind('\n').map(|index| index + 1).unwrap_or(0);
            (&rest[..line_start], head[line_start..].to_string())
        };
        out.push_str(before);
        rest = &rest[at..];

        let tail = &rest[marker.len()..];
        let declaration = tail.split('(').next().unwrap_or_default();
        let parameters = tail
            .split_once('(')
            .and_then(|(_, after)| after.split_once(')'))
            .map(|(inside, _)| inside)
            .unwrap_or_default();
        if let Some(name) = name_of(declaration) {
            out.push_str(&comment(&name, &indent, parameters));
        }

        out.push_str(&indent);
        out.push_str(marker);
        rest = &rest[marker.len()..];
    }
    out.push_str(rest);
    out
}

fn documented_ffi(spec: &Spec, source: &str) -> String {
    annotate(
        source,
        "pub unsafe extern \"C\" fn xt_",
        &|declaration| Some(declaration.to_string()),
        &|name, _, _| {
            let summary = summary(spec, name).unwrap_or_else(|| format!("`xt_{name}`."));
            format!(
                "/// {summary}\n\
                 ///\n\
                 /// # Safety\n\
                 ///\n\
                 /// `handle` must be a live handle from `xt_client_new`, `channel` must point at\n\
                 /// a NUL-terminated UTF-8 string, and every other pointer must be null or valid\n\
                 /// for the length it is passed with. See the crate docs for the out-buffer and\n\
                 /// packing conventions.\n"
            )
        },
    )
}

/// PyO3 turns a doc comment on a `#[pymethods]` function into the method's
/// `__doc__`, so the Python client's docstrings are these comments.
fn documented_python(spec: &Spec, source: &str) -> String {
    let known = operations(spec);
    annotate(
        source,
        "fn ",
        &|declaration| {
            let name = declaration.trim();
            known
                .iter()
                .any(|known| known == name)
                .then(|| name.to_string())
        },
        &|name, indent, _| match summary(spec, name) {
            Some(summary) => format!("{indent}/// {summary}\n"),
            None => String::new(),
        },
    )
}

/// Javadoc for the generated Java methods, from the same sentences.
fn documented_java(spec: &Spec, source: &str) -> String {
    let known: Vec<(String, String)> = operations(spec)
        .into_iter()
        .map(|snake| {
            let camel = match snake.split_once('_') {
                Some(("compare", rest)) => format!(
                    "compareAndSet{}",
                    upper_camel(rest.strip_prefix("and_set_").unwrap_or(rest),)
                ),
                Some((verb, rest)) => format!("{verb}{}", upper_camel(rest)),
                None => snake.clone(),
            };
            (camel, snake)
        })
        .collect();

    annotate(
        source,
        "public ",
        &|declaration| {
            let name = declaration.split_whitespace().last()?;
            known
                .iter()
                .find(|(camel, _)| camel == name)
                .map(|(_, snake)| snake.clone())
        },
        &|name, indent, parameters| {
            let Some(summary) = summary(spec, name) else {
                return String::new();
            };
            let summary = summary.replace('`', "{@code PLACEHOLDER}");
            let mut text = String::new();
            let mut open = true;
            for part in summary.split("{@code PLACEHOLDER}") {
                text.push_str(part);
                if open {
                    text.push_str("{@code ");
                } else {
                    text.push('}');
                }
                open = !open;
            }
            let text = text
                .strip_suffix("{@code ")
                .map(|trimmed| trimmed.to_string())
                .unwrap_or(text);

            let mut doc = format!("{indent}/**\n{indent} * {text}\n{indent} *\n");
            for parameter in parameters.split(',') {
                let Some(identifier) = parameter.split_whitespace().last() else {
                    continue;
                };
                let describes = match identifier {
                    "channel" if name.starts_with("get_") => "the channel to read",
                    "channel" if name.starts_with("compare_and_set_") => "the channel to swap",
                    "channel" => "the channel to publish to",
                    "expected" => "the value the channel must currently hold",
                    _ => "the value",
                };
                let _ = writeln!(doc, "{indent} * @param {identifier} {describes}");
            }
            let returns = if name.starts_with("get_") {
                Some("the value, or null when the channel is unset")
            } else if name.starts_with("compare_and_set_") {
                Some("whether the swap happened")
            } else {
                None
            };
            if let Some(returns) = returns {
                let _ = writeln!(doc, "{indent} * @return {returns}");
            }
            let _ = writeln!(doc, "{indent} */");
            doc
        },
    )
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
    packed_len: u32,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel), false) =
            (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}, packed.is_null())
        else {{
            return XT_ERR_NULL;
        }};
        let buffer = unsafe {{ std::slice::from_raw_parts(packed, packed_len as usize) }};
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
    capacity: u32,
    out_len: *mut u64,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}) else {{
            return XT_ERR_NULL;
        }};
        match handle.client.get(channel) {{
            Some(Kind::{kind}(list)) => {{
                let buffer = encode_packed({encode});
                unsafe {{ copy_out(&buffer, out, capacity, out_len) }};
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
    count: u32,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel), false) =
            (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}, values.is_null())
        else {{
            return XT_ERR_NULL;
        }};
        let values = unsafe {{ std::slice::from_raw_parts(values, count as usize) }};
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
    capacity: u32,
    out_len: *mut u64,
) -> c_int {{
    guard(|| {{
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}) else {{
            return XT_ERR_NULL;
        }};
        match handle.client.get(channel) {{
            Some(Kind::{kind}(list)) => {{
                unsafe {{ copy_out(&list.{field}, out, capacity, out_len) }};
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
        "\nuse std::ffi::{c_char, c_int, c_longlong};\n\nuse tarwyn_protobuf::protobuf::supported_values::Kind;\nuse tarwyn_protobuf::protobuf::{\n    BoolList, BytesList, DoubleList, FloatList, IntegerList, LongList, StringList,\n};\n\n\
         use crate::{\n    \
             Handle, XT_ERR_NO_VALUE, XT_ERR_NULL, XT_ERR_UTF8, XT_ERR_WRONG_TYPE, XT_OK, copy_out,\n    decode_packed, encode_packed, guard, to_str,\n\
         };\n\n",
    );

    for scalar in &spec.scalar {
        let function = format!("xt_put_{}", scalar.name);
        let (parameter, build) = if scalar.name == "string" {
            (
                "value: *const c_char".to_string(),
                "let Some(value) = (unsafe { to_str(value) }) else {\n            return XT_ERR_UTF8;\n        };\n        \
                 let kind = Kind::String(value.to_string());"
                    .to_string(),
            )
        } else {
            (
                format!("value: {}", ffi_scalar(&scalar.rust)),
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
                     let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}) else {{\n            \
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
                     out: *mut c_char,\n    capacity: u32,\n    out_len: *mut u64,\n\
                 ) -> c_int {{\n    \
                     guard(|| {{\n        \
                         let (Some(handle), Some(channel)) =\n            \
                             (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }})\n        \
                         else {{\n            return XT_ERR_NULL;\n        }};\n        \
                         match handle.client.get(channel) {{\n            \
                             Some(Kind::String(value)) => {{\n                \
                                 let bytes = value.as_bytes();\n                \
                                 if !out_len.is_null() {{\n                    \
                                     unsafe {{ *out_len = bytes.len() as u64 + 1 }};\n                \
                                 }}\n                \
                                 if !out.is_null() && capacity > 0 {{\n                    \
                                     let copied = bytes.len().min(capacity as usize - 1);\n                    \
                                     unsafe {{\n                        \
                                         std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), copied);\n                        \
                                         *out.add(copied) = 0;\n                    \
                                     }}\n                \
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
                         let Some(channel) = (unsafe {{ to_str(channel) }}) else {{\n            \
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
                ffi_scalar(&scalar.rust),
                scalar.kind
            );
        }
    }

    for scalar in &spec.scalar {
        let (parameters, build_expected, build_value) = if scalar.name == "string" {
            (
                "expected: *const c_char,\n    has_expected: bool,\n    value: *const c_char",
                "let expected = if has_expected {\n            let Some(expected) = (unsafe { to_str(expected) }) else {\n                return XT_ERR_UTF8;\n            };\n            Some(Kind::String(expected.to_string()))\n        } else {\n            None\n        };",
                "let Some(value) = (unsafe { to_str(value) }) else {\n            return XT_ERR_UTF8;\n        };\n        let value = Kind::String(value.to_string());",
            )
        } else {
            (
                "expected: PLACEHOLDER_TYPE,\n    has_expected: bool,\n    value: PLACEHOLDER_TYPE",
                "let expected = if has_expected {\n            Some(Kind::PLACEHOLDER_KIND(expected))\n        } else {\n            None\n        };",
                "let value = Kind::PLACEHOLDER_KIND(value);",
            )
        };
        let parameters = parameters.replace("PLACEHOLDER_TYPE", ffi_scalar(&scalar.rust));
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
        let (Some(handle), Some(channel)) = (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}) else {{
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
                         (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}, out.is_null())\n        \
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
                         (unsafe {{ handle.as_ref() }}, unsafe {{ to_str(channel) }}, values.is_null())\n        \
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
            check(xt_put_{name}(handle, channel(channel), body, total), "put{java}");
        }}
    }}

    public {element_java}[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate(needed);
                check(xt_get_{name}(handle, channel(channel), out, (int) needed, size), "get{java}");
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
                xt_put_{name}(handle, channel(channel), body, values.length),
                "put{java}");
        }}
    }}

    public boolean[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN, capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate(ValueLayout.JAVA_BOOLEAN, needed);
                check(xt_get_{name}(handle, channel(channel), out, (int) needed, size), "get{java}");
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
                xt_put_{name}(handle, channel(channel), body, values.length),
                "put{java}");
        }}
    }}

    public {element_java}[] get{java}(String channel) {{
        try (Arena call = Arena.ofConfined()) {{
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate({element_layout}, capacity);
            int code = xt_get_{name}(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "get{java}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {{
                out = call.allocate({element_layout}, needed);
                check(xt_get_{name}(handle, channel(channel), out, (int) needed, size), "get{java}");
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
package org.tarwyn;

import static org.tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The generated half of the Java client: every {@code put}, {@code get} and
 * {@code compareAndSet} the API spec defines.
 *
 * Generated from {@code clients/api.toml} alongside the C ABI and the Python
 * methods, so the three clients cannot drift apart when a type is added.
 * {@code TarwynClient} extends this and supplies the rest.
 */
public abstract class BaseTarwynClient {
    /** Backs the client for its whole lifetime; holds the cached channel names. */
    protected Arena arena;
    /** The native client, from {@code xt_client_new}. */
    protected MemorySegment handle;

    private final ConcurrentHashMap<String, MemorySegment> channels = new ConcurrentHashMap<>();

    /** For subclasses only. */
    protected BaseTarwynClient() {}

    /**
     * Turn a non-zero status from the native library into an exception.
     *
     * @param code the status returned by the call
     * @param what the operation that returned it, for the message
     */
    protected abstract void check(int code, String what);

    /**
     * The native string for a channel name, allocated once and reused.
     *
     * Every call would otherwise allocate into {@link #arena}, which reclaims
     * nothing until the client closes.
     *
     * @param name the channel name
     * @return the NUL-terminated native string
     */
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
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int code = xt_get_string(handle, channel(channel), MemorySegment.NULL, 0, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {{
                return null;
            }}
            check(code, "{name}");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            MemorySegment out = call.allocate(needed);
            check(xt_get_string(handle, channel(channel), out, (int) needed, size), "{name}");
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

    write(
        &root.join("ffi/src/generated.rs"),
        &documented_ffi(&spec, &ffi(&spec)),
    );
    write(
        &root.join("clients/java/src/org/tarwyn/BaseTarwynClient.java"),
        &documented_java(&spec, &java(&spec)),
    );
    write(
        &root.join("clients/python/src/generated.rs"),
        &documented_python(&spec, &python(&spec)),
    );
    write(
        &root.join("clients/cpp/include/tarwyn_generated.hpp"),
        &cpp(&spec),
    );
}
