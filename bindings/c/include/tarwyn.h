/* The C ABI of the tarwyn client. Generated from bindings/c/src/lib.rs; do not edit. */

#ifndef TARWYN_H
#define TARWYN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/**
 * Bumped whenever a signature or encoding in this header changes. A wrapper
 * compares it against `tarwyn_abi_version()` before using anything else.
 */
#define TARWYN_ABI_VERSION 1

/**
 * A client. Opaque.
 */
typedef struct TarwynClient TarwynClient;

/**
 * What the server reports about itself; see `tarwyn_get_server_statistics`.
 */
typedef struct TarwynStatistics {
  uint64_t channels;
  uint64_t values;
  uint64_t telemetry_subscribers;
  uint64_t uptime_seconds;
  uint64_t dropped_publishes;
  uint64_t dropped_logs;
} TarwynStatistics;

/**
 * Receives a value or log line: the channel it arrived on and, for values,
 * the protobuf `SupportedValues` encoding of the value; for log lines, the
 * line.
 */
typedef void (*TarwynSampleFn)(void *ctx,
                               const uint8_t *channel,
                               size_t channel_len,
                               const uint8_t *value,
                               size_t value_len);

/**
 * Releases a callback's `ctx`. May be `NULL`.
 */
typedef void (*TarwynDropFn)(void *ctx);

/**
 * Receives a telemetry sample and the publisher's timestamp in microseconds
 * since the Unix epoch.
 */
typedef void (*TarwynTelemetryFn)(void *ctx,
                                  uint64_t timestamp_micros,
                                  const uint8_t *payload,
                                  size_t payload_len);

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * The ABI this library was built with; compare with `TARWYN_ABI_VERSION`.
 */
uint32_t tarwyn_abi_version(void);

/**
 * A client for a server on this machine.
 */
struct TarwynClient *tarwyn_client_new(void);

/**
 * A client for the server on `host`, an address rather than a URL.
 */
struct TarwynClient *tarwyn_client_connect(const uint8_t *host, size_t host_len);

/**
 * A client with every port, timeout and window spelled out.
 *
 * `busy_poll_micros` is how long the reader spins on its socket before it
 * blocks, so a subscribed value is delivered without a thread wakeup; 0
 * blocks at once. `predict_micros` is how far around a predicted arrival
 * the reader spins instead, once the stream has shown a period; 0 turns
 * prediction off, and [`tarwyn_client_connect`] uses the library's default.
 */
struct TarwynClient *tarwyn_client_with_ports(const uint8_t *host,
                                              size_t host_len,
                                              uint16_t port,
                                              uint16_t telemetry_port,
                                              uint64_t request_timeout_ms,
                                              int32_t send_high_water_mark,
                                              uint64_t busy_poll_micros,
                                              uint64_t predict_micros);

/**
 * Stops the client, cancels its subscriptions and releases it. `NULL` is fine.
 */
void tarwyn_client_free(struct TarwynClient *client);

void tarwyn_client_start(const struct TarwynClient *client);

void tarwyn_client_stop(const struct TarwynClient *client);

/**
 * Releases bytes the library handed out. `NULL` is fine.
 */
void tarwyn_bytes_free(uint8_t *ptr, size_t len);

void tarwyn_put_string(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       const uint8_t *value,
                       size_t value_len);

void tarwyn_put_integer(const struct TarwynClient *client,
                        const uint8_t *channel,
                        size_t channel_len,
                        int32_t value);

void tarwyn_put_long(const struct TarwynClient *client,
                     const uint8_t *channel,
                     size_t channel_len,
                     int64_t value);

void tarwyn_put_double(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double value);

void tarwyn_put_float(const struct TarwynClient *client,
                      const uint8_t *channel,
                      size_t channel_len,
                      float value);

void tarwyn_put_boolean(const struct TarwynClient *client,
                        const uint8_t *channel,
                        size_t channel_len,
                        bool value);

void tarwyn_put_bytes(const struct TarwynClient *client,
                      const uint8_t *channel,
                      size_t channel_len,
                      const uint8_t *value,
                      size_t value_len);

/**
 * `value` is a frame of strings.
 */
void tarwyn_put_string_list(const struct TarwynClient *client,
                            const uint8_t *channel,
                            size_t channel_len,
                            const uint8_t *value,
                            size_t value_len);

/**
 * `value` is a frame of byte strings.
 */
void tarwyn_put_bytes_list(const struct TarwynClient *client,
                           const uint8_t *channel,
                           size_t channel_len,
                           const uint8_t *value,
                           size_t value_len);

void tarwyn_put_double_list(const struct TarwynClient *client,
                            const uint8_t *channel,
                            size_t channel_len,
                            const double *value,
                            size_t count);

void tarwyn_put_float_list(const struct TarwynClient *client,
                           const uint8_t *channel,
                           size_t channel_len,
                           const float *value,
                           size_t count);

void tarwyn_put_integer_list(const struct TarwynClient *client,
                             const uint8_t *channel,
                             size_t channel_len,
                             const int32_t *value,
                             size_t count);

void tarwyn_put_long_list(const struct TarwynClient *client,
                          const uint8_t *channel,
                          size_t channel_len,
                          const int64_t *value,
                          size_t count);

void tarwyn_put_boolean_list(const struct TarwynClient *client,
                             const uint8_t *channel,
                             size_t channel_len,
                             const bool *value,
                             size_t count);

/**
 * `xy` holds `count` points as `x, y` pairs.
 */
void tarwyn_put_coordinates(const struct TarwynClient *client,
                            const uint8_t *channel,
                            size_t channel_len,
                            const double *xy,
                            size_t count);

/**
 * `rotation` is in radians.
 */
void tarwyn_put_pose2d(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double x,
                       double y,
                       double rotation);

/**
 * The rotation is a quaternion, `w` first.
 */
void tarwyn_put_pose3d(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double x,
                       double y,
                       double z,
                       double qw,
                       double qx,
                       double qy,
                       double qz);

/**
 * `xyr` holds `count` control points as `x, y, rotation_degrees` triples.
 */
void tarwyn_put_bezier_curve(const struct TarwynClient *client,
                             const uint8_t *channel,
                             size_t channel_len,
                             const double *xyr,
                             size_t count);

/**
 * `value` is an encoded protobuf `BezierCurves`; false when it is not.
 */
bool tarwyn_put_bezier_curves(const struct TarwynClient *client,
                              const uint8_t *channel,
                              size_t channel_len,
                              const uint8_t *value,
                              size_t value_len);

/**
 * `value` is an encoded protobuf `BezierCurvesList`; false when it is not.
 */
bool tarwyn_put_bezier_curves_list(const struct TarwynClient *client,
                                   const uint8_t *channel,
                                   size_t channel_len,
                                   const uint8_t *value,
                                   size_t value_len);

/**
 * False when `value` does not decode as `tarwyn_type`.
 */
bool tarwyn_put_typed_bytes(const struct TarwynClient *client,
                            const uint8_t *channel,
                            size_t channel_len,
                            int32_t tarwyn_type,
                            const uint8_t *value,
                            size_t value_len);

void tarwyn_put_unknown_bytes(const struct TarwynClient *client,
                              const uint8_t *channel,
                              size_t channel_len,
                              const uint8_t *value,
                              size_t value_len);

/**
 * Publishes `packed` as a struct topic of `type_name`. `schemas` is a
 * frame of alternating struct names and their schemas, each announced once.
 */
void tarwyn_put_struct(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       const uint8_t *type_name,
                       size_t type_name_len,
                       const uint8_t *schemas,
                       size_t schemas_len,
                       const uint8_t *packed,
                       size_t packed_len);

uint8_t *tarwyn_get_string(const struct TarwynClient *client,
                           const uint8_t *channel,
                           size_t channel_len,
                           size_t *out_len);

bool tarwyn_get_integer(const struct TarwynClient *client,
                        const uint8_t *channel,
                        size_t channel_len,
                        int32_t *out);

bool tarwyn_get_long(const struct TarwynClient *client,
                     const uint8_t *channel,
                     size_t channel_len,
                     int64_t *out);

bool tarwyn_get_double(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double *out);

bool tarwyn_get_float(const struct TarwynClient *client,
                      const uint8_t *channel,
                      size_t channel_len,
                      float *out);

bool tarwyn_get_boolean(const struct TarwynClient *client,
                        const uint8_t *channel,
                        size_t channel_len,
                        bool *out);

uint8_t *tarwyn_get_bytes(const struct TarwynClient *client,
                          const uint8_t *channel,
                          size_t channel_len,
                          size_t *out_len);

/**
 * A frame of strings.
 */
uint8_t *tarwyn_get_string_list(const struct TarwynClient *client,
                                const uint8_t *channel,
                                size_t channel_len,
                                size_t *out_len);

/**
 * A frame of byte strings.
 */
uint8_t *tarwyn_get_bytes_list(const struct TarwynClient *client,
                               const uint8_t *channel,
                               size_t channel_len,
                               size_t *out_len);

/**
 * Packed doubles; `out_len` is in bytes.
 */
uint8_t *tarwyn_get_double_list(const struct TarwynClient *client,
                                const uint8_t *channel,
                                size_t channel_len,
                                size_t *out_len);

/**
 * Packed floats; `out_len` is in bytes.
 */
uint8_t *tarwyn_get_float_list(const struct TarwynClient *client,
                               const uint8_t *channel,
                               size_t channel_len,
                               size_t *out_len);

/**
 * Packed 32-bit integers; `out_len` is in bytes.
 */
uint8_t *tarwyn_get_integer_list(const struct TarwynClient *client,
                                 const uint8_t *channel,
                                 size_t channel_len,
                                 size_t *out_len);

/**
 * Packed 64-bit integers; `out_len` is in bytes.
 */
uint8_t *tarwyn_get_long_list(const struct TarwynClient *client,
                              const uint8_t *channel,
                              size_t channel_len,
                              size_t *out_len);

/**
 * One byte per boolean.
 */
uint8_t *tarwyn_get_boolean_list(const struct TarwynClient *client,
                                 const uint8_t *channel,
                                 size_t channel_len,
                                 size_t *out_len);

/**
 * Packed doubles as `x, y` pairs; `out_len` is in bytes.
 */
uint8_t *tarwyn_get_coordinates(const struct TarwynClient *client,
                                const uint8_t *channel,
                                size_t channel_len,
                                size_t *out_len);

/**
 * Packed doubles as `x, y, rotation_degrees` triples, `NaN` for no rotation;
 * `out_len` is in bytes.
 */
uint8_t *tarwyn_get_bezier_curve(const struct TarwynClient *client,
                                 const uint8_t *channel,
                                 size_t channel_len,
                                 size_t *out_len);

/**
 * An encoded protobuf `BezierCurves`.
 */
uint8_t *tarwyn_get_bezier_curves(const struct TarwynClient *client,
                                  const uint8_t *channel,
                                  size_t channel_len,
                                  size_t *out_len);

/**
 * An encoded protobuf `BezierCurvesList`.
 */
uint8_t *tarwyn_get_bezier_curves_list(const struct TarwynClient *client,
                                       const uint8_t *channel,
                                       size_t channel_len,
                                       size_t *out_len);

/**
 * Writes `x, y, rotation` (radians) to `out`.
 */
bool tarwyn_get_pose2d(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double *out);

/**
 * Writes `x, y, z, qw, qx, qy, qz` to `out`.
 */
bool tarwyn_get_pose3d(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len,
                       double *out);

uint8_t *tarwyn_get_unknown_bytes(const struct TarwynClient *client,
                                  const uint8_t *channel,
                                  size_t channel_len,
                                  size_t *out_len);

/**
 * How many channels were removed: 0 or 1.
 */
uint32_t tarwyn_delete(const struct TarwynClient *client,
                       const uint8_t *channel,
                       size_t channel_len);

uint32_t tarwyn_delete_all(const struct TarwynClient *client);

/**
 * A frame of channel names under `prefix`; empty rather than `NULL` when
 * there are none.
 */
uint8_t *tarwyn_get_tables(const struct TarwynClient *client,
                           const uint8_t *prefix,
                           size_t prefix_len,
                           size_t *out_len);

/**
 * The round trip to the server in nanoseconds.
 */
bool tarwyn_get_ping(const struct TarwynClient *client, uint64_t *out_nanos);

/**
 * Fills `out` and hands back the server's version string.
 */
bool tarwyn_get_server_statistics(const struct TarwynClient *client,
                                  struct TarwynStatistics *out,
                                  uint8_t **out_version,
                                  size_t *out_version_len);

/**
 * The JSON of everything under `prefix`; `{}` rather than `NULL` when the
 * server is absent.
 */
uint8_t *tarwyn_get_raw_json(const struct TarwynClient *client,
                             const uint8_t *prefix,
                             size_t prefix_len,
                             size_t *out_len);

bool tarwyn_compare_and_set_absent_string(const struct TarwynClient *client,
                                          const uint8_t *channel,
                                          size_t channel_len,
                                          const uint8_t *value,
                                          size_t value_len);

bool tarwyn_compare_and_set_string(const struct TarwynClient *client,
                                   const uint8_t *channel,
                                   size_t channel_len,
                                   const uint8_t *expected,
                                   size_t expected_len,
                                   const uint8_t *value,
                                   size_t value_len);

bool tarwyn_compare_and_set_double(const struct TarwynClient *client,
                                   const uint8_t *channel,
                                   size_t channel_len,
                                   double expected,
                                   double value);

bool tarwyn_compare_and_set_long(const struct TarwynClient *client,
                                 const uint8_t *channel,
                                 size_t channel_len,
                                 int64_t expected,
                                 int64_t value);

bool tarwyn_compare_and_set_boolean(const struct TarwynClient *client,
                                    const uint8_t *channel,
                                    size_t channel_len,
                                    bool expected,
                                    bool value);

void tarwyn_publish_telemetry(const struct TarwynClient *client,
                              const uint8_t *channel,
                              size_t channel_len,
                              const uint8_t *payload,
                              size_t payload_len);

bool tarwyn_log_to(const struct TarwynClient *client, const uint8_t *path, size_t path_len);

/**
 * The path the log landed at, or `NULL` when it could not be opened.
 */
uint8_t *tarwyn_log_to_drive(const struct TarwynClient *client,
                             const uint8_t *filename,
                             size_t filename_len,
                             size_t *out_len);

uint64_t tarwyn_dropped_log_records(const struct TarwynClient *client);

bool tarwyn_logging_healthy(const struct TarwynClient *client);

uint64_t tarwyn_dropped_publishes(const struct TarwynClient *client);

/**
 * False when `channel` already has a subscription, in which case `drop` has
 * already run.
 */
bool tarwyn_subscribe(const struct TarwynClient *client,
                      const uint8_t *channel,
                      size_t channel_len,
                      TarwynSampleFn callback,
                      void *ctx,
                      TarwynDropFn drop);

bool tarwyn_unsubscribe(const struct TarwynClient *client,
                        const uint8_t *channel,
                        size_t channel_len);

/**
 * False when `channel` already has a subscription or the telemetry plane
 * refused it, in which case `drop` has already run.
 */
bool tarwyn_subscribe_telemetry(const struct TarwynClient *client,
                                const uint8_t *channel,
                                size_t channel_len,
                                TarwynTelemetryFn callback,
                                void *ctx,
                                TarwynDropFn drop);

bool tarwyn_unsubscribe_telemetry(const struct TarwynClient *client,
                                  const uint8_t *channel,
                                  size_t channel_len);

/**
 * Lines arrive on the channel `logs`. False when logs are already subscribed,
 * in which case `drop` has already run.
 */
bool tarwyn_subscribe_to_logs(const struct TarwynClient *client,
                              TarwynSampleFn callback,
                              void *ctx,
                              TarwynDropFn drop);

bool tarwyn_unsubscribe_from_logs(const struct TarwynClient *client);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* TARWYN_H */
