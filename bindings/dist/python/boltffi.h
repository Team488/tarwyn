#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int32_t code;
} FfiStatus;

#define FFI_STATUS_OK ((FfiStatus){0})
#define FFI_STATUS_NULL_POINTER ((FfiStatus){1})
#define FFI_STATUS_BUFFER_TOO_SMALL ((FfiStatus){2})
#define FFI_STATUS_INVALID_ARG ((FfiStatus){3})
#define FFI_STATUS_CANCELLED ((FfiStatus){4})
#define FFI_STATUS_INTERNAL_ERROR ((FfiStatus){100})

typedef struct {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t cap;
    uintptr_t align;
} FfiBuf_u8;

typedef struct {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t cap;
} FfiString;

typedef struct {
    FfiString message;
} FfiError;

typedef struct {
    const uint8_t *ptr;
    uintptr_t len;
} FfiSpan;

typedef const void *RustFutureHandle;
typedef int8_t StreamPollResult;
typedef int32_t WaitResult;
typedef void (*RustFutureContinuationCallback)(uint64_t callback_data, int8_t poll_result);
typedef void (*StreamContinuationCallback)(uint64_t callback_data, StreamPollResult result);

static inline bool boltffi_atomic_u8_cas(uint8_t *state, uint8_t expected, uint8_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t *)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}

static inline uint64_t boltffi_atomic_u64_exchange(uint64_t *slot, uint64_t value) {
    return atomic_exchange_explicit((_Atomic uint64_t *)slot, value, memory_order_acq_rel);
}

static inline bool boltffi_atomic_u64_cas(uint64_t *slot, uint64_t expected, uint64_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint64_t *)slot, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}

static inline uint64_t boltffi_atomic_u64_load(uint64_t *slot) {
    return atomic_load_explicit((_Atomic uint64_t *)slot, memory_order_acquire);
}

typedef struct {
    uint64_t handle;
    const void *vtable;
} BoltFFICallbackHandle;

void boltffi_free_string(FfiString string);
void boltffi_free_buf(FfiBuf_u8 buf);
FfiBuf_u8 boltffi_buf_from_bytes(const uint8_t *ptr, uintptr_t len);
FfiBuf_u8 boltffi_buf_with_len(uintptr_t len);
FfiStatus boltffi_last_error_message(FfiString *out);
void boltffi_clear_last_error(void);
typedef struct {
    double x;
    double y;
} ___Coordinate;
typedef struct {
    double x;
    double y;
    double rotation;
} ___Pose2d;
typedef struct {
    double x;
    double y;
    double z;
    double roll;
    double pitch;
    double yaw;
} ___Pose3d;
void boltffi_release_class_tarwyn_bindings_x_tables_client(uint64_t handle);
uint64_t boltffi_init_class_tarwyn_bindings_x_tables_client_new(void);
uint64_t boltffi_init_class_tarwyn_bindings_x_tables_client_connect(const uint8_t *host_ptr, uintptr_t host_len);
uint64_t boltffi_init_class_tarwyn_bindings_x_tables_client_with_ports(const uint8_t *host_ptr, uintptr_t host_len, uint16_t push_port, uint16_t req_port, uint16_t sub_port, uint16_t telemetry_port, uint64_t request_timeout_ms, int32_t send_high_water_mark);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_start(uint64_t receiver);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_stop(uint64_t receiver);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_string(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, int32_t value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_long(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, int64_t value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_double(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, double value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_float(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, float value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, bool value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_string_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_double_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const double *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_float_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const float *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const int32_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_long_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const int64_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const bool *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_coordinates(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_byte_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose2d(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, ___Pose2d value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose3d(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, ___Pose3d value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curve(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_put_unknown_bytes(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_put_typed_bytes(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, int32_t tarwyn_type, const uint8_t *value_ptr, uintptr_t value_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_string(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_long(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_double(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_float(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_string_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_double_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_float_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_long_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_coordinates(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose2d(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose3d(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curve(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves_list(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_unknown_bytes(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
uint32_t boltffi_method_class_tarwyn_bindings_x_tables_client_delete(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
uint32_t boltffi_method_class_tarwyn_bindings_x_tables_client_delete_all(uint64_t receiver);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_tables(uint64_t receiver, const uint8_t *prefix_ptr, uintptr_t prefix_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_ping(uint64_t receiver);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_server_statistics(uint64_t receiver);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_get_raw_json(uint64_t receiver, const uint8_t *prefix_ptr, uintptr_t prefix_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_absent_string(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *value_ptr, uintptr_t value_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_string(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *expected_ptr, uintptr_t expected_len, const uint8_t *value_ptr, uintptr_t value_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_double(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, double expected, double value);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_long(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, int64_t expected, int64_t value);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_boolean(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, bool expected, bool value);
FfiStatus boltffi_method_class_tarwyn_bindings_x_tables_client_publish_telemetry(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len, const uint8_t *payload_ptr, uintptr_t payload_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_log_to(uint64_t receiver, const uint8_t *path_ptr, uintptr_t path_len);
FfiBuf_u8 boltffi_method_class_tarwyn_bindings_x_tables_client_log_to_drive(uint64_t receiver, const uint8_t *filename_ptr, uintptr_t filename_len);
uint64_t boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_log_records(uint64_t receiver);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_logging_healthy(uint64_t receiver);
uint64_t boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_publishes(uint64_t receiver);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_telemetry(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_telemetry(uint64_t receiver, const uint8_t *channel_ptr, uintptr_t channel_len);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_to_logs(uint64_t receiver);
bool boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_from_logs(uint64_t receiver);
uint64_t boltffi_stream_tarwyn_bindings_x_tables_client_updates_subscribe(uint64_t receiver);
FfiBuf_u8 boltffi_stream_tarwyn_bindings_x_tables_client_updates_pop_batch(uint64_t subscription, uintptr_t max_count);
WaitResult boltffi_stream_tarwyn_bindings_x_tables_client_updates_wait(uint64_t subscription, uint32_t timeout_milliseconds);
void boltffi_stream_tarwyn_bindings_x_tables_client_updates_poll(uint64_t subscription, uint64_t callback_data, void (*callback)(uint64_t, StreamPollResult));
void boltffi_stream_tarwyn_bindings_x_tables_client_updates_unsubscribe(uint64_t subscription);
void boltffi_stream_tarwyn_bindings_x_tables_client_updates_free(uint64_t subscription);
uint64_t boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_subscribe(uint64_t receiver);
FfiBuf_u8 boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_pop_batch(uint64_t subscription, uintptr_t max_count);
WaitResult boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_wait(uint64_t subscription, uint32_t timeout_milliseconds);
void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_poll(uint64_t subscription, uint64_t callback_data, void (*callback)(uint64_t, StreamPollResult));
void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_unsubscribe(uint64_t subscription);
void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_free(uint64_t subscription);
uint64_t boltffi_stream_tarwyn_bindings_x_tables_client_logs_subscribe(uint64_t receiver);
FfiBuf_u8 boltffi_stream_tarwyn_bindings_x_tables_client_logs_pop_batch(uint64_t subscription, uintptr_t max_count);
WaitResult boltffi_stream_tarwyn_bindings_x_tables_client_logs_wait(uint64_t subscription, uint32_t timeout_milliseconds);
void boltffi_stream_tarwyn_bindings_x_tables_client_logs_poll(uint64_t subscription, uint64_t callback_data, void (*callback)(uint64_t, StreamPollResult));
void boltffi_stream_tarwyn_bindings_x_tables_client_logs_unsubscribe(uint64_t subscription);
void boltffi_stream_tarwyn_bindings_x_tables_client_logs_free(uint64_t subscription);

#ifdef __cplusplus
}
#endif