#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <limits.h>
#include <string.h>
#include <stdlib.h>
#if defined(__ANDROID__)
#include <pthread.h>
#endif

#include "tarwyn_bindings.h"
static JavaVM *boltffi_jni_vm = NULL;
static jclass boltffi_jni_native_class = NULL;

#define BOLTFFI_JNI_LOCAL_FRAME_CAPACITY 64
static jint boltffi_jni_attach_current_thread(JavaVM *vm, JNIEnv **env) {
#if defined(__ANDROID__)
    return (*vm)->AttachCurrentThread(vm, env, NULL);
#else
    return (*vm)->AttachCurrentThread(vm, (void **)env, NULL);
#endif
}
#if defined(__ANDROID__)
static pthread_key_t boltffi_jni_env_key;
static pthread_once_t boltffi_jni_env_key_once = PTHREAD_ONCE_INIT;
static int boltffi_jni_env_key_status = 0;
static char boltffi_jni_tls_attached_marker;

static void boltffi_jni_android_env_destructor(void *value) {
    if (value != NULL && boltffi_jni_vm != NULL) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

static void boltffi_jni_android_env_key_init(void) {
    boltffi_jni_env_key_status =
        pthread_key_create(&boltffi_jni_env_key, boltffi_jni_android_env_destructor);
}

static jint boltffi_jni_android_attach_cached(JavaVM *vm, JNIEnv **env, int *attached) {
    *attached = 0;

    if (pthread_once(&boltffi_jni_env_key_once, boltffi_jni_android_env_key_init) != 0 ||
        boltffi_jni_env_key_status != 0) {
        jint result = boltffi_jni_attach_current_thread(vm, env);
        if (result == JNI_OK) {
            *attached = 1;
        }
        return result;
    }

    jint result = (*vm)->AttachCurrentThreadAsDaemon(vm, env, NULL);
    if (result != JNI_OK) {
        return result;
    }

    if (pthread_setspecific(boltffi_jni_env_key, &boltffi_jni_tls_attached_marker) != 0) {
        (*vm)->DetachCurrentThread(vm);
        *env = NULL;
        return JNI_ERR;
    }

    return JNI_OK;
}
#endif

static inline bool boltffi_jni_clear_exception(JNIEnv *env) {
    if (!(*env)->ExceptionCheck(env)) {
        return false;
    }
    (*env)->ExceptionClear(env);
    return true;
}

static void boltffi_jni_describe_load_exception(JNIEnv *env) {
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionDescribe(env);
        (*env)->ExceptionClear(env);
    }
}

static bool boltffi_jni_report_class_load_failure(JNIEnv *env, const char *message, const char *diagnostic_class_name) {
    fprintf(stderr, "BoltFFI JNI_OnLoad failed: %s '%s'\n", message, diagnostic_class_name);
    boltffi_jni_describe_load_exception(env);
    return false;
}

static bool boltffi_jni_report_static_method_load_failure(JNIEnv *env, const char *diagnostic_class_name, const char *diagnostic_method_name, const char *diagnostic_signature) {
    fprintf(stderr, "BoltFFI JNI_OnLoad failed: could not resolve static method %s.%s%s\n", diagnostic_class_name, diagnostic_method_name, diagnostic_signature);
    boltffi_jni_describe_load_exception(env);
    return false;
}

static bool boltffi_jni_lookup_global_class_with_diagnostic(JNIEnv *env, const char *lookup_class_name, const char *diagnostic_class_name, jclass *out_class) {
    *out_class = NULL;
    jclass local_class = (*env)->FindClass(env, lookup_class_name);
    if (local_class == NULL) {
        return boltffi_jni_report_class_load_failure(env, "could not find JVM class", diagnostic_class_name);
    }
    jclass global_class = (*env)->NewGlobalRef(env, local_class);
    (*env)->DeleteLocalRef(env, local_class);
    if (global_class == NULL) {
        return boltffi_jni_report_class_load_failure(env, "could not create global reference for JVM class", diagnostic_class_name);
    }
    *out_class = global_class;
    return true;
}

static bool boltffi_jni_lookup_static_method_with_diagnostic(JNIEnv *env, jclass cls, const char *diagnostic_class_name, const char *lookup_method_name, const char *diagnostic_method_name, const char *lookup_signature, const char *diagnostic_signature, jmethodID *out_method) {
    *out_method = (*env)->GetStaticMethodID(env, cls, lookup_method_name, lookup_signature);
    if (*out_method == NULL) {
        return boltffi_jni_report_static_method_load_failure(env, diagnostic_class_name, diagnostic_method_name, diagnostic_signature);
    }
    return true;
}

static inline bool boltffi_jni_enter(JNIEnv **env, int *attached) {
    if (boltffi_jni_vm == NULL) {
        return false;
    }
    *env = NULL;
    *attached = 0;
    jint env_status = (*boltffi_jni_vm)->GetEnv(boltffi_jni_vm, (void **)env, JNI_VERSION_1_6);
    if (env_status == JNI_EDETACHED) {
#if defined(__ANDROID__)
        if (boltffi_jni_android_attach_cached(boltffi_jni_vm, env, attached) != JNI_OK) {
            return false;
        }
#else
        if (boltffi_jni_attach_current_thread(boltffi_jni_vm, env) != JNI_OK) {
            return false;
        }
        *attached = 1;
#endif
    } else if (env_status != JNI_OK) {
        return false;
    }

#if defined(__ANDROID__)
    JNIEnv *callback_env = *env;
    if ((*callback_env)->PushLocalFrame(callback_env, BOLTFFI_JNI_LOCAL_FRAME_CAPACITY) != JNI_OK) {
        boltffi_jni_clear_exception(callback_env);
        if (*attached) {
            (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
            *attached = 0;
        }
        return false;
    }
#endif

    return true;
}

static inline void boltffi_jni_exit(JNIEnv *env, int attached) {
#if defined(__ANDROID__)
    if (env != NULL) {
        (*env)->PopLocalFrame(env, NULL);
        boltffi_jni_clear_exception(env);
    }
#else
    (void)env;
#endif
    if (attached) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

static jmethodID boltffi_jni_continuation_method = NULL;

static bool boltffi_jni_continuation_load(JNIEnv *env) {
    return boltffi_jni_lookup_static_method_with_diagnostic(env, boltffi_jni_native_class, "org/tarwyn/Native", "boltffiFutureContinuationCallback", "boltffiFutureContinuationCallback", "(JB)V", "(JB)V", &boltffi_jni_continuation_method);
}

static void boltffi_jni_continuation_unload(JNIEnv *env) {
    (void)env;
    boltffi_jni_continuation_method = NULL;
}

static void boltffi_jni_continuation_callback(uint64_t handle, int8_t poll_result) {
    if (boltffi_jni_vm == NULL || boltffi_jni_native_class == NULL || boltffi_jni_continuation_method == NULL) {
        return;
    }
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
        return;
    }
    (*env)->CallStaticVoidMethod(env, boltffi_jni_native_class, boltffi_jni_continuation_method, (jlong)handle, (jbyte)poll_result);
    boltffi_jni_clear_exception(env);
    boltffi_jni_exit(env, attached);
}

static void boltffi_jni_throw_runtime(JNIEnv *env, const char *message) {
    jclass exception_class = (*env)->FindClass(env, "java/lang/RuntimeException");
    if (exception_class == NULL) {
        return;
    }
    (*env)->ThrowNew(env, exception_class, message);
    (*env)->DeleteLocalRef(env, exception_class);
}

static void boltffi_jni_throw_illegal_argument(JNIEnv *env, const char *message) {
    jclass exception_class = (*env)->FindClass(env, "java/lang/IllegalArgumentException");
    if (exception_class == NULL) {
        return;
    }
    (*env)->ThrowNew(env, exception_class, message);
    (*env)->DeleteLocalRef(env, exception_class);
}

static void boltffi_jni_throw_status(JNIEnv *env, FfiStatus status) {
    if (status.code != 0) {
        boltffi_jni_throw_runtime(env, "BoltFFI call failed");
    }
}

static jbyteArray boltffi_jni_buffer_to_byte_array(JNIEnv *env, FfiBuf_u8 buffer) {
    if (buffer.ptr == NULL) {
        if (buffer.len != 0) {
            boltffi_jni_throw_runtime(env, "BoltFFI buffer pointer was null with non-zero length");
            return NULL;
        }
        return (*env)->NewByteArray(env, 0);
    }
    if (buffer.len > (uintptr_t)INT32_MAX) {
        boltffi_free_buf(buffer);
        boltffi_jni_throw_runtime(env, "BoltFFI buffer too large for Java byte array");
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)buffer.len);
    if (array == NULL) {
        boltffi_free_buf(buffer);
        return NULL;
    }
    (*env)->SetByteArrayRegion(env, array, 0, (jsize)buffer.len, (const jbyte *)buffer.ptr);
    boltffi_free_buf(buffer);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->DeleteLocalRef(env, array);
        return NULL;
    }
    return array;
}

static inline jbyteArray boltffi_jni_bytes_to_byte_array(JNIEnv *env, const uint8_t *bytes, uintptr_t len) {
    if (bytes == NULL && len != 0) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte slice pointer was null with non-zero length");
        return NULL;
    }
    if (len > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte slice too large for Java byte array");
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)len);
    if (array == NULL) {
        return NULL;
    }
    if (len != 0) {
        (*env)->SetByteArrayRegion(env, array, 0, (jsize)len, (const jbyte *)bytes);
    }
    return array;
}

static inline FfiBuf_u8 boltffi_jni_byte_array_to_buffer(JNIEnv *env, jbyteArray array) {
    FfiBuf_u8 empty = {0};
    if (array == NULL) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte array return was null");
        return empty;
    }
    jsize len = (*env)->GetArrayLength(env, array);
    if (len == 0) {
        return empty;
    }
    FfiBuf_u8 buffer = boltffi_buf_with_len((uintptr_t)len);
    if (buffer.ptr == NULL) {
        boltffi_jni_throw_runtime(env, "failed to allocate BoltFFI byte array return");
        return empty;
    }
    (*env)->GetByteArrayRegion(env, array, 0, len, (jbyte *)buffer.ptr);
    return buffer;
}

static bool boltffi_jni_direct_buffer_address(JNIEnv *env, jobject buffer, jlong required_capacity, void **address) {
    if (buffer == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI direct buffer argument was null");
        return false;
    }
    if (required_capacity < 0) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI direct buffer length was negative");
        return false;
    }
    jlong capacity = (*env)->GetDirectBufferCapacity(env, buffer);
    if (capacity < 0) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI argument was not a direct buffer");
        return false;
    }
    if (capacity < required_capacity) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI direct buffer capacity was too small");
        return false;
    }
    *address = (*env)->GetDirectBufferAddress(env, buffer);
    if (*address == NULL && required_capacity != 0) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI direct buffer address was unavailable");
        return false;
    }
    return true;
}

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    jint env_result = (*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6);
    if (env_result != JNI_OK) {
        fprintf(stderr, "BoltFFI JNI_OnLoad failed: GetEnv(JNI_VERSION_1_6) returned %d\n", (int)env_result);
        return JNI_ERR;
    }
    if (!boltffi_jni_lookup_global_class_with_diagnostic(env, "org/tarwyn/Native", "org/tarwyn/Native", &boltffi_jni_native_class)) {
        return JNI_ERR;
    }
    if (!boltffi_jni_continuation_load(env)) {
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
    boltffi_jni_vm = vm;
    return JNI_VERSION_1_6;
}

JNIEXPORT void JNICALL JNI_OnUnload(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) == JNI_OK) {
        boltffi_jni_continuation_unload(env);
        if (boltffi_jni_native_class != NULL) {
            (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        }
    }
    boltffi_jni_vm = NULL;
    boltffi_jni_native_class = NULL;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1release_1class_1tarwyn_1bindings_1x_1tables_1client(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;

    (void)env;
    boltffi_release_class_tarwyn_bindings_x_tables_client(handle);

    return;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1init_1class_1tarwyn_1bindings_1x_1tables_1client_1new(JNIEnv *env, jclass cls) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_init_class_tarwyn_bindings_x_tables_client_new();

    return (jlong)__boltffi_result;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1init_1class_1tarwyn_1bindings_1x_1tables_1client_1connect(JNIEnv *env, jclass cls, jobject host, jint __boltffi_host_len) {
    (void)cls;

    void *__boltffi_host_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, host, (jlong)__boltffi_host_len, &__boltffi_host_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    uint64_t __boltffi_result = boltffi_init_class_tarwyn_bindings_x_tables_client_connect((const uint8_t *)__boltffi_host_ptr, (uintptr_t)__boltffi_host_len);

    return (jlong)__boltffi_result;
__boltffi_error:
    return 0;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1init_1class_1tarwyn_1bindings_1x_1tables_1client_1with_1ports(JNIEnv *env, jclass cls, jobject host, jint __boltffi_host_len, jshort push_port, jshort req_port, jshort sub_port, jshort telemetry_port, jlong request_timeout_ms, jint send_high_water_mark) {
    (void)cls;

    void *__boltffi_host_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, host, (jlong)__boltffi_host_len, &__boltffi_host_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    uint64_t __boltffi_result = boltffi_init_class_tarwyn_bindings_x_tables_client_with_ports((const uint8_t *)__boltffi_host_ptr, (uintptr_t)__boltffi_host_len, push_port, req_port, sub_port, telemetry_port, request_timeout_ms, send_high_water_mark);

    return (jlong)__boltffi_result;
__boltffi_error:
    return 0;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1start(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_start(receiver);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1stop(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_stop(receiver);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1string(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_string(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1integer(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jint value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1long(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jlong value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_long(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1double(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jdouble value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_double(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1float(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jfloat value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_float(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1boolean(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jboolean value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1bytes(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1string_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_string_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1bytes_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1double_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jdoubleArray value) {
    (void)cls;

    jdouble *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    jdouble __boltffi_value_stack[8];
    bool __boltffi_value_needs_release = false;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    if (__boltffi_value_len <= (jsize)8) {
        (*env)->GetDoubleArrayRegion(env, value, 0, __boltffi_value_len, __boltffi_value_stack);
        if ((*env)->ExceptionCheck(env)) {
            goto __boltffi_error;
        }
        __boltffi_value_ptr = __boltffi_value_stack;
    } else {
        __boltffi_value_ptr = (*env)->GetDoubleArrayElements(env, value, NULL);
        if (__boltffi_value_ptr == NULL) {
            goto __boltffi_error;
        }
        __boltffi_value_needs_release = true;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_double_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const double *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseDoubleArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseDoubleArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1float_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jfloatArray value) {
    (void)cls;

    jfloat *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    jfloat __boltffi_value_stack[8];
    bool __boltffi_value_needs_release = false;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    if (__boltffi_value_len <= (jsize)8) {
        (*env)->GetFloatArrayRegion(env, value, 0, __boltffi_value_len, __boltffi_value_stack);
        if ((*env)->ExceptionCheck(env)) {
            goto __boltffi_error;
        }
        __boltffi_value_ptr = __boltffi_value_stack;
    } else {
        __boltffi_value_ptr = (*env)->GetFloatArrayElements(env, value, NULL);
        if (__boltffi_value_ptr == NULL) {
            goto __boltffi_error;
        }
        __boltffi_value_needs_release = true;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_float_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const float *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseFloatArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseFloatArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1integer_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jintArray value) {
    (void)cls;

    jint *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    jint __boltffi_value_stack[8];
    bool __boltffi_value_needs_release = false;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    if (__boltffi_value_len <= (jsize)8) {
        (*env)->GetIntArrayRegion(env, value, 0, __boltffi_value_len, __boltffi_value_stack);
        if ((*env)->ExceptionCheck(env)) {
            goto __boltffi_error;
        }
        __boltffi_value_ptr = __boltffi_value_stack;
    } else {
        __boltffi_value_ptr = (*env)->GetIntArrayElements(env, value, NULL);
        if (__boltffi_value_ptr == NULL) {
            goto __boltffi_error;
        }
        __boltffi_value_needs_release = true;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const int32_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseIntArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseIntArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1long_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jlongArray value) {
    (void)cls;

    jlong *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    jlong __boltffi_value_stack[8];
    bool __boltffi_value_needs_release = false;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    if (__boltffi_value_len <= (jsize)8) {
        (*env)->GetLongArrayRegion(env, value, 0, __boltffi_value_len, __boltffi_value_stack);
        if ((*env)->ExceptionCheck(env)) {
            goto __boltffi_error;
        }
        __boltffi_value_ptr = __boltffi_value_stack;
    } else {
        __boltffi_value_ptr = (*env)->GetLongArrayElements(env, value, NULL);
        if (__boltffi_value_ptr == NULL) {
            goto __boltffi_error;
        }
        __boltffi_value_needs_release = true;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_long_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const int64_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseLongArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseLongArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1boolean_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jbooleanArray value) {
    (void)cls;

    jboolean *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    jboolean __boltffi_value_stack[8];
    bool __boltffi_value_needs_release = false;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    if (__boltffi_value_len <= (jsize)8) {
        (*env)->GetBooleanArrayRegion(env, value, 0, __boltffi_value_len, __boltffi_value_stack);
        if ((*env)->ExceptionCheck(env)) {
            goto __boltffi_error;
        }
        __boltffi_value_ptr = __boltffi_value_stack;
    } else {
        __boltffi_value_ptr = (*env)->GetBooleanArrayElements(env, value, NULL);
        if (__boltffi_value_ptr == NULL) {
            goto __boltffi_error;
        }
        __boltffi_value_needs_release = true;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const bool *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseBooleanArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        if (__boltffi_value_needs_release) {
            (*env)->ReleaseBooleanArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        }
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1coordinates(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jbyteArray value) {
    (void)cls;

    jbyte *__boltffi_value_ptr = NULL;
    jsize __boltffi_value_len = 0;
    void *__boltffi_channel_ptr = NULL;

    if (value == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
        goto __boltffi_error;
    }
    __boltffi_value_len = (*env)->GetArrayLength(env, value);
    __boltffi_value_ptr = (*env)->GetByteArrayElements(env, value, NULL);
    if (__boltffi_value_ptr == NULL) {
        goto __boltffi_error;
    }

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_coordinates(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_value_ptr != NULL) {
        (*env)->ReleaseByteArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        __boltffi_value_ptr = NULL;
    }
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    if (__boltffi_value_ptr != NULL) {
        (*env)->ReleaseByteArrayElements(env, value, __boltffi_value_ptr, JNI_ABORT);
        __boltffi_value_ptr = NULL;
    }
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1pose2d(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;
    ___Pose2d __boltffi_value_value;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)sizeof(___Pose2d), &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }
    memcpy(&__boltffi_value_value, __boltffi_value_ptr, sizeof(___Pose2d));

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose2d(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, __boltffi_value_value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1pose3d(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;
    ___Pose3d __boltffi_value_value;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)sizeof(___Pose3d), &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }
    memcpy(&__boltffi_value_value, __boltffi_value_ptr, sizeof(___Pose3d));

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose3d(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, __boltffi_value_value);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1bezier_1curve(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curve(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1bezier_1curves(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1bezier_1curves_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1unknown_1bytes(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_put_unknown_bytes(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1put_1typed_1bytes(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jint tarwyn_type, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_put_typed_bytes(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, tarwyn_type, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1string(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_string(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1integer(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1long(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_long(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1double(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_double(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1float(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_float(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1boolean(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1bytes(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1string_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_string_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1bytes_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1double_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_double_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1float_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_float_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1integer_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1long_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_long_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1boolean_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1coordinates(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_coordinates(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1pose2d(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose2d(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1pose3d(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose3d(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1bezier_1curve(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curve(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1bezier_1curves(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1bezier_1curves_1list(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves_list(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1unknown_1bytes(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_unknown_bytes(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jint JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1delete(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    uint32_t __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_delete(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return (jint)__boltffi_result;
__boltffi_error:
    return 0;
}

JNIEXPORT jint JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1delete_1all(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint32_t __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_delete_all(receiver);

    return (jint)__boltffi_result;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1tables(JNIEnv *env, jclass cls, jlong receiver, jobject prefix, jint __boltffi_prefix_len) {
    (void)cls;

    void *__boltffi_prefix_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, prefix, (jlong)__boltffi_prefix_len, &__boltffi_prefix_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_tables(receiver, (const uint8_t *)__boltffi_prefix_ptr, (uintptr_t)__boltffi_prefix_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1ping(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_ping(receiver);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1server_1statistics(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_server_statistics(receiver);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1get_1raw_1json(JNIEnv *env, jclass cls, jlong receiver, jobject prefix, jint __boltffi_prefix_len) {
    (void)cls;

    void *__boltffi_prefix_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, prefix, (jlong)__boltffi_prefix_len, &__boltffi_prefix_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_get_raw_json(receiver, (const uint8_t *)__boltffi_prefix_ptr, (uintptr_t)__boltffi_prefix_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1compare_1and_1set_1absent_1string(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_absent_string(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1compare_1and_1set_1string(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject expected, jint __boltffi_expected_len, jobject value, jint __boltffi_value_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_expected_ptr = NULL;
    void *__boltffi_value_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, expected, (jlong)__boltffi_expected_len, &__boltffi_expected_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, value, (jlong)__boltffi_value_len, &__boltffi_value_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_string(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_expected_ptr, (uintptr_t)__boltffi_expected_len, (const uint8_t *)__boltffi_value_ptr, (uintptr_t)__boltffi_value_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1compare_1and_1set_1double(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jdouble expected, jdouble value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_double(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, expected, value);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1compare_1and_1set_1long(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jlong expected, jlong value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_long(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, expected, value);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1compare_1and_1set_1boolean(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jboolean expected, jboolean value) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_boolean(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, expected, value);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1publish_1telemetry(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len, jobject payload, jint __boltffi_payload_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;
    void *__boltffi_payload_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }
    if (!boltffi_jni_direct_buffer_address(env, payload, (jlong)__boltffi_payload_len, &__boltffi_payload_ptr)) {
        goto __boltffi_error;
    }

    FfiStatus __boltffi_status = boltffi_method_class_tarwyn_bindings_x_tables_client_publish_telemetry(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len, (const uint8_t *)__boltffi_payload_ptr, (uintptr_t)__boltffi_payload_len);

    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
        return;
    }

    return;
__boltffi_error:
    return;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1log_1to(JNIEnv *env, jclass cls, jlong receiver, jobject path, jint __boltffi_path_len) {
    (void)cls;

    void *__boltffi_path_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, path, (jlong)__boltffi_path_len, &__boltffi_path_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_log_to(receiver, (const uint8_t *)__boltffi_path_ptr, (uintptr_t)__boltffi_path_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1log_1to_1drive(JNIEnv *env, jclass cls, jlong receiver, jobject filename, jint __boltffi_filename_len) {
    (void)cls;

    void *__boltffi_filename_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, filename, (jlong)__boltffi_filename_len, &__boltffi_filename_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_log_to_drive(receiver, (const uint8_t *)__boltffi_filename_ptr, (uintptr_t)__boltffi_filename_len);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
__boltffi_error:
    return NULL;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1dropped_1log_1records(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_log_records(receiver);

    return (jlong)__boltffi_result;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1logging_1healthy(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_logging_healthy(receiver);

    return (jboolean)__boltffi_result;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1dropped_1publishes(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_publishes(receiver);

    return (jlong)__boltffi_result;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1subscribe(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1unsubscribe(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1subscribe_1telemetry(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_telemetry(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1unsubscribe_1telemetry(JNIEnv *env, jclass cls, jlong receiver, jobject channel, jint __boltffi_channel_len) {
    (void)cls;

    void *__boltffi_channel_ptr = NULL;

    if (!boltffi_jni_direct_buffer_address(env, channel, (jlong)__boltffi_channel_len, &__boltffi_channel_ptr)) {
        goto __boltffi_error;
    }

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_telemetry(receiver, (const uint8_t *)__boltffi_channel_ptr, (uintptr_t)__boltffi_channel_len);

    return (jboolean)__boltffi_result;
__boltffi_error:
    return JNI_FALSE;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1subscribe_1to_1logs(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_to_logs(receiver);

    return (jboolean)__boltffi_result;
}

JNIEXPORT jboolean JNICALL Java_org_tarwyn_Native_boltffi_1method_1class_1tarwyn_1bindings_1x_1tables_1client_1unsubscribe_1from_1logs(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    bool __boltffi_result = boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_from_logs(receiver);

    return (jboolean)__boltffi_result;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1subscribe(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_updates_subscribe(receiver);

    return (jlong)__boltffi_result;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1pop_1batch(JNIEnv *env, jclass cls, jlong subscription, jlong max_count) {
    (void)cls;

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_updates_pop_batch(subscription, max_count);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
}

JNIEXPORT jint JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1wait(JNIEnv *env, jclass cls, jlong subscription, jint timeout_milliseconds) {
    (void)cls;

    (void)env;
    WaitResult __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_updates_wait(subscription, timeout_milliseconds);

    return (jint)__boltffi_result;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1poll(JNIEnv *env, jclass cls, jlong subscription, jlong callback_data) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_updates_poll(subscription, callback_data, boltffi_jni_continuation_callback);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1unsubscribe(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_updates_unsubscribe(subscription);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1updates_1free(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_updates_free(subscription);

    return;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1subscribe(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_subscribe(receiver);

    return (jlong)__boltffi_result;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1pop_1batch(JNIEnv *env, jclass cls, jlong subscription, jlong max_count) {
    (void)cls;

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_pop_batch(subscription, max_count);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
}

JNIEXPORT jint JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1wait(JNIEnv *env, jclass cls, jlong subscription, jint timeout_milliseconds) {
    (void)cls;

    (void)env;
    WaitResult __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_wait(subscription, timeout_milliseconds);

    return (jint)__boltffi_result;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1poll(JNIEnv *env, jclass cls, jlong subscription, jlong callback_data) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_poll(subscription, callback_data, boltffi_jni_continuation_callback);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1unsubscribe(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_unsubscribe(subscription);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1telemetry_1free(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_free(subscription);

    return;
}

JNIEXPORT jlong JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1subscribe(JNIEnv *env, jclass cls, jlong receiver) {
    (void)cls;

    (void)env;
    uint64_t __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_logs_subscribe(receiver);

    return (jlong)__boltffi_result;
}

JNIEXPORT jbyteArray JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1pop_1batch(JNIEnv *env, jclass cls, jlong subscription, jlong max_count) {
    (void)cls;

    (void)env;
    FfiBuf_u8 __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_logs_pop_batch(subscription, max_count);

    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
}

JNIEXPORT jint JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1wait(JNIEnv *env, jclass cls, jlong subscription, jint timeout_milliseconds) {
    (void)cls;

    (void)env;
    WaitResult __boltffi_result = boltffi_stream_tarwyn_bindings_x_tables_client_logs_wait(subscription, timeout_milliseconds);

    return (jint)__boltffi_result;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1poll(JNIEnv *env, jclass cls, jlong subscription, jlong callback_data) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_logs_poll(subscription, callback_data, boltffi_jni_continuation_callback);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1unsubscribe(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_logs_unsubscribe(subscription);

    return;
}

JNIEXPORT void JNICALL Java_org_tarwyn_Native_boltffi_1stream_1tarwyn_1bindings_1x_1tables_1client_1logs_1free(JNIEnv *env, jclass cls, jlong subscription) {
    (void)cls;

    (void)env;
    boltffi_stream_tarwyn_bindings_x_tables_client_logs_free(subscription);

    return;
}
