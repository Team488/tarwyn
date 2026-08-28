/// \file
/// A C++ client for TARWYN.
///
/// Header-only, over the C ABI in `tarwyn.h`. Include this and link the native
/// library; there is nothing to compile.
///
/// \code
/// #include <tarwyn.hpp>
///
/// tarwyn::Client client("10.4.88.2");
/// client.Start();
/// client.PutDouble("pose", 1.5);
/// if (auto value = client.GetDouble("pose")) {
///     Use(*value);
/// }
/// \endcode
///
/// An absent channel is `std::nullopt`, never an exception. Exceptions are for
/// genuine faults: a null handle, a broken payload, a filesystem error.
///
/// If WPILib's geometry headers are on the include path, the `Pose2d` and `Pose3d`
/// overloads appear. Nothing declares a dependency on WPILib, so this header still
/// compiles on a coprocessor that has never heard of it.

#pragma once

#include <chrono>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "tarwyn_generated.hpp"

#if defined(__has_include)
#  if __has_include(<wpi/math/geometry/Pose2d.hpp>)
#    include <wpi/math/geometry/Pose2d.hpp>
#    include <wpi/math/geometry/Pose3d.hpp>
#    define TARWYN_HAS_WPILIB 1
#    define TARWYN_WPILIB_NS wpi::math
#  elif __has_include(<frc/geometry/Pose2d.h>)
#    include <frc/geometry/Pose2d.h>
#    include <frc/geometry/Pose3d.h>
#    define TARWYN_HAS_WPILIB 1
#    define TARWYN_WPILIB_NS frc
#  endif
#endif

namespace tarwyn {

/// Where the client dials and how patient it is.
struct Config {
    /// Host running the server. An address, not a URL.
    std::string host = "127.0.0.1";
    /// PUSH/PULL port, used by every put.
    std::uint16_t push_port = 5557;
    /// REQ/REP port, used by every get and the control plane.
    std::uint16_t req_port = 5556;
    /// PUB/SUB port, used by subscriptions.
    std::uint16_t sub_port = 5555;
    /// How long a read waits for a reply before giving up.
    std::chrono::milliseconds request_timeout{500};
    /// Publishes past this many queued are dropped rather than queued.
    int send_high_water_mark = 500;
};

/// The server's counters, as returned by Client::Statistics.
struct ServerStatistics {
    std::uint64_t channels = 0;
    std::uint64_t values = 0;
    std::uint64_t telemetry_subscribers = 0;
    std::uint64_t uptime_seconds = 0;
    std::string version;
};

/// A connection to an TARWYN server.
///
/// Move-only, and closed when it goes out of scope. Constructing it never blocks:
/// ZeroMQ dials in the background, so a client can be built before the server
/// exists. Nothing is received until Start().
class Client : public detail::Generated {
 public:
    /// Connects to a server on `host` with the default ports.
    explicit Client(std::string_view host = "127.0.0.1")
        : Client(Config{std::string(host), 5557, 5556, 5555, std::chrono::milliseconds(500), 500}) {}

    /// Connects with the ports and timeout spelled out.
    explicit Client(const Config& config) {
        handle_ = xt_client_new(config.host.c_str(), config.push_port, config.req_port,
                                config.sub_port,
                                static_cast<std::uint64_t>(config.request_timeout.count()),
                                config.send_high_water_mark);
        if (handle_ == nullptr) {
            throw TarwynError("Client", XT_ERR_NULL);
        }
    }

    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    Client(Client&& other) noexcept { handle_ = std::exchange(other.handle_, nullptr); }

    Client& operator=(Client&& other) noexcept {
        if (this != &other) {
            Close();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    ~Client() { Close(); }

    /// Starts the receive threads, so subscriptions begin delivering. Publishing
    /// and reading work without it.
    void Start() { detail::Check(xt_client_start(handle_), "Start"); }

    /// The underlying C handle, for calling the ABI directly.
    ///
    /// This wrapper does not cover every entry point, and an escape hatch is
    /// cheaper than being trapped. The handle is owned by this client and is
    /// invalid once it goes out of scope.
    [[nodiscard]] Handle* raw() const noexcept { return handle_; }

    /// Publishes raw bytes to `channel`.
    void PutBytes(std::string_view channel, const std::vector<std::uint8_t>& value) {
        const std::string name(channel);
        detail::Check(xt_publish_bytes(handle_, name.c_str(), value.data(), value.size()),
                      "PutBytes");
    }

    /// Reads raw bytes from `channel`, or `std::nullopt` when it holds nothing of
    /// that type.
    std::optional<std::vector<std::uint8_t>> GetBytes(std::string_view channel) const {
        const std::string name(channel);
        std::vector<std::uint8_t> buffer;
        if (!detail::ReadInto(buffer,
                              [&](std::uint8_t* out, std::size_t capacity, std::size_t* needed) {
                                  return xt_get_bytes(handle_, name.c_str(), out, capacity, needed);
                              },
                              "GetBytes")) {
            return std::nullopt;
        }
        return buffer;
    }

    /// Publishes a list of `(x, y)` coordinates to `channel`.
    void PutCoordinates(std::string_view channel,
                        const std::vector<std::pair<double, double>>& values) {
        const std::string name(channel);
        std::vector<double> flat;
        flat.reserve(values.size() * 2);
        for (const auto& [x, y] : values) {
            flat.push_back(x);
            flat.push_back(y);
        }
        detail::Check(xt_put_coordinates(handle_, name.c_str(), flat.data(), values.size()),
                      "PutCoordinates");
    }

    /// Reads a coordinate list from `channel`, or `std::nullopt` when it holds
    /// nothing of that type.
    std::optional<std::vector<std::pair<double, double>>> GetCoordinates(
        std::string_view channel) const {
        const std::string name(channel);
        std::size_t needed = 0;
        const int sized = xt_get_coordinates(handle_, name.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return std::nullopt;
        }
        detail::Check(sized, "GetCoordinates");
        std::vector<double> flat(needed);
        detail::Check(xt_get_coordinates(handle_, name.c_str(), flat.data(), flat.size(), &needed),
                      "GetCoordinates");
        flat.resize(needed);

        std::vector<std::pair<double, double>> out;
        out.reserve(flat.size() / 2);
        for (std::size_t index = 0; index + 1 < flat.size(); index += 2) {
            out.emplace_back(flat[index], flat[index + 1]);
        }
        return out;
    }

    /// Publishes a value already encoded in TARWYN' own byte layout.
    ///
    /// An unrecognised tag is published as raw bytes. Returns false, publishing
    /// nothing, only when a recognised tag comes with bytes that are not a valid
    /// value of that type.
    bool PutTypedBytes(std::string_view channel, int type,
                       const std::vector<std::uint8_t>& value) {
        const std::string name(channel);
        const int code = xt_put_typed_bytes(handle_, name.c_str(), type, value.data(), value.size());
        if (code == XT_ERR_WRONG_TYPE) {
            return false;
        }
        detail::Check(code, "PutTypedBytes");
        return true;
    }

    /// Deletes `channel`, returning how many were removed. Pass "" to delete all.
    std::uint32_t Delete(std::string_view channel) {
        const std::string name(channel);
        std::uint32_t deleted = 0;
        detail::Check(xt_delete(handle_, name.c_str(), &deleted), "Delete");
        return deleted;
    }

    /// Deletes every channel, returning how many were removed.
    std::uint32_t DeleteAll() { return Delete(""); }

    /// The channel names beginning with `prefix`. Pass "" for all of them.
    std::vector<std::string> Tables(std::string_view prefix = "") const {
        const std::string owned(prefix);
        std::vector<std::uint8_t> buffer;
        if (!detail::ReadInto(buffer,
                              [&](std::uint8_t* out, std::size_t capacity, std::size_t* needed) {
                                  return xt_tables(handle_, owned.c_str(), out, capacity, needed);
                              },
                              "Tables")) {
            return {};
        }
        std::vector<std::string> out;
        const std::uint8_t* cursor = buffer.data();
        const std::uint8_t* end = cursor + buffer.size();
        const std::uint32_t count = detail::ReadCount(cursor, end, "Tables");
        out.reserve(count);
        for (std::uint32_t index = 0; index < count; ++index) {
            const std::uint32_t length = detail::ReadCount(cursor, end, "Tables");
            if (static_cast<std::size_t>(end - cursor) < length) {
                throw TarwynError("Tables read a truncated list", XT_ERR_WRONG_TYPE);
            }
            out.emplace_back(reinterpret_cast<const char*>(cursor), length);
            cursor += length;
        }
        return out;
    }

    /// Round-trip time to the server, or `std::nullopt` if it does not answer.
    std::optional<std::chrono::nanoseconds> Ping() const {
        std::uint64_t nanos = 0;
        const int code = xt_ping(handle_, &nanos);
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "Ping");
        return std::chrono::nanoseconds(nanos);
    }

    /// The server's counters, or `std::nullopt` if it does not answer.
    std::optional<ServerStatistics> Statistics() const {
        std::uint64_t fields[4] = {};
        char version[64] = {};
        const int code = xt_statistics(handle_, fields, 4, version, sizeof(version));
        if (detail::Absent(code)) {
            return std::nullopt;
        }
        detail::Check(code, "Statistics");
        return ServerStatistics{fields[0], fields[1], fields[2], fields[3], std::string(version)};
    }

    /// The channels beginning with `prefix`, as a JSON document.
    std::string RawJson(std::string_view prefix = "") const {
        const std::string owned(prefix);
        std::size_t needed = 0;
        const int sized = xt_raw_json(handle_, owned.c_str(), nullptr, 0, &needed);
        if (detail::Absent(sized)) {
            return "{}";
        }
        detail::Check(sized, "RawJson");
        std::string out(needed, '\0');
        detail::Check(xt_raw_json(handle_, owned.c_str(), out.data(), out.size(), &needed),
                      "RawJson");
        out.resize(std::strlen(out.c_str()));
        return out;
    }

    /// Mirrors every published value into a WPILOG file.
    void LogTo(std::string_view path) {
        const std::string owned(path);
        detail::Check(xt_log_to(handle_, owned.c_str()), "LogTo");
    }

    /// As LogTo, but onto the first writable removable drive that accepts it.
    /// Returns the path chosen.
    std::string LogToDrive(std::string_view filename) {
        const std::string owned(filename);
        std::string out(4096, '\0');
        detail::Check(xt_log_to_drive(handle_, owned.c_str(), out.data(), out.size()),
                      "LogToDrive");
        out.resize(std::strlen(out.c_str()));
        return out;
    }

    /// How many log records were dropped because the writer queue was full.
    std::uint64_t LogDropped() const {
        std::uint64_t value = 0;
        detail::Check(xt_log_dropped(handle_, &value), "LogDropped");
        return value;
    }

    /// Whether the log writer is still succeeding. True when logging never started.
    bool LoggingHealthy() const {
        bool value = false;
        detail::Check(xt_logging_healthy(handle_, &value), "LoggingHealthy");
        return value;
    }

    /// How many publishes were dropped rather than queued.
    std::uint64_t DroppedPublishes() const {
        std::uint64_t value = 0;
        detail::Check(xt_dropped_publishes(handle_, &value), "DroppedPublishes");
        return value;
    }

#ifdef TARWYN_HAS_WPILIB
    /// Publishes a pose to `channel`.
    void PutPose2d(std::string_view channel, const TARWYN_WPILIB_NS::Pose2d& value) {
        PutPose2d(channel, value.X().value(), value.Y().value(),
                  value.Rotation().Radians().value());
    }

    /// Publishes a pose to `channel`.
    void PutPose3d(std::string_view channel, const TARWYN_WPILIB_NS::Pose3d& value) {
        PutPose3d(channel, value.X().value(), value.Y().value(), value.Z().value(),
                  value.Rotation().X().value(), value.Rotation().Y().value(),
                  value.Rotation().Z().value());
    }
#endif

    /// A ring of payloads written by the native client and read here directly.
    ///
    /// The bytes are copied out of the mapped buffer without crossing the FFI,
    /// which is what keeps a subscription cheap. A writer that laps the reader
    /// overwrites slots it has not drained; Lapped() reports that.
    class Subscription {
     public:
        Subscription(const Subscription&) = delete;
        Subscription& operator=(const Subscription&) = delete;

        Subscription(Subscription&& other) noexcept
            : client_(other.client_), id_(other.id_), records_(other.records_),
              record_bytes_(other.record_bytes_), read_index_(other.read_index_) {
            other.client_ = nullptr;
        }

        ~Subscription() { Close(); }

        /// The subscription's id, for calling the ABI directly.
        [[nodiscard]] std::uint64_t id() const noexcept { return id_; }

        /// How many payloads have been pushed since the ring was created.
        [[nodiscard]] std::uint64_t WriteIndex() const {
            std::uint64_t value = 0;
            detail::Check(xt_ring_write_index(client_, id_, &value), "WriteIndex");
            return value;
        }

        /// Whether the writer has overwritten payloads never drained.
        [[nodiscard]] bool Lapped() const { return WriteIndex() - read_index_ > records_; }

        /// Takes every payload written since the last drain, oldest first.
        ///
        /// Payloads the writer overwrote while this was copying are left out rather
        /// than returned torn.
        std::vector<std::vector<std::uint8_t>> Drain() {
            const std::uint64_t available = WriteIndex();
            std::vector<std::vector<std::uint8_t>> values;
            if (available <= read_index_) {
                return values;
            }
            const auto* base = static_cast<const std::uint8_t*>(xt_ring_base(client_, id_));
            if (base == nullptr) {
                throw TarwynError("Drain", XT_ERR_NO_VALUE);
            }
            std::uint64_t from = available > records_ ? available - records_ : 0;
            from = from > read_index_ ? from : read_index_;
            for (std::uint64_t index = from; index < available; ++index) {
                const std::uint8_t* slot = base + (index % records_) * record_bytes_;
                std::uint64_t length = 0;
                std::memcpy(&length, slot, sizeof(length));
                if (length > record_bytes_ - 8) {
                    continue;
                }
                std::vector<std::uint8_t> payload(slot + 8, slot + 8 + length);
                if (WriteIndex() - index <= records_) {
                    values.push_back(std::move(payload));
                }
            }
            read_index_ = available;
            return values;
        }

        /// Cancels the subscription and releases the ring. Safe to call twice.
        void Close() {
            if (client_ != nullptr) {
                xt_unsubscribe(const_cast<Handle*>(client_), id_);
                client_ = nullptr;
            }
        }

     private:
        friend class Client;
        Subscription(Handle* client, std::uint64_t id, std::size_t records,
                     std::size_t record_bytes)
            : client_(client), id_(id), records_(records), record_bytes_(record_bytes) {}

        Handle* client_;
        std::uint64_t id_ = 0;
        std::uint64_t records_ = 0;
        std::uint64_t record_bytes_ = 0;
        std::uint64_t read_index_ = 0;
    };

    /// Subscribes to `channel`, delivering payloads into a ring the caller drains.
    ///
    /// `record_bytes` must exceed 8, since each slot carries its payload length
    /// ahead of the payload.
    Subscription Subscribe(std::string_view channel, std::size_t records = 256,
                           std::size_t record_bytes = 4096) {
        const std::string name(channel);
        std::uint64_t id = 0;
        detail::Check(xt_subscribe_ring(handle_, name.c_str(), records, record_bytes, &id),
                      "Subscribe");
        return Subscription(handle_, id, records, record_bytes);
    }

 private:
    void Close() {
        if (handle_ != nullptr) {
            xt_client_free(handle_);
            handle_ = nullptr;
        }
    }
};

}  // namespace tarwyn
