// Generated from clients/api.toml by codegen. Do not edit.

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
    std::size_t needed = 0;
    const int sized = call(nullptr, 0, &needed);
    if (Absent(sized)) {
        return false;
    }
    Check(sized, what);
    buffer.resize(needed);
    Check(call(buffer.data(), buffer.size(), &needed), what);
    buffer.resize(needed);
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
    /// Publishes a string to `channel`.
    void PutString(std::string_view channel, std::string_view value) {
        const std::string name(channel);
        const std::string owned(value);
        detail::Check(xt_put_string(handle_, name.c_str(), owned.c_str()), "PutString");
    }

    /// Reads a string from `channel`, or `std::nullopt` when the channel holds
    /// nothing of that type.
    [[nodiscard]] std::optional<std::string> GetString(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_string(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetString");
        std::string buffer(needed, '\0');
        detail::Check(xt_get_string(handle_, name.c_str(), buffer.data(), buffer.size(), &needed),
                      "GetString");
        buffer.resize(std::strlen(buffer.c_str()));
        return buffer;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetString(std::string_view channel, std::optional<std::string_view> expected,
                               std::string_view value) {
        const std::string name(channel);
        const std::string owned(value);
        const std::string expected_owned(expected.value_or(std::string_view{}));
        bool swapped = false;
        detail::Check(xt_compare_and_set_string(handle_, name.c_str(), expected_owned.c_str(),
                                                expected.has_value(), owned.c_str(), &swapped),
                      "CompareAndSetString");
        return swapped;
    }

    /// Publishes an integer to `channel`.
    void PutInteger(std::string_view channel, std::int32_t value) {
        const std::string name(channel);
        detail::Check(xt_put_integer(handle_, name.c_str(), value), "PutInteger");
    }

    /// Reads an integer from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::int32_t> GetInteger(std::string_view channel) const {
        const std::string name(channel);
        std::int32_t value{};
        const int code = xt_get_integer(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetInteger");
        return value;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetInteger(std::string_view channel, std::optional<std::int32_t> expected,
                               std::int32_t value) {
        const std::string name(channel);
        bool swapped = false;
        detail::Check(xt_compare_and_set_integer(handle_, name.c_str(), expected.value_or(std::int32_t{}),
                                                expected.has_value(), value, &swapped),
                      "CompareAndSetInteger");
        return swapped;
    }

    /// Publishes a long to `channel`.
    void PutLong(std::string_view channel, std::int64_t value) {
        const std::string name(channel);
        detail::Check(xt_put_long(handle_, name.c_str(), value), "PutLong");
    }

    /// Reads a long from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::int64_t> GetLong(std::string_view channel) const {
        const std::string name(channel);
        std::int64_t value{};
        const int code = xt_get_long(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetLong");
        return value;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetLong(std::string_view channel, std::optional<std::int64_t> expected,
                               std::int64_t value) {
        const std::string name(channel);
        bool swapped = false;
        detail::Check(xt_compare_and_set_long(handle_, name.c_str(), expected.value_or(std::int64_t{}),
                                                expected.has_value(), value, &swapped),
                      "CompareAndSetLong");
        return swapped;
    }

    /// Publishes a double to `channel`.
    void PutDouble(std::string_view channel, double value) {
        const std::string name(channel);
        detail::Check(xt_put_double(handle_, name.c_str(), value), "PutDouble");
    }

    /// Reads a double from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<double> GetDouble(std::string_view channel) const {
        const std::string name(channel);
        double value{};
        const int code = xt_get_double(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetDouble");
        return value;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetDouble(std::string_view channel, std::optional<double> expected,
                               double value) {
        const std::string name(channel);
        bool swapped = false;
        detail::Check(xt_compare_and_set_double(handle_, name.c_str(), expected.value_or(double{}),
                                                expected.has_value(), value, &swapped),
                      "CompareAndSetDouble");
        return swapped;
    }

    /// Publishes a float to `channel`.
    void PutFloat(std::string_view channel, float value) {
        const std::string name(channel);
        detail::Check(xt_put_float(handle_, name.c_str(), value), "PutFloat");
    }

    /// Reads a float from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<float> GetFloat(std::string_view channel) const {
        const std::string name(channel);
        float value{};
        const int code = xt_get_float(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetFloat");
        return value;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetFloat(std::string_view channel, std::optional<float> expected,
                               float value) {
        const std::string name(channel);
        bool swapped = false;
        detail::Check(xt_compare_and_set_float(handle_, name.c_str(), expected.value_or(float{}),
                                                expected.has_value(), value, &swapped),
                      "CompareAndSetFloat");
        return swapped;
    }

    /// Publishes a boolean to `channel`.
    void PutBoolean(std::string_view channel, bool value) {
        const std::string name(channel);
        detail::Check(xt_put_boolean(handle_, name.c_str(), value), "PutBoolean");
    }

    /// Reads a boolean from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<bool> GetBoolean(std::string_view channel) const {
        const std::string name(channel);
        bool value{};
        const int code = xt_get_boolean(handle_, name.c_str(), &value);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetBoolean");
        return value;
    }

    /// Sets `channel` to `value` only if it currently holds `expected`, and reports
    /// whether it swapped. Pass `std::nullopt` to claim the channel only while it is
    /// empty.
    [[nodiscard]] bool CompareAndSetBoolean(std::string_view channel, std::optional<bool> expected,
                               bool value) {
        const std::string name(channel);
        bool swapped = false;
        detail::Check(xt_compare_and_set_boolean(handle_, name.c_str(), expected.value_or(bool{}),
                                                expected.has_value(), value, &swapped),
                      "CompareAndSetBoolean");
        return swapped;
    }

    /// Publishes a list of strings to `channel`.
    void PutStringList(std::string_view channel, const std::vector<std::string>& values) {
        const std::string name(channel);
        std::vector<std::uint8_t> packed;
        detail::AppendCount(packed, values.size());
        for (const auto& item : values) {
            detail::AppendPacked(packed, item.data(), item.size());
        }
        detail::Check(xt_put_string_list(handle_, name.c_str(), packed.data(), packed.size()),
                      "PutStringList");
    }

    /// Reads a list of strings from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<std::string>> GetStringList(std::string_view channel) const {
        const std::string name(channel);
        std::vector<std::uint8_t> buffer;
        if (!detail::ReadInto(buffer, [&](std::uint8_t* out, std::size_t capacity,
                                          std::size_t* needed) {
                return xt_get_string_list(handle_, name.c_str(), out, capacity, needed);
            },
            "GetStringList")) {
            return std::nullopt;
        }

        std::vector<std::string> out;
        const std::uint8_t* cursor = buffer.data();
        const std::uint8_t* end = cursor + buffer.size();
        const std::uint32_t count = detail::ReadCount(cursor, end, "GetStringList");
        out.reserve(count);
        for (std::uint32_t index = 0; index < count; ++index) {
            const std::uint32_t length = detail::ReadCount(cursor, end, "GetStringList");
            if (static_cast<std::size_t>(end - cursor) < length) {
                throw TarwynError("GetStringList read a truncated list", XT_ERR_WRONG_TYPE);
            }
            out.emplace_back(reinterpret_cast<const char*>(cursor), length);
            cursor += length;
        }
        return out;
    }

    /// Publishes a list of byte arrays to `channel`.
    void PutBytesList(std::string_view channel, const std::vector<std::vector<std::uint8_t>>& values) {
        const std::string name(channel);
        std::vector<std::uint8_t> packed;
        detail::AppendCount(packed, values.size());
        for (const auto& item : values) {
            detail::AppendPacked(packed, reinterpret_cast<const char*>(item.data()), item.size());
        }
        detail::Check(xt_put_bytes_list(handle_, name.c_str(), packed.data(), packed.size()),
                      "PutBytesList");
    }

    /// Reads a list of byte arrays from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<std::vector<std::uint8_t>>> GetBytesList(std::string_view channel) const {
        const std::string name(channel);
        std::vector<std::uint8_t> buffer;
        if (!detail::ReadInto(buffer, [&](std::uint8_t* out, std::size_t capacity,
                                          std::size_t* needed) {
                return xt_get_bytes_list(handle_, name.c_str(), out, capacity, needed);
            },
            "GetBytesList")) {
            return std::nullopt;
        }

        std::vector<std::vector<std::uint8_t>> out;
        const std::uint8_t* cursor = buffer.data();
        const std::uint8_t* end = cursor + buffer.size();
        const std::uint32_t count = detail::ReadCount(cursor, end, "GetBytesList");
        out.reserve(count);
        for (std::uint32_t index = 0; index < count; ++index) {
            const std::uint32_t length = detail::ReadCount(cursor, end, "GetBytesList");
            if (static_cast<std::size_t>(end - cursor) < length) {
                throw TarwynError("GetBytesList read a truncated list", XT_ERR_WRONG_TYPE);
            }
            out.emplace_back(cursor, cursor + length);
            cursor += length;
        }
        return out;
    }

    /// Publishes a list of doubles to `channel`.
    void PutDoubleList(std::string_view channel, const std::vector<double>& values) {
        const std::string name(channel);
        detail::Check(xt_put_double_list(handle_, name.c_str(), values.data(), values.size()),
                      "PutDoubleList");
    }

    /// Reads a list of doubles from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<double>> GetDoubleList(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_double_list(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetDoubleList");
        std::vector<double> out(needed);
        detail::Check(xt_get_double_list(handle_, name.c_str(), out.data(), out.size(), &needed),
                      "GetDoubleList");
        out.resize(needed);
        return out;
    }

    /// Publishes a list of floats to `channel`.
    void PutFloatList(std::string_view channel, const std::vector<float>& values) {
        const std::string name(channel);
        detail::Check(xt_put_float_list(handle_, name.c_str(), values.data(), values.size()),
                      "PutFloatList");
    }

    /// Reads a list of floats from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<float>> GetFloatList(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_float_list(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetFloatList");
        std::vector<float> out(needed);
        detail::Check(xt_get_float_list(handle_, name.c_str(), out.data(), out.size(), &needed),
                      "GetFloatList");
        out.resize(needed);
        return out;
    }

    /// Publishes a list of integers to `channel`.
    void PutIntegerList(std::string_view channel, const std::vector<std::int32_t>& values) {
        const std::string name(channel);
        detail::Check(xt_put_integer_list(handle_, name.c_str(), values.data(), values.size()),
                      "PutIntegerList");
    }

    /// Reads a list of integers from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<std::int32_t>> GetIntegerList(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_integer_list(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetIntegerList");
        std::vector<std::int32_t> out(needed);
        detail::Check(xt_get_integer_list(handle_, name.c_str(), out.data(), out.size(), &needed),
                      "GetIntegerList");
        out.resize(needed);
        return out;
    }

    /// Publishes a list of longs to `channel`.
    void PutLongList(std::string_view channel, const std::vector<std::int64_t>& values) {
        const std::string name(channel);
        detail::Check(xt_put_long_list(handle_, name.c_str(), values.data(), values.size()),
                      "PutLongList");
    }

    /// Reads a list of longs from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<std::int64_t>> GetLongList(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_long_list(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetLongList");
        std::vector<std::int64_t> out(needed);
        detail::Check(xt_get_long_list(handle_, name.c_str(), out.data(), out.size(), &needed),
                      "GetLongList");
        out.resize(needed);
        return out;
    }

    /// Publishes a list of booleans to `channel`.
    void PutBooleanList(std::string_view channel, const std::vector<bool>& values) {
        const std::string name(channel);
        auto staging = std::make_unique<bool[]>(values.size());
        for (std::size_t index = 0; index < values.size(); ++index) {
            staging[index] = values[index];
        }
        detail::Check(xt_put_boolean_list(handle_, name.c_str(), staging.get(), values.size()),
                      "PutBooleanList");
    }

    /// Reads a list of booleans from `channel`, or `std::nullopt` when the channel
    /// holds nothing of that type.
    [[nodiscard]] std::optional<std::vector<bool>> GetBooleanList(std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_boolean_list(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetBooleanList");
        auto staging = std::make_unique<bool[]>(needed);
        detail::Check(xt_get_boolean_list(handle_, name.c_str(), staging.get(), needed, &needed),
                      "GetBooleanList");
        return std::vector<bool>(staging.get(), staging.get() + needed);
    }

    /// Publishes a Pose2d to `channel`.
    void PutPose2d(std::string_view channel, double x, double y, double rotation) {
        const std::string name(channel);
        const std::array<double, 3> values = {x, y, rotation};
        detail::Check(xt_put_pose2d(handle_, name.c_str(), values.data()), "PutPose2d");
    }

    /// Reads a Pose2d from `channel` as its 3 fields, or `std::nullopt` when
    /// the channel holds nothing of that type.
    [[nodiscard]] std::optional<std::array<double, 3>> GetPose2d(std::string_view channel) const {
        const std::string name(channel);
        std::array<double, 3> fields{};
        const int code = xt_get_pose2d(handle_, name.c_str(), fields.data());
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetPose2d");
        return fields;
    }

    /// Publishes a Pose3d to `channel`.
    void PutPose3d(std::string_view channel, double x, double y, double z, double roll, double pitch, double yaw) {
        const std::string name(channel);
        const std::array<double, 6> values = {x, y, z, roll, pitch, yaw};
        detail::Check(xt_put_pose3d(handle_, name.c_str(), values.data()), "PutPose3d");
    }

    /// Reads a Pose3d from `channel` as its 6 fields, or `std::nullopt` when
    /// the channel holds nothing of that type.
    [[nodiscard]] std::optional<std::array<double, 6>> GetPose3d(std::string_view channel) const {
        const std::string name(channel);
        std::array<double, 6> fields{};
        const int code = xt_get_pose3d(handle_, name.c_str(), fields.data());
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "GetPose3d");
        return fields;
    }

};

}  // namespace detail
}  // namespace tarwyn
