/* Generated from src/lib.rs by cbindgen. Do not edit. */

#ifndef TARWYN_H
#define TARWYN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/**
 * The call succeeded.
 */
#define XT_OK 0

/**
 * A required pointer was null, or an argument was out of range.
 */
#define XT_ERR_NULL -1

/**
 * A string argument was not valid UTF-8.
 */
#define XT_ERR_UTF8 -2

/**
 * The channel holds nothing, or the server did not answer.
 */
#define XT_ERR_NO_VALUE -3

/**
 * The channel holds a value of a different type.
 */
#define XT_ERR_WRONG_TYPE -4

/**
 * Rust panicked. The panic was caught at the boundary, not unwound into C.
 */
#define XT_ERR_PANIC -5

/**
 * A filesystem operation failed.
 */
#define XT_ERR_IO -6

/**
 * An opaque client, owned by the caller between [`xt_client_new`] and
 * [`xt_client_free`].
 */
typedef struct Handle Handle;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Construct a client. Returns null if `host` is null or not UTF-8.
 *
 * Connecting never blocks: ZeroMQ dials in the background, so this succeeds
 * before the server exists. Nothing is received until [`xt_client_start`].
 * The result must be released with [`xt_client_free`].
 *
 * # Safety
 *
 * `host` must point at a NUL-terminated UTF-8 string.
 */
struct Handle *xt_client_new(const char *host,
                             uint16_t push_port,
                             uint16_t req_port,
                             uint16_t sub_port,
                             uint64_t request_timeout_ms,
                             int send_high_water_mark);

/**
 * Start the receive threads, so subscriptions begin delivering.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 */
int xt_client_start(struct Handle *handle);

/**
 * Stop the client, drop every subscription, and release the handle.
 *
 * Null is accepted and ignored. Any ring pointer from [`xt_ring_base`] dangles
 * after this returns.
 *
 * # Safety
 *
 * `handle` must be null, or a handle from [`xt_client_new`] that has not already
 * been freed. It must not be used again afterwards.
 */
void xt_client_free(struct Handle *handle);

/**
 * Write out how many publishes were dropped rather than queued.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out` must be writable.
 */
int xt_dropped_publishes(const struct Handle *handle, uint64_t *out);

/**
 * Begin mirroring published values into a WPILOG file at `path`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `path` must point at a NUL-terminated UTF-8 string.
 */
int xt_log_to(const struct Handle *handle, const char *path);

/**
 * Begin logging onto the first writable removable drive that accepts the file,
 * writing the chosen path into `out_path` as a NUL-terminated string.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `filename` must point at a NUL-terminated UTF-8 string, and `out_path` must be
 * writable for `out_len` bytes.
 */
int xt_log_to_drive(const struct Handle *handle,
                    const char *filename,
                    char *out_path,
                    size_t out_len);

/**
 * Write out how many log records were dropped because the queue was full.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out` must be writable.
 */
int xt_log_dropped(const struct Handle *handle, uint64_t *out);

/**
 * Write out whether the log writer is still succeeding. `true` when logging was
 * never started.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out` must be writable.
 */
int xt_logging_healthy(const struct Handle *handle, bool *out);

/**
 * Publish a double to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_double(const struct Handle *handle, const char *channel, double value);

/**
 * Publish a float to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_float(const struct Handle *handle, const char *channel, float value);

/**
 * Publish a 32-bit integer to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_int32(const struct Handle *handle, const char *channel, int32_t value);

/**
 * Publish a 64-bit integer to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_int64(const struct Handle *handle, const char *channel, int64_t value);

/**
 * Publish a boolean to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_bool(const struct Handle *handle, const char *channel, bool value);

/**
 * Publish a string to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must point at a NUL-terminated UTF-8 string.
 */
int xt_publish_string(const struct Handle *handle, const char *channel, const char *value);

/**
 * Publish raw bytes to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must be readable for `len` bytes.
 */
int xt_publish_bytes(const struct Handle *handle,
                     const char *channel,
                     const uint8_t *value,
                     size_t len);

/**
 * Read the bytes on `channel` into `out`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_get_bytes(const struct Handle *handle,
                 const char *channel,
                 uint8_t *out,
                 size_t capacity,
                 size_t *out_len);

/**
 * Publish a list of `(x, y)` coordinates to `channel`.
 *
 * `values` is flat: `count` pairs, so `count * 2` doubles.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `values` must be readable for `count * 2` doubles.
 */
int xt_put_coordinates(const struct Handle *handle,
                       const char *channel,
                       const double *values,
                       size_t count);

/**
 * Read the coordinate list on `channel` into `out`, flat — `x`, `y`, `x`, `y`.
 *
 * `out_len` receives the number of doubles, which is twice the number of pairs.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable for `capacity` doubles, and `out_len` null or writable.
 */
int xt_get_coordinates(const struct Handle *handle,
                       const char *channel,
                       double *out,
                       size_t capacity,
                       size_t *out_len);

/**
 * Publish a bezier path to `channel`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must be readable for `len` bytes.
 */
int xt_put_bezier_curves(const struct Handle *handle,
                         const char *channel,
                         const uint8_t *encoded,
                         size_t encoded_len);

/**
 * Read the bezier path on `channel` into `out`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_get_bezier_curves(const struct Handle *handle,
                         const char *channel,
                         uint8_t *out,
                         size_t capacity,
                         size_t *out_len);

/**
 * Publish one bezier curve to `channel`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must be readable for `len` bytes.
 */
int xt_put_bezier_curve(const struct Handle *handle,
                        const char *channel,
                        const uint8_t *encoded,
                        size_t encoded_len);

/**
 * Read the bezier curve on `channel` into `out`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_get_bezier_curve(const struct Handle *handle,
                        const char *channel,
                        uint8_t *out,
                        size_t capacity,
                        size_t *out_len);

/**
 * Publish a list of bezier paths to `channel`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must be readable for `len` bytes.
 */
int xt_put_bezier_curves_list(const struct Handle *handle,
                              const char *channel,
                              const uint8_t *encoded,
                              size_t encoded_len);

/**
 * Read the list of bezier paths on `channel` into `out`, as encoded protobuf.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_get_bezier_curves_list(const struct Handle *handle,
                              const char *channel,
                              uint8_t *out,
                              size_t capacity,
                              size_t *out_len);

/**
 * Publish a value already encoded in TARWYN' own byte layout.
 *
 * `tarwyn_type` is TARWYN' type tag. Returns [`XT_ERR_WRONG_TYPE`], publishing
 * nothing, if the tag is unknown or the bytes do not decode as that type.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `value` must be readable for `len` bytes.
 */
int xt_put_typed_bytes(const struct Handle *handle,
                       const char *channel,
                       int tarwyn_type,
                       const uint8_t *value,
                       size_t len);

/**
 * Delete `channel`, writing out how many were removed. Pass `""` to delete all.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out` must be null or writable.
 */
int xt_delete(const struct Handle *handle, const char *channel, uint32_t *out);

/**
 * Write the channel names beginning with `prefix` into `out`, packed.
 *
 * Pass `""` for all of them. See the module docs for the packed layout.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `prefix` must point at a NUL-terminated UTF-8 string, `out` must be null or
 * writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_tables(const struct Handle *handle,
              const char *prefix,
              uint8_t *out,
              size_t capacity,
              size_t *out_len);

/**
 * Write out the round-trip time to the server, in nanoseconds.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out_nanos` must be writable.
 */
int xt_ping(const struct Handle *handle, uint64_t *out_nanos);

/**
 * Write the server's counters into `out`, and its version into `version` as a
 * NUL-terminated string.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out` must be writable for `capacity` values, and `version` null or writable
 * for `version_len` bytes.
 */
int xt_statistics(const struct Handle *handle,
                  uint64_t *out,
                  size_t capacity,
                  char *version,
                  size_t version_len);

/**
 * Write the channels beginning with `prefix` into `out` as a NUL-terminated JSON
 * document.
 *
 * `out_len` receives the length including the terminator, so a null `out` sizes
 * the buffer.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `prefix` must point at a NUL-terminated UTF-8 string, `out` must be null or
 * writable for `capacity` bytes, and `out_len` null or writable.
 */
int xt_raw_json(const struct Handle *handle,
                const char *prefix,
                char *out,
                size_t capacity,
                size_t *out_len);

/**
 * Subscribe to `channel`, delivering payloads into a ring the caller reads directly.
 *
 * Writes the subscription id into `out_id`. `records` must be non-zero and
 * `record_bytes` greater than 8, since each slot carries an 8-byte length ahead
 * of its payload. Read the bytes through [`xt_ring_base`], bounded by
 * [`xt_ring_write_index`].
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `channel` must point at a NUL-terminated UTF-8 string.
 * `out_id` must be writable.
 */
int xt_subscribe_ring(struct Handle *handle,
                      const char *channel,
                      size_t records,
                      size_t record_bytes,
                      uint64_t *out_id);

/**
 * Cancel a subscription and release its ring, invalidating any pointer
 * [`xt_ring_base`] returned for it.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * No ring pointer for `id` may be used afterwards.
 */
int xt_unsubscribe(struct Handle *handle, uint64_t id);

/**
 * The base address of a subscription's ring, or null if `id` is unknown.
 *
 * Valid until the subscription is cancelled or the client freed. Slot `n` starts
 * at `(n % records) * record_bytes` and begins with its payload length as a
 * little-endian `u64`.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * The returned pointer is valid for `records * record_bytes` bytes, and only
 * until [`xt_unsubscribe`] or [`xt_client_free`].
 */
void *xt_ring_base(const struct Handle *handle, uint64_t id);

/**
 * Write out how many records have been pushed to a subscription's ring.
 *
 * Loaded with `Acquire`, so every slot below the returned index is fully written.
 * An index more than `records` ahead of what the reader last saw means the writer
 * lapped it and those slots were overwritten.
 *
 * # Safety
 *
 * `handle` must be a live handle returned by [`xt_client_new`] and not yet
 * passed to [`xt_client_free`].
 * `out` must be writable.
 */
int xt_ring_write_index(const struct Handle *handle, uint64_t id, uint64_t *out);

/**
 * Publish a string to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_string(const struct Handle *handle, const char *channel, const char *value);

/**
 * Publish an integer to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_integer(const struct Handle *handle, const char *channel, int32_t value);

/**
 * Publish a long to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_long(const struct Handle *handle, const char *channel, int64_t value);

/**
 * Publish a double to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_double(const struct Handle *handle, const char *channel, double value);

/**
 * Publish a float to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_float(const struct Handle *handle, const char *channel, float value);

/**
 * Publish a boolean to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_boolean(const struct Handle *handle, const char *channel, bool value);

/**
 * Read a string from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_string(const struct Handle *handle, const char *channel, char *out, size_t out_len);

/**
 * Read an integer from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_integer(const struct Handle *handle, const char *channel, int32_t *out);

/**
 * Read a long from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_long(const struct Handle *handle, const char *channel, int64_t *out);

/**
 * Read a double from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_double(const struct Handle *handle, const char *channel, double *out);

/**
 * Read a float from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_float(const struct Handle *handle, const char *channel, float *out);

/**
 * Read a boolean from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_boolean(const struct Handle *handle, const char *channel, bool *out);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes a string.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_string(const struct Handle *handle,
                              const char *channel,
                              const char *expected,
                              bool has_expected,
                              const char *value,
                              bool *out_swapped);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes an integer.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_integer(const struct Handle *handle,
                               const char *channel,
                               int32_t expected,
                               bool has_expected,
                               int32_t value,
                               bool *out_swapped);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes a long.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_long(const struct Handle *handle,
                            const char *channel,
                            int64_t expected,
                            bool has_expected,
                            int64_t value,
                            bool *out_swapped);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes a double.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_double(const struct Handle *handle,
                              const char *channel,
                              double expected,
                              bool has_expected,
                              double value,
                              bool *out_swapped);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes a float.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_float(const struct Handle *handle,
                             const char *channel,
                             float expected,
                             bool has_expected,
                             float value,
                             bool *out_swapped);

/**
 * Set `channel` to `value` only if it currently holds `expected`, writing
 * out whether it swapped. Takes a boolean.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_compare_and_set_boolean(const struct Handle *handle,
                               const char *channel,
                               bool expected,
                               bool has_expected,
                               bool value,
                               bool *out_swapped);

/**
 * Publish a list of strings to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_string_list(const struct Handle *handle,
                       const char *channel,
                       const uint8_t *packed,
                       size_t packed_len);

/**
 * Read a list of strings from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_string_list(const struct Handle *handle,
                       const char *channel,
                       uint8_t *out,
                       size_t capacity,
                       size_t *out_len);

/**
 * Publish a list of byte arrays to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_bytes_list(const struct Handle *handle,
                      const char *channel,
                      const uint8_t *packed,
                      size_t packed_len);

/**
 * Read a list of byte arrays from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_bytes_list(const struct Handle *handle,
                      const char *channel,
                      uint8_t *out,
                      size_t capacity,
                      size_t *out_len);

/**
 * Publish a list of doubles to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_double_list(const struct Handle *handle,
                       const char *channel,
                       const double *values,
                       size_t count);

/**
 * Read a list of doubles from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_double_list(const struct Handle *handle,
                       const char *channel,
                       double *out,
                       size_t capacity,
                       size_t *out_len);

/**
 * Publish a list of floats to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_float_list(const struct Handle *handle,
                      const char *channel,
                      const float *values,
                      size_t count);

/**
 * Read a list of floats from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_float_list(const struct Handle *handle,
                      const char *channel,
                      float *out,
                      size_t capacity,
                      size_t *out_len);

/**
 * Publish a list of integers to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_integer_list(const struct Handle *handle,
                        const char *channel,
                        const int32_t *values,
                        size_t count);

/**
 * Read a list of integers from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_integer_list(const struct Handle *handle,
                        const char *channel,
                        int32_t *out,
                        size_t capacity,
                        size_t *out_len);

/**
 * Publish a list of longs to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_long_list(const struct Handle *handle,
                     const char *channel,
                     const int64_t *values,
                     size_t count);

/**
 * Read a list of longs from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_long_list(const struct Handle *handle,
                     const char *channel,
                     int64_t *out,
                     size_t capacity,
                     size_t *out_len);

/**
 * Publish a list of booleans to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_boolean_list(const struct Handle *handle,
                        const char *channel,
                        const bool *values,
                        size_t count);

/**
 * Read a list of booleans from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_boolean_list(const struct Handle *handle,
                        const char *channel,
                        bool *out,
                        size_t capacity,
                        size_t *out_len);

/**
 * Read a Pose2d from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_pose2d(const struct Handle *handle, const char *channel, double *out);

/**
 * Read a Pose3d from `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_get_pose3d(const struct Handle *handle, const char *channel, double *out);

/**
 * Publish a Pose2d to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_pose2d(const struct Handle *handle, const char *channel, const double *values);

/**
 * Publish a Pose3d to `channel`.
 *
 * # Safety
 *
 * `handle` must be a live handle from `xt_client_new`, `channel` must point at
 * a NUL-terminated UTF-8 string, and every other pointer must be null or valid
 * for the length it is passed with. See the crate docs for the out-buffer and
 * packing conventions.
 */
int xt_put_pose3d(const struct Handle *handle, const char *channel, const double *values);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* TARWYN_H */
