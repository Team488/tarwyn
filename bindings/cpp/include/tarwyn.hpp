/**
 * @file
 * The tarwyn client for C++. Poses, coordinates and bezier control points are
 * wpi::math geometry; everything else is std::string, std::vector or std::span.
 */
#ifndef TARWYN_HPP
#define TARWYN_HPP

#include <cstdint>
#include <cmath>
#include <cstring>
#include <functional>
#include <limits>
#include <optional>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <wpi/math/geometry/Pose2d.hpp>
#include <wpi/math/geometry/Pose3d.hpp>
#include <wpi/math/geometry/Quaternion.hpp>
#include <wpi/math/geometry/Rotation2d.hpp>
#include <wpi/math/geometry/Rotation3d.hpp>
#include <wpi/math/geometry/Translation2d.hpp>

#include "tarwyn.h"

namespace tarwyn {

/** A control point of a bezier curve; no rotation when the point carries no heading. */
struct Point {
  double x;
  double y;
  std::optional<double> rotation_degrees;
};

/** What the server reports about itself. */
struct ServerStatistics {
  uint64_t channels;
  uint64_t values;
  uint64_t telemetry_subscribers;
  uint64_t uptime_seconds;
  uint64_t dropped_publishes;
  uint64_t dropped_logs;
  std::string version;
};

/** Receives a value as its protobuf `SupportedValues` encoding, or a log line. */
using SampleCallback = std::function<void(std::string_view channel, std::span<const uint8_t> value)>;

/** Receives a telemetry sample and the publisher's timestamp in microseconds since the Unix epoch. */
using TelemetryCallback = std::function<void(uint64_t timestamp_micros, std::span<const uint8_t> payload)>;

/** The channel a log subscription reports its lines under. */
inline constexpr std::string_view kLogsChannel = "logs";

namespace detail {

inline const uint8_t* Data(std::string_view text) {
  return reinterpret_cast<const uint8_t*>(text.data());
}

/** Bytes the library handed out, released when this goes out of scope. */
class Owned {
 public:
  Owned(uint8_t* ptr, size_t len) : ptr_(ptr), len_(len) {}
  Owned(const Owned&) = delete;
  Owned& operator=(const Owned&) = delete;
  ~Owned() { tarwyn_bytes_free(ptr_, len_); }

  [[nodiscard]] bool Absent() const { return ptr_ == nullptr; }
  [[nodiscard]] std::span<const uint8_t> Bytes() const { return {ptr_, len_}; }
  [[nodiscard]] std::string Text() const { return {reinterpret_cast<const char*>(ptr_), len_}; }

  template <class T>
  [[nodiscard]] std::vector<T> Unpack() const {
    std::vector<T> values(len_ / sizeof(T));
    if (!values.empty()) {
      std::memcpy(values.data(), ptr_, values.size() * sizeof(T));
    }
    return values;
  }

  [[nodiscard]] std::vector<std::string> Unframe() const {
    std::vector<std::string> items;
    size_t at = 0;
    while (at + 4 <= len_) {
      uint32_t len;
      std::memcpy(&len, ptr_ + at, 4);
      at += 4;
      if (at + len > len_) {
        break;
      }
      items.emplace_back(reinterpret_cast<const char*>(ptr_ + at), len);
      at += len;
    }
    return items;
  }

 private:
  uint8_t* ptr_;
  size_t len_;
};

template <class Item>
std::vector<uint8_t> Frame(std::span<const Item> items) {
  std::vector<uint8_t> out;
  for (const auto& item : items) {
    std::span<const uint8_t> bytes = std::span<const uint8_t>(
        reinterpret_cast<const uint8_t*>(std::data(item)), std::size(item));
    auto len = static_cast<uint32_t>(bytes.size());
    const auto* len_bytes = reinterpret_cast<const uint8_t*>(&len);
    out.insert(out.end(), len_bytes, len_bytes + 4);
    out.insert(out.end(), bytes.begin(), bytes.end());
  }
  return out;
}

template <class Callback, class... Args>
void Invoke(void* ctx, Args... args) noexcept {
  try {
    (*static_cast<Callback*>(ctx))(args...);
  } catch (...) {
    return;
  }
}

inline void OnSample(void* ctx, const uint8_t* channel, size_t channel_len, const uint8_t* value,
                     size_t value_len) {
  Invoke<SampleCallback>(ctx, std::string_view(reinterpret_cast<const char*>(channel), channel_len),
                         std::span<const uint8_t>(value, value_len));
}

inline void OnTelemetry(void* ctx, uint64_t timestamp_micros, const uint8_t* payload,
                        size_t payload_len) {
  Invoke<TelemetryCallback>(ctx, timestamp_micros, std::span<const uint8_t>(payload, payload_len));
}

template <class Callback>
void Release(void* ctx) {
  delete static_cast<Callback*>(ctx);
}

}  // namespace detail

/**
 * One connection: publishes, reads, control and subscriptions.
 *
 * Every method is safe to call from any thread. Reads return std::nullopt
 * when the server does not answer within the request timeout, and publishing
 * without a server neither blocks nor throws. Subscription callbacks run on
 * the client's receive threads; anything they throw is dropped.
 */
class Client {
 public:
  /** A client for a server on this machine. */
  Client() : Client(tarwyn_client_new()) {}

  /** A client for the server on `host`, an address rather than a URL. */
  static Client Connect(std::string_view host) {
    return Client(tarwyn_client_connect(detail::Data(host), host.size()));
  }

  /**
   * A client with every port, timeout and window spelled out.
   *
   * `busy_poll_micros` is how long the reader spins on its socket before it
   * blocks, so a subscribed value is delivered without a thread wakeup; 0
   * blocks at once. `predict_micros` is how far around a predicted arrival
   * the reader spins instead, once the stream has shown a period; 0 turns
   * prediction off, and the default matches `Connect`.
   */
  static Client WithPorts(std::string_view host, uint16_t port, uint16_t telemetry_port,
                          uint64_t request_timeout_ms, int32_t send_high_water_mark,
                          uint64_t busy_poll_micros = 0, uint64_t predict_micros = 200) {
    return Client(tarwyn_client_with_ports(detail::Data(host), host.size(), port, telemetry_port,
                                       request_timeout_ms, send_high_water_mark,
                                       busy_poll_micros, predict_micros));
  }

  Client(Client&& other) noexcept : client_(std::exchange(other.client_, nullptr)) {}
  Client& operator=(Client&& other) noexcept {
    if (this != &other) {
      tarwyn_client_free(client_);
      client_ = std::exchange(other.client_, nullptr);
    }
    return *this;
  }
  Client(const Client&) = delete;
  Client& operator=(const Client&) = delete;

  /** Stops the client and cancels its subscriptions. */
  ~Client() { tarwyn_client_free(client_); }

  void Start() { tarwyn_client_start(client_); }
  void Stop() { tarwyn_client_stop(client_); }

  void PutString(std::string_view channel, std::string_view value) {
    tarwyn_put_string(client_, detail::Data(channel), channel.size(), detail::Data(value), value.size());
  }
  void PutInteger(std::string_view channel, int32_t value) {
    tarwyn_put_integer(client_, detail::Data(channel), channel.size(), value);
  }
  void PutLong(std::string_view channel, int64_t value) {
    tarwyn_put_long(client_, detail::Data(channel), channel.size(), value);
  }
  void PutDouble(std::string_view channel, double value) {
    tarwyn_put_double(client_, detail::Data(channel), channel.size(), value);
  }
  void PutFloat(std::string_view channel, float value) {
    tarwyn_put_float(client_, detail::Data(channel), channel.size(), value);
  }
  void PutBoolean(std::string_view channel, bool value) {
    tarwyn_put_boolean(client_, detail::Data(channel), channel.size(), value);
  }
  void PutBytes(std::string_view channel, std::span<const uint8_t> value) {
    tarwyn_put_bytes(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutStringList(std::string_view channel, std::span<const std::string> value) {
    auto framed = detail::Frame(value);
    tarwyn_put_string_list(client_, detail::Data(channel), channel.size(), framed.data(), framed.size());
  }
  void PutBytesList(std::string_view channel, std::span<const std::vector<uint8_t>> value) {
    auto framed = detail::Frame(value);
    tarwyn_put_bytes_list(client_, detail::Data(channel), channel.size(), framed.data(), framed.size());
  }
  void PutDoubleList(std::string_view channel, std::span<const double> value) {
    tarwyn_put_double_list(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutFloatList(std::string_view channel, std::span<const float> value) {
    tarwyn_put_float_list(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutIntegerList(std::string_view channel, std::span<const int32_t> value) {
    tarwyn_put_integer_list(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutLongList(std::string_view channel, std::span<const int64_t> value) {
    tarwyn_put_long_list(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutBooleanList(std::string_view channel, std::span<const bool> value) {
    tarwyn_put_boolean_list(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  void PutCoordinates(std::string_view channel, std::span<const wpi::math::Translation2d> value) {
    std::vector<double> xy;
    xy.reserve(value.size() * 2);
    for (const auto& point : value) {
      xy.push_back(point.X().value());
      xy.push_back(point.Y().value());
    }
    tarwyn_put_coordinates(client_, detail::Data(channel), channel.size(), xy.data(), value.size());
  }
  void PutPose2d(std::string_view channel, const wpi::math::Pose2d& pose) {
    tarwyn_put_pose2d(client_, detail::Data(channel), channel.size(), pose.X().value(),
                  pose.Y().value(), pose.Rotation().Radians().value());
  }
  void PutPose3d(std::string_view channel, const wpi::math::Pose3d& pose) {
    const auto& q = pose.Rotation().GetQuaternion();
    tarwyn_put_pose3d(client_, detail::Data(channel), channel.size(), pose.X().value(),
                  pose.Y().value(), pose.Z().value(), q.W(), q.X(), q.Y(), q.Z());
  }
  void PutBezierCurve(std::string_view channel, std::span<const Point> value) {
    std::vector<double> xyr;
    xyr.reserve(value.size() * 3);
    for (const auto& point : value) {
      xyr.push_back(point.x);
      xyr.push_back(point.y);
      xyr.push_back(point.rotation_degrees.value_or(std::numeric_limits<double>::quiet_NaN()));
    }
    tarwyn_put_bezier_curve(client_, detail::Data(channel), channel.size(), xyr.data(), value.size());
  }
  /** `value` is an encoded protobuf `BezierCurves`; false when it is not. */
  bool PutBezierCurves(std::string_view channel, std::span<const uint8_t> value) {
    return tarwyn_put_bezier_curves(client_, detail::Data(channel), channel.size(), value.data(),
                                value.size());
  }
  /** `value` is an encoded protobuf `BezierCurvesList`; false when it is not. */
  bool PutBezierCurvesList(std::string_view channel, std::span<const uint8_t> value) {
    return tarwyn_put_bezier_curves_list(client_, detail::Data(channel), channel.size(), value.data(),
                                     value.size());
  }
  /** False when `value` does not decode as `tarwyn_type`. */
  bool PutTypedBytes(std::string_view channel, int32_t tarwyn_type, std::span<const uint8_t> value) {
    return tarwyn_put_typed_bytes(client_, detail::Data(channel), channel.size(), tarwyn_type,
                              value.data(), value.size());
  }
  void PutUnknownBytes(std::string_view channel, std::span<const uint8_t> value) {
    tarwyn_put_unknown_bytes(client_, detail::Data(channel), channel.size(), value.data(), value.size());
  }
  /**
   * Publishes `packed` as a struct topic of `type_name`, announcing each
   * (name, schema) in `schemas` once.
   */
  void PutStruct(std::string_view channel, std::string_view type_name,
                 std::span<const std::pair<std::string, std::string>> schemas,
                 std::span<const uint8_t> packed) {
    std::vector<std::string_view> flat;
    flat.reserve(schemas.size() * 2);
    for (const auto& [name, schema] : schemas) {
      flat.push_back(name);
      flat.push_back(schema);
    }
    auto framed = detail::Frame(std::span<const std::string_view>(flat));
    tarwyn_put_struct(client_, detail::Data(channel), channel.size(), detail::Data(type_name),
                  type_name.size(), framed.data(), framed.size(), packed.data(), packed.size());
  }

  std::optional<std::string> GetString(std::string_view channel) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_get_string(client_, detail::Data(channel), channel.size(), &len);
    detail::Owned owned(ptr, len);
    if (owned.Absent()) {
      return std::nullopt;
    }
    return owned.Text();
  }
  std::optional<int32_t> GetInteger(std::string_view channel) {
    int32_t out;
    if (!tarwyn_get_integer(client_, detail::Data(channel), channel.size(), &out)) {
      return std::nullopt;
    }
    return out;
  }
  std::optional<int64_t> GetLong(std::string_view channel) {
    int64_t out;
    if (!tarwyn_get_long(client_, detail::Data(channel), channel.size(), &out)) {
      return std::nullopt;
    }
    return out;
  }
  std::optional<double> GetDouble(std::string_view channel) {
    double out;
    if (!tarwyn_get_double(client_, detail::Data(channel), channel.size(), &out)) {
      return std::nullopt;
    }
    return out;
  }
  std::optional<float> GetFloat(std::string_view channel) {
    float out;
    if (!tarwyn_get_float(client_, detail::Data(channel), channel.size(), &out)) {
      return std::nullopt;
    }
    return out;
  }
  std::optional<bool> GetBoolean(std::string_view channel) {
    bool out;
    if (!tarwyn_get_boolean(client_, detail::Data(channel), channel.size(), &out)) {
      return std::nullopt;
    }
    return out;
  }
  std::optional<std::vector<uint8_t>> GetBytes(std::string_view channel) {
    return Bytes(tarwyn_get_bytes, channel);
  }
  std::optional<std::vector<std::string>> GetStringList(std::string_view channel) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_get_string_list(client_, detail::Data(channel), channel.size(), &len);
    detail::Owned owned(ptr, len);
    if (owned.Absent()) {
      return std::nullopt;
    }
    return owned.Unframe();
  }
  std::optional<std::vector<std::vector<uint8_t>>> GetBytesList(std::string_view channel) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_get_bytes_list(client_, detail::Data(channel), channel.size(), &len);
    detail::Owned owned(ptr, len);
    if (owned.Absent()) {
      return std::nullopt;
    }
    std::vector<std::vector<uint8_t>> items;
    for (auto& item : owned.Unframe()) {
      items.emplace_back(item.begin(), item.end());
    }
    return items;
  }
  std::optional<std::vector<double>> GetDoubleList(std::string_view channel) {
    return Unpacked<double>(tarwyn_get_double_list, channel);
  }
  std::optional<std::vector<float>> GetFloatList(std::string_view channel) {
    return Unpacked<float>(tarwyn_get_float_list, channel);
  }
  std::optional<std::vector<int32_t>> GetIntegerList(std::string_view channel) {
    return Unpacked<int32_t>(tarwyn_get_integer_list, channel);
  }
  std::optional<std::vector<int64_t>> GetLongList(std::string_view channel) {
    return Unpacked<int64_t>(tarwyn_get_long_list, channel);
  }
  std::optional<std::vector<bool>> GetBooleanList(std::string_view channel) {
    auto bytes = Unpacked<uint8_t>(tarwyn_get_boolean_list, channel);
    if (!bytes) {
      return std::nullopt;
    }
    return std::vector<bool>(bytes->begin(), bytes->end());
  }
  std::optional<std::vector<wpi::math::Translation2d>> GetCoordinates(std::string_view channel) {
    auto xy = Unpacked<double>(tarwyn_get_coordinates, channel);
    if (!xy) {
      return std::nullopt;
    }
    std::vector<wpi::math::Translation2d> points;
    for (size_t at = 0; at + 1 < xy->size(); at += 2) {
      points.emplace_back(wpi::units::meter_t{(*xy)[at]}, wpi::units::meter_t{(*xy)[at + 1]});
    }
    return points;
  }
  std::optional<std::vector<Point>> GetBezierCurve(std::string_view channel) {
    auto xyr = Unpacked<double>(tarwyn_get_bezier_curve, channel);
    if (!xyr) {
      return std::nullopt;
    }
    std::vector<Point> points;
    for (size_t at = 0; at + 2 < xyr->size(); at += 3) {
      double rotation = (*xyr)[at + 2];
      points.push_back({(*xyr)[at], (*xyr)[at + 1],
                        std::isnan(rotation) ? std::nullopt : std::optional<double>(rotation)});
    }
    return points;
  }
  /** The encoded protobuf `BezierCurves` on `channel`. */
  std::optional<std::vector<uint8_t>> GetBezierCurves(std::string_view channel) {
    return Bytes(tarwyn_get_bezier_curves, channel);
  }
  /** The encoded protobuf `BezierCurvesList` on `channel`. */
  std::optional<std::vector<uint8_t>> GetBezierCurvesList(std::string_view channel) {
    return Bytes(tarwyn_get_bezier_curves_list, channel);
  }
  std::optional<wpi::math::Pose2d> GetPose2d(std::string_view channel) {
    double fields[3];
    if (!tarwyn_get_pose2d(client_, detail::Data(channel), channel.size(), fields)) {
      return std::nullopt;
    }
    return wpi::math::Pose2d(wpi::units::meter_t{fields[0]}, wpi::units::meter_t{fields[1]},
                             wpi::math::Rotation2d(wpi::units::radian_t{fields[2]}));
  }
  std::optional<wpi::math::Pose3d> GetPose3d(std::string_view channel) {
    double f[7];
    if (!tarwyn_get_pose3d(client_, detail::Data(channel), channel.size(), f)) {
      return std::nullopt;
    }
    return wpi::math::Pose3d(wpi::units::meter_t{f[0]}, wpi::units::meter_t{f[1]},
                             wpi::units::meter_t{f[2]},
                             wpi::math::Rotation3d(wpi::math::Quaternion(f[3], f[4], f[5], f[6])));
  }
  std::optional<std::vector<uint8_t>> GetUnknownBytes(std::string_view channel) {
    return Bytes(tarwyn_get_unknown_bytes, channel);
  }

  /** How many channels were removed: 0 or 1. */
  uint32_t Delete(std::string_view channel) {
    return tarwyn_delete(client_, detail::Data(channel), channel.size());
  }
  uint32_t DeleteAll() { return tarwyn_delete_all(client_); }
  std::vector<std::string> GetTables(std::string_view prefix) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_get_tables(client_, detail::Data(prefix), prefix.size(), &len);
    detail::Owned owned(ptr, len);
    return owned.Unframe();
  }
  /** The round trip to the server in nanoseconds. */
  std::optional<uint64_t> GetPing() {
    uint64_t nanos;
    if (!tarwyn_get_ping(client_, &nanos)) {
      return std::nullopt;
    }
    return nanos;
  }
  std::optional<ServerStatistics> GetServerStatistics() {
    TarwynStatistics raw;
    uint8_t* version = nullptr;
    size_t version_len = 0;
    if (!tarwyn_get_server_statistics(client_, &raw, &version, &version_len)) {
      return std::nullopt;
    }
    detail::Owned owned(version, version_len);
    return ServerStatistics{raw.channels,           raw.values,       raw.telemetry_subscribers,
                            raw.uptime_seconds,     raw.dropped_publishes, raw.dropped_logs,
                            owned.Text()};
  }
  /** The JSON of everything under `prefix`; `{}` when the server is absent. */
  std::string GetRawJson(std::string_view prefix) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_get_raw_json(client_, detail::Data(prefix), prefix.size(), &len);
    detail::Owned owned(ptr, len);
    return owned.Text();
  }
  bool CompareAndSetAbsentString(std::string_view channel, std::string_view value) {
    return tarwyn_compare_and_set_absent_string(client_, detail::Data(channel), channel.size(),
                                            detail::Data(value), value.size());
  }
  bool CompareAndSetString(std::string_view channel, std::string_view expected,
                           std::string_view value) {
    return tarwyn_compare_and_set_string(client_, detail::Data(channel), channel.size(),
                                     detail::Data(expected), expected.size(), detail::Data(value),
                                     value.size());
  }
  bool CompareAndSetDouble(std::string_view channel, double expected, double value) {
    return tarwyn_compare_and_set_double(client_, detail::Data(channel), channel.size(), expected, value);
  }
  bool CompareAndSetLong(std::string_view channel, int64_t expected, int64_t value) {
    return tarwyn_compare_and_set_long(client_, detail::Data(channel), channel.size(), expected, value);
  }
  bool CompareAndSetBoolean(std::string_view channel, bool expected, bool value) {
    return tarwyn_compare_and_set_boolean(client_, detail::Data(channel), channel.size(), expected,
                                      value);
  }

  void PublishTelemetry(std::string_view channel, std::span<const uint8_t> payload) {
    tarwyn_publish_telemetry(client_, detail::Data(channel), channel.size(), payload.data(),
                         payload.size());
  }
  bool LogTo(std::string_view path) { return tarwyn_log_to(client_, detail::Data(path), path.size()); }
  /** The path the log landed at, or std::nullopt when it could not be opened. */
  std::optional<std::string> LogToDrive(std::string_view filename) {
    size_t len = 0;
    uint8_t* ptr = tarwyn_log_to_drive(client_, detail::Data(filename), filename.size(), &len);
    detail::Owned owned(ptr, len);
    if (owned.Absent()) {
      return std::nullopt;
    }
    return owned.Text();
  }
  uint64_t DroppedLogRecords() { return tarwyn_dropped_log_records(client_); }
  bool LoggingHealthy() { return tarwyn_logging_healthy(client_); }
  uint64_t DroppedPublishes() { return tarwyn_dropped_publishes(client_); }

  /** False when `channel` already has a subscription. */
  bool Subscribe(std::string_view channel, SampleCallback callback) {
    return tarwyn_subscribe(client_, detail::Data(channel), channel.size(), detail::OnSample,
                        new SampleCallback(std::move(callback)), detail::Release<SampleCallback>);
  }
  bool Unsubscribe(std::string_view channel) {
    return tarwyn_unsubscribe(client_, detail::Data(channel), channel.size());
  }
  /** False when `channel` already has a subscription or the telemetry plane refused it. */
  bool SubscribeTelemetry(std::string_view channel, TelemetryCallback callback) {
    return tarwyn_subscribe_telemetry(client_, detail::Data(channel), channel.size(), detail::OnTelemetry,
                                  new TelemetryCallback(std::move(callback)),
                                  detail::Release<TelemetryCallback>);
  }
  bool UnsubscribeTelemetry(std::string_view channel) {
    return tarwyn_unsubscribe_telemetry(client_, detail::Data(channel), channel.size());
  }
  /** Lines arrive on kLogsChannel. False when logs are already subscribed. */
  bool SubscribeToLogs(SampleCallback callback) {
    return tarwyn_subscribe_to_logs(client_, detail::OnSample, new SampleCallback(std::move(callback)),
                                detail::Release<SampleCallback>);
  }
  bool UnsubscribeFromLogs() { return tarwyn_unsubscribe_from_logs(client_); }

 private:
  using Reader = uint8_t* (*)(const TarwynClient*, const uint8_t*, size_t, size_t*);

  explicit Client(TarwynClient* client) : client_(client) {
    if (tarwyn_abi_version() != TARWYN_ABI_VERSION) {
      tarwyn_client_free(client_);
      throw std::runtime_error("tarwyn: the loaded library speaks ABI " +
                               std::to_string(tarwyn_abi_version()) + ", this header ABI " +
                               std::to_string(TARWYN_ABI_VERSION));
    }
  }

  std::optional<std::vector<uint8_t>> Bytes(Reader read, std::string_view channel) {
    return Unpacked<uint8_t>(read, channel);
  }

  template <class T>
  std::optional<std::vector<T>> Unpacked(Reader read, std::string_view channel) {
    size_t len = 0;
    uint8_t* ptr = read(client_, detail::Data(channel), channel.size(), &len);
    detail::Owned owned(ptr, len);
    if (owned.Absent()) {
      return std::nullopt;
    }
    return owned.Unpack<T>();
  }

  TarwynClient* client_;
};

}  // namespace tarwyn

#endif
