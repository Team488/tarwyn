#ifndef TARWYN_H
#define TARWYN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define XT_OK               0
#define XT_ERR_NULL        -1
#define XT_ERR_UTF8        -2
#define XT_ERR_NO_VALUE    -3
#define XT_ERR_WRONG_TYPE  -4
#define XT_ERR_PANIC       -5

typedef struct Handle Handle;

Handle *xt_client_new(const char *host,
                      uint16_t push_port,
                      uint16_t req_port,
                      uint16_t sub_port,
                      uint64_t request_timeout_ms,
                      int send_high_water_mark);

int  xt_client_start(Handle *handle);
void xt_client_free(Handle *handle);

int xt_dropped_publishes(const Handle *handle, uint64_t *out);

int xt_publish_double(const Handle *handle, const char *channel, double value);
int xt_publish_float(const Handle *handle, const char *channel, float value);
int xt_publish_int32(const Handle *handle, const char *channel, int32_t value);
int xt_publish_int64(const Handle *handle, const char *channel, int64_t value);
int xt_publish_bool(const Handle *handle, const char *channel, _Bool value);
int xt_publish_string(const Handle *handle, const char *channel, const char *value);
int xt_publish_bytes(const Handle *handle, const char *channel, const uint8_t *value, size_t len);

int xt_get_double(const Handle *handle, const char *channel, double *out);

int xt_subscribe_ring(Handle *handle,
                      const char *channel,
                      size_t records,
                      size_t record_bytes,
                      uint64_t *out_id);

int   xt_unsubscribe(Handle *handle, uint64_t id);
void *xt_ring_base(const Handle *handle, uint64_t id);
int   xt_ring_write_index(const Handle *handle, uint64_t id, uint64_t *out);

#ifdef __cplusplus
}
#endif

#endif
