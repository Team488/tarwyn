/* Generated from src/lib.rs by cbindgen. Do not edit. */

#ifndef TARWYN_H
#define TARWYN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define XT_OK 0

#define XT_ERR_NULL -1

#define XT_ERR_UTF8 -2

#define XT_ERR_NO_VALUE -3

#define XT_ERR_WRONG_TYPE -4

#define XT_ERR_PANIC -5

#define XT_ERR_IO -6

typedef struct Handle Handle;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

struct Handle *xt_client_new(const char *host,
                             uint16_t push_port,
                             uint16_t req_port,
                             uint16_t sub_port,
                             uint64_t request_timeout_ms,
                             int send_high_water_mark);

int xt_client_start(struct Handle *handle);

void xt_client_free(struct Handle *handle);

int xt_dropped_publishes(const struct Handle *handle, uint64_t *out);

int xt_log_to(const struct Handle *handle, const char *path);

int xt_log_to_drive(const struct Handle *handle,
                    const char *filename,
                    char *out_path,
                    size_t out_len);

int xt_log_dropped(const struct Handle *handle, uint64_t *out);

int xt_logging_healthy(const struct Handle *handle, bool *out);

int xt_publish_double(const struct Handle *handle, const char *channel, double value);

int xt_publish_float(const struct Handle *handle, const char *channel, float value);

int xt_publish_int32(const struct Handle *handle, const char *channel, int32_t value);

int xt_publish_int64(const struct Handle *handle, const char *channel, int64_t value);

int xt_publish_bool(const struct Handle *handle, const char *channel, bool value);

int xt_publish_string(const struct Handle *handle, const char *channel, const char *value);

int xt_publish_bytes(const struct Handle *handle,
                     const char *channel,
                     const uint8_t *value,
                     size_t len);

int xt_subscribe_ring(struct Handle *handle,
                      const char *channel,
                      size_t records,
                      size_t record_bytes,
                      uint64_t *out_id);

int xt_unsubscribe(struct Handle *handle, uint64_t id);

void *xt_ring_base(const struct Handle *handle, uint64_t id);

int xt_ring_write_index(const struct Handle *handle, uint64_t id, uint64_t *out);

int xt_put_string(const struct Handle *handle, const char *channel, const char *value);

int xt_put_integer(const struct Handle *handle, const char *channel, int32_t value);

int xt_put_long(const struct Handle *handle, const char *channel, int64_t value);

int xt_put_double(const struct Handle *handle, const char *channel, double value);

int xt_put_float(const struct Handle *handle, const char *channel, float value);

int xt_put_boolean(const struct Handle *handle, const char *channel, bool value);

int xt_get_string(const struct Handle *handle, const char *channel, char *out, size_t out_len);

int xt_get_integer(const struct Handle *handle, const char *channel, int32_t *out);

int xt_get_long(const struct Handle *handle, const char *channel, int64_t *out);

int xt_get_double(const struct Handle *handle, const char *channel, double *out);

int xt_get_float(const struct Handle *handle, const char *channel, float *out);

int xt_get_boolean(const struct Handle *handle, const char *channel, bool *out);

int xt_get_pose2d(const struct Handle *handle, const char *channel, double *out);

int xt_get_pose3d(const struct Handle *handle, const char *channel, double *out);

int xt_put_pose2d(const struct Handle *handle, const char *channel, const double *values);

int xt_put_pose3d(const struct Handle *handle, const char *channel, const double *values);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* TARWYN_H */
