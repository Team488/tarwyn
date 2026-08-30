package org.tarwyn;

final class Native {
    static {
        String virtualMachineName = System.getProperty("java.vm.name", "")
            .toLowerCase(java.util.Locale.ROOT);
        boolean androidRuntime = virtualMachineName.contains("dalvik")
            || virtualMachineName.contains("art");
        if (androidRuntime) {
            System.loadLibrary("tarwyn_bindings");
        } else {
            BoltFFINativeRuntime.load(
                Native.class,
                "tarwyn_bindings_jni",
                "tarwyn_bindings"
            );
        }
    }

    private Native() {}

    static native void boltffi_release_class_tarwyn_bindings_x_tables_client(long handle);

    static native long boltffi_init_class_tarwyn_bindings_x_tables_client_new();

    static native long boltffi_init_class_tarwyn_bindings_x_tables_client_connect(java.nio.ByteBuffer host, int __boltffi_host_len);

    static native long boltffi_init_class_tarwyn_bindings_x_tables_client_with_ports(java.nio.ByteBuffer host, int __boltffi_host_len, short push_port, short req_port, short sub_port, short telemetry_port, long request_timeout_ms, int send_high_water_mark);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_start(long receiver);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_stop(long receiver);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_string(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, int value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_long(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, long value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_double(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, double value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_float(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, float value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, boolean value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_string_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_double_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, double[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_float_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, float[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, int[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_long_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, long[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, boolean[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_coordinates(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, byte[] value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose2d(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose3d(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curve(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_put_unknown_bytes(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_put_typed_bytes(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, int tarwyn_type, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_string(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_long(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_double(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_float(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_string_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_double_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_float_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_long_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_coordinates(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose2d(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose3d(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curve(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves_list(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_unknown_bytes(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native int boltffi_method_class_tarwyn_bindings_x_tables_client_delete(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native int boltffi_method_class_tarwyn_bindings_x_tables_client_delete_all(long receiver);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_tables(long receiver, java.nio.ByteBuffer prefix, int __boltffi_prefix_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_ping(long receiver);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_server_statistics(long receiver);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_get_raw_json(long receiver, java.nio.ByteBuffer prefix, int __boltffi_prefix_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_absent_string(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_string(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer expected, int __boltffi_expected_len, java.nio.ByteBuffer value, int __boltffi_value_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_double(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, double expected, double value);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_long(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, long expected, long value);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_boolean(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, boolean expected, boolean value);

    static native void boltffi_method_class_tarwyn_bindings_x_tables_client_publish_telemetry(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len, java.nio.ByteBuffer payload, int __boltffi_payload_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_log_to(long receiver, java.nio.ByteBuffer path, int __boltffi_path_len);

    static native byte[] boltffi_method_class_tarwyn_bindings_x_tables_client_log_to_drive(long receiver, java.nio.ByteBuffer filename, int __boltffi_filename_len);

    static native long boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_log_records(long receiver);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_logging_healthy(long receiver);

    static native long boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_publishes(long receiver);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_telemetry(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_telemetry(long receiver, java.nio.ByteBuffer channel, int __boltffi_channel_len);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_to_logs(long receiver);

    static native boolean boltffi_method_class_tarwyn_bindings_x_tables_client_unsubscribe_from_logs(long receiver);

    static native long boltffi_stream_tarwyn_bindings_x_tables_client_updates_subscribe(long receiver);

    static native byte[] boltffi_stream_tarwyn_bindings_x_tables_client_updates_pop_batch(long subscription, long max_count);

    static native int boltffi_stream_tarwyn_bindings_x_tables_client_updates_wait(long subscription, int timeout_milliseconds);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_updates_poll(long subscription, long callback_data);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_updates_unsubscribe(long subscription);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_updates_free(long subscription);

    static void boltffiFutureContinuationCallback(long handle, byte pollResult) {
        BoltFfiAsync.resume(handle, pollResult);
    }

    static native long boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_subscribe(long receiver);

    static native byte[] boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_pop_batch(long subscription, long max_count);

    static native int boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_wait(long subscription, int timeout_milliseconds);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_poll(long subscription, long callback_data);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_unsubscribe(long subscription);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_free(long subscription);

    static native long boltffi_stream_tarwyn_bindings_x_tables_client_logs_subscribe(long receiver);

    static native byte[] boltffi_stream_tarwyn_bindings_x_tables_client_logs_pop_batch(long subscription, long max_count);

    static native int boltffi_stream_tarwyn_bindings_x_tables_client_logs_wait(long subscription, int timeout_milliseconds);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_logs_poll(long subscription, long callback_data);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_logs_unsubscribe(long subscription);

    static native void boltffi_stream_tarwyn_bindings_x_tables_client_logs_free(long subscription);
}

@FunctionalInterface
interface BoltFfiFutureStart {
    long start();
}

@FunctionalInterface
interface BoltFfiFuturePoll {
    void poll(long future, long continuation);
}

@FunctionalInterface
interface BoltFfiFutureComplete<T> {
    T complete(long future);
}

@FunctionalInterface
interface BoltFfiFutureLifecycle {
    void apply(long future);
}

final class BoltFfiAsync {
    private static final byte READY = 0;
    private static final java.util.concurrent.atomic.AtomicLong NEXT_CONTINUATION =
        new java.util.concurrent.atomic.AtomicLong(1L);
    private static final java.util.concurrent.ConcurrentHashMap<Long, PollSignal> CONTINUATIONS =
        new java.util.concurrent.ConcurrentHashMap<>();

    private BoltFfiAsync() {}

    static void resume(long handle, byte result) {
        PollSignal signal = CONTINUATIONS.get(handle);
        if (signal != null && signal.begin(result)) {
            signal.finish(result);
            CONTINUATIONS.remove(handle, signal);
        }
    }

    static <T> java.util.concurrent.CompletableFuture<T> call(
        BoltFfiFutureStart start,
        BoltFfiFuturePoll poll,
        BoltFfiFutureComplete<T> complete,
        BoltFfiFutureLifecycle cancel,
        BoltFfiFutureLifecycle free
    ) {
        long future;
        try {
            future = start.start();
        } catch (Throwable error) {
            java.util.concurrent.CompletableFuture<T> failed =
                new java.util.concurrent.CompletableFuture<>();
            failed.completeExceptionally(error);
            return failed;
        }
        Operation<T> operation = new Operation<>(future, poll, complete, cancel, free);
        operation.drive();
        return operation.result;
    }

    static RuntimeException failure(
        Throwable failure,
        java.util.function.Supplier<byte[]> panic
    ) {
        try {
            byte[] message = panic.get();
            if (message != null && message.length != 0) {
                return new RuntimeException(
                    new String(message, java.nio.charset.StandardCharsets.UTF_8),
                    failure
                );
            }
        } catch (Throwable ignored) {}
        if (failure instanceof RuntimeException) return (RuntimeException) failure;
        return new RuntimeException(failure);
    }

    static java.util.concurrent.CompletableFuture<Byte> poll(
        long owner,
        BoltFfiFuturePoll poll
    ) {
        PollSignal signal = new PollSignal(() -> {});
        long handle = NEXT_CONTINUATION.getAndIncrement();
        CONTINUATIONS.put(handle, signal);
        try {
            poll.poll(owner, handle);
        } catch (Throwable failure) {
            CONTINUATIONS.remove(handle, signal);
            signal.future.completeExceptionally(failure);
        }
        return signal.future;
    }

    private enum Phase {
        ACTIVE,
        POLLING,
        WAITING,
        CANCEL_REQUESTED,
        READY,
        OWNED
    }

    private enum Cancellation {
        REJECTED,
        DEFERRED,
        OWNED
    }

    private enum Pending {
        STOPPED,
        CONTINUE,
        CANCELLED
    }

    private enum Delivery {
        WAITING,
        PENDING,
        READY,
        CANCELLED
    }

    private static final class PollSignal {
        private final java.util.concurrent.atomic.AtomicReference<Delivery> delivery =
            new java.util.concurrent.atomic.AtomicReference<>(Delivery.WAITING);
        private final java.util.concurrent.CompletableFuture<Byte> future =
            new java.util.concurrent.CompletableFuture<>();
        private final Runnable ready;

        private PollSignal(Runnable ready) {
            this.ready = ready;
        }

        private boolean begin(byte result) {
            Delivery next = result == READY ? Delivery.READY : Delivery.PENDING;
            if (!delivery.compareAndSet(Delivery.WAITING, next)) return false;
            if (next == Delivery.READY) ready.run();
            return true;
        }

        private Cancellation cancel() {
            while (true) {
                Delivery current = delivery.get();
                if (current == Delivery.READY) return Cancellation.REJECTED;
                if (current == Delivery.PENDING) return Cancellation.DEFERRED;
                if (current == Delivery.CANCELLED) return Cancellation.OWNED;
                if (delivery.compareAndSet(Delivery.WAITING, Delivery.CANCELLED)) {
                    return Cancellation.OWNED;
                }
            }
        }

        private boolean readyStarted() {
            return delivery.get() == Delivery.READY;
        }

        private void finish(byte result) {
            future.complete(result);
        }
    }

    private static final class ActivePoll {
        private final long handle;
        private final PollSignal signal;

        private ActivePoll(long handle, PollSignal signal) {
            this.handle = handle;
            this.signal = signal;
        }
    }

    private static final class Result<T> extends java.util.concurrent.CompletableFuture<T> {
        private final Operation<T> operation;

        private Result(Operation<T> operation) {
            this.operation = operation;
        }

        @Override
        public boolean cancel(boolean mayInterruptIfRunning) {
            Cancellation cancellation = operation.requestCancellation();
            if (cancellation == Cancellation.REJECTED) return false;
            if (cancellation == Cancellation.OWNED) operation.cancelAndFree();
            return super.cancel(mayInterruptIfRunning);
        }

        private void finishCancelled() {
            operation.cancelAndFree();
            super.cancel(false);
        }
    }

    private static final class Operation<T> {
        private final long future;
        private final BoltFfiFuturePoll poll;
        private final BoltFfiFutureComplete<T> complete;
        private final BoltFfiFutureLifecycle cancel;
        private final BoltFfiFutureLifecycle free;
        private final java.util.concurrent.atomic.AtomicReference<Phase> phase =
            new java.util.concurrent.atomic.AtomicReference<>(Phase.ACTIVE);
        private final java.util.concurrent.atomic.AtomicReference<ActivePoll> active =
            new java.util.concurrent.atomic.AtomicReference<>();
        private final Result<T> result = new Result<>(this);

        private Operation(
            long future,
            BoltFfiFuturePoll poll,
            BoltFfiFutureComplete<T> complete,
            BoltFfiFutureLifecycle cancel,
            BoltFfiFutureLifecycle free
        ) {
            this.future = future;
            this.poll = poll;
            this.complete = complete;
            this.cancel = cancel;
            this.free = free;
        }

        private void drive() {
            while (true) {
                if (!phase.compareAndSet(Phase.ACTIVE, Phase.POLLING)) return;
                PollSignal signal = new PollSignal(this::markReady);
                long handle = NEXT_CONTINUATION.getAndIncrement();
                ActivePoll currentPoll = new ActivePoll(handle, signal);
                CONTINUATIONS.put(handle, signal);
                active.set(currentPoll);
                try {
                    poll.poll(future, handle);
                } catch (Throwable error) {
                    finishPollingFailure(currentPoll, error);
                    return;
                }
                if (!signal.future.isDone()) {
                    if (phase.compareAndSet(Phase.POLLING, Phase.WAITING)) {
                        signal.future.whenComplete(
                            (pollResult, error) -> finishAsyncPoll(currentPoll, pollResult, error)
                        );
                        return;
                    }
                    release(currentPoll);
                    if (finishDeferredCancellation()) return;
                    if (claimReady()) finishReady();
                    return;
                }
                byte pollResult;
                try {
                    pollResult = signal.future.join();
                } catch (Throwable error) {
                    finishPollingFailure(currentPoll, error);
                    return;
                }
                if (pollResult == READY) {
                    finishJoinedReady(currentPoll);
                    return;
                }
                if (finishDeferredCancellation(currentPoll)) return;
                Pending pending = finishJoinedPending(currentPoll);
                if (pending == Pending.CANCELLED) {
                    result.finishCancelled();
                    return;
                }
                if (pending != Pending.CONTINUE) return;
            }
        }

        private void finishAsyncPoll(ActivePoll currentPoll, Byte pollResult, Throwable error) {
            release(currentPoll);
            if (error != null) {
                if (phase.getAndSet(Phase.OWNED) != Phase.OWNED) cancelAndFree();
                result.completeExceptionally(error);
                return;
            }
            if (pollResult == READY) {
                if (claimReady()) {
                    finishReady();
                    return;
                }
                finishDeferredCancellation();
                return;
            }
            if (finishDeferredCancellation()) return;
            if (rearm()) drive();
        }

        private Cancellation requestCancellation() {
            while (true) {
                Phase current = phase.get();
                if (current == Phase.OWNED || current == Phase.READY) {
                    return Cancellation.REJECTED;
                }
                if (current == Phase.CANCEL_REQUESTED) return Cancellation.DEFERRED;
                if (current == Phase.ACTIVE) {
                    if (phase.compareAndSet(Phase.ACTIVE, Phase.OWNED)) {
                        return Cancellation.OWNED;
                    }
                    continue;
                }
                ActivePoll currentPoll = active.get();
                if (currentPoll == null) continue;
                if (current == Phase.WAITING) {
                    Cancellation delivery = currentPoll.signal.cancel();
                    if (delivery == Cancellation.REJECTED) return Cancellation.REJECTED;
                    if (delivery == Cancellation.DEFERRED) {
                        if (phase.compareAndSet(Phase.WAITING, Phase.CANCEL_REQUESTED)) {
                            return Cancellation.DEFERRED;
                        }
                        continue;
                    }
                    if (!phase.compareAndSet(Phase.WAITING, Phase.OWNED)) continue;
                    release(currentPoll);
                    CONTINUATIONS.remove(currentPoll.handle, currentPoll.signal);
                    return Cancellation.OWNED;
                }
                if (current == Phase.POLLING) {
                    if (currentPoll.signal.readyStarted()) return Cancellation.REJECTED;
                    if (!phase.compareAndSet(Phase.POLLING, Phase.CANCEL_REQUESTED)) continue;
                    ActivePoll updated = active.get();
                    if (updated != null && updated.signal.readyStarted()) {
                        if (phase.compareAndSet(Phase.CANCEL_REQUESTED, Phase.READY)) {
                            return Cancellation.REJECTED;
                        }
                        continue;
                    }
                    return Cancellation.DEFERRED;
                }
            }
        }

        private void finishJoinedReady(ActivePoll currentPoll) {
            if (claimReady()) {
                finishReady();
                release(currentPoll);
                return;
            }
            finishDeferredCancellation(currentPoll);
            release(currentPoll);
        }

        private Pending finishJoinedPending(ActivePoll currentPoll) {
            if (claimDeferredCancellation()) {
                release(currentPoll);
                return Pending.CANCELLED;
            }
            Pending pending = rearm()
                ? Pending.CONTINUE
                : claimDeferredCancellation()
                    ? Pending.CANCELLED
                    : Pending.STOPPED;
            release(currentPoll);
            return pending;
        }

        private void finishPollingFailure(ActivePoll currentPoll, Throwable error) {
            release(currentPoll);
            CONTINUATIONS.remove(currentPoll.handle, currentPoll.signal);
            if (phase.getAndSet(Phase.OWNED) != Phase.OWNED) cancelAndFree();
            result.completeExceptionally(error);
        }

        private void finishReady() {
            try {
                result.complete(complete.complete(future));
            } catch (Throwable error) {
                result.completeExceptionally(error);
            } finally {
                releaseFuture();
            }
        }

        private void cancelAndFree() {
            try {
                cancel.apply(future);
            } catch (Throwable ignored) {}
            releaseFuture();
        }

        private void releaseFuture() {
            try {
                free.apply(future);
            } catch (Throwable ignored) {}
        }

        private boolean finishDeferredCancellation() {
            if (!claimDeferredCancellation()) return false;
            result.finishCancelled();
            return true;
        }

        private boolean finishDeferredCancellation(ActivePoll currentPoll) {
            if (!claimDeferredCancellation()) return false;
            release(currentPoll);
            result.finishCancelled();
            return true;
        }

        private void markReady() {
            while (true) {
                Phase current = phase.get();
                if (current == Phase.CANCEL_REQUESTED
                    || current == Phase.READY
                    || current == Phase.OWNED) return;
                if (phase.compareAndSet(current, Phase.READY)) return;
            }
        }

        private boolean claimReady() {
            return phase.compareAndSet(Phase.READY, Phase.OWNED);
        }

        private boolean claimDeferredCancellation() {
            return phase.compareAndSet(Phase.CANCEL_REQUESTED, Phase.OWNED);
        }

        private boolean rearm() {
            while (true) {
                Phase current = phase.get();
                if (current != Phase.WAITING && current != Phase.POLLING) return false;
                if (phase.compareAndSet(current, Phase.ACTIVE)) return true;
            }
        }

        private void release(ActivePoll currentPoll) {
            active.compareAndSet(currentPoll, null);
        }
    }
}

@FunctionalInterface
interface DirectRecordWrite<T> {
    void write(T value, java.nio.ByteBuffer buffer, int offset);
}

@FunctionalInterface
interface DirectRecordRead<T> {
    T read(java.nio.ByteBuffer buffer, int offset);
}

final class DirectVectorCodec {
    private DirectVectorCodec() {}

    static boolean[] readBooleanArray(byte[] bytes) {
        boolean[] values = new boolean[bytes.length];
        int index = 0;
        while (index < bytes.length) {
            values[index] = bytes[index] != 0;
            index += 1;
        }
        return values;
    }

    static byte[] writeBooleanArray(boolean[] values) {
        byte[] bytes = new byte[values.length];
        int index = 0;
        while (index < values.length) {
            bytes[index] = (byte) (values[index] ? 1 : 0);
            index += 1;
        }
        return bytes;
    }

    static byte[] readByteArray(byte[] bytes) { return bytes; }
    static byte[] writeByteArray(byte[] values) { return values; }

    static short[] readShortArray(byte[] bytes) {
        short[] values = new short[exactLength(bytes, 2)];
        ordered(bytes).asShortBuffer().get(values);
        return values;
    }

    static byte[] writeShortArray(short[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 2)];
        ordered(bytes).asShortBuffer().put(values);
        return bytes;
    }

    static int[] readIntArray(byte[] bytes) {
        int[] values = new int[exactLength(bytes, 4)];
        ordered(bytes).asIntBuffer().get(values);
        return values;
    }

    static byte[] writeIntArray(int[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 4)];
        ordered(bytes).asIntBuffer().put(values);
        return bytes;
    }

    static long[] readLongArray(byte[] bytes) {
        long[] values = new long[exactLength(bytes, 8)];
        ordered(bytes).asLongBuffer().get(values);
        return values;
    }

    static byte[] writeLongArray(long[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 8)];
        ordered(bytes).asLongBuffer().put(values);
        return bytes;
    }

    static float[] readFloatArray(byte[] bytes) {
        float[] values = new float[exactLength(bytes, 4)];
        ordered(bytes).asFloatBuffer().get(values);
        return values;
    }

    static byte[] writeFloatArray(float[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 4)];
        ordered(bytes).asFloatBuffer().put(values);
        return bytes;
    }

    static double[] readDoubleArray(byte[] bytes) {
        double[] values = new double[exactLength(bytes, 8)];
        ordered(bytes).asDoubleBuffer().get(values);
        return values;
    }

    static byte[] writeDoubleArray(double[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 8)];
        ordered(bytes).asDoubleBuffer().put(values);
        return bytes;
    }

    static <T> byte[] writeRecords(java.util.List<T> values, int size, DirectRecordWrite<T> write) {
        byte[] bytes = new byte[Math.multiplyExact(values.size(), size)];
        java.nio.ByteBuffer buffer = ordered(bytes);
        int index = 0;
        while (index < values.size()) {
            write.write(values.get(index), buffer, Math.multiplyExact(index, size));
            index += 1;
        }
        return bytes;
    }

    static <T> java.util.List<T> readRecords(byte[] bytes, int size, DirectRecordRead<T> read) {
        int count = exactLength(bytes, size);
        java.util.ArrayList<T> values = new java.util.ArrayList<>(count);
        java.nio.ByteBuffer buffer = ordered(bytes);
        int index = 0;
        while (index < count) {
            values.add(read.read(buffer, Math.multiplyExact(index, size)));
            index += 1;
        }
        return values;
    }

    private static java.nio.ByteBuffer ordered(byte[] bytes) {
        return java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.nativeOrder());
    }

    private static int exactLength(byte[] bytes, int width) {
        if (width <= 0 || bytes.length % width != 0) {
            throw new IllegalArgumentException("invalid direct vector byte size");
        }
        return bytes.length / width;
    }
}

@FunctionalInterface
interface BoltFfiStreamBatch<T> {
    java.util.List<T> read(long stream, long maxCount);
}

@FunctionalInterface
interface BoltFfiStreamWait {
    int waitForItems(long stream, int timeout);
}

final class BoltFfiStream {
    private static final byte CLOSED = 1;

    private BoltFfiStream() {}

    static <T> StreamSubscription<T> callback(
        long stream,
        long batchSize,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiFuturePoll poll,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free,
        java.util.function.Consumer<T> deliver
    ) {
        if (stream == 0L) return StreamSubscription.callback(() -> {});
        Context<T> context = new Context<>(
            stream,
            batchSize,
            readBatch,
            poll,
            unsubscribe,
            free,
            deliver
        );
        context.start();
        return StreamSubscription.callback(context::requestTermination);
    }

    static RuntimeException failure(Throwable failure) {
        if (failure instanceof RuntimeException) return (RuntimeException) failure;
        if (failure instanceof Error) throw (Error) failure;
        return new RuntimeException(failure);
    }

    private static final class Context<T> {
        private static final int ACTIVE = 0;
        private static final int TERMINATING = 1;
        private static final int RELEASABLE = 2;
        private static final int RELEASED = 3;

        private final long stream;
        private final long batchSize;
        private final BoltFfiStreamBatch<T> readBatch;
        private final BoltFfiFuturePoll poll;
        private final BoltFfiFutureLifecycle unsubscribe;
        private final BoltFfiFutureLifecycle free;
        private final java.util.function.Consumer<T> deliver;
        private final java.util.concurrent.atomic.AtomicInteger lifecycle =
            new java.util.concurrent.atomic.AtomicInteger(ACTIVE);
        private final java.util.concurrent.atomic.AtomicBoolean processing =
            new java.util.concurrent.atomic.AtomicBoolean(false);

        private Context(
            long stream,
            long batchSize,
            BoltFfiStreamBatch<T> readBatch,
            BoltFfiFuturePoll poll,
            BoltFfiFutureLifecycle unsubscribe,
            BoltFfiFutureLifecycle free,
            java.util.function.Consumer<T> deliver
        ) {
            this.stream = stream;
            this.batchSize = batchSize;
            this.readBatch = readBatch;
            this.poll = poll;
            this.unsubscribe = unsubscribe;
            this.free = free;
            this.deliver = deliver;
        }

        private void start() {
            drive();
        }

        private void requestTermination() {
            Throwable failure = null;
            if (lifecycle.compareAndSet(ACTIVE, TERMINATING)) {
                try {
                    unsubscribe.apply(stream);
                } catch (Throwable error) {
                    failure = error;
                } finally {
                    lifecycle.compareAndSet(TERMINATING, RELEASABLE);
                }
            }
            try {
                finalizeIfIdle();
            } catch (Throwable error) {
                if (failure == null) failure = error;
                else failure.addSuppressed(error);
            }
            if (failure != null) throw BoltFfiStream.failure(failure);
        }

        private void drive() {
            while (lifecycle.get() == ACTIVE) {
                java.util.concurrent.CompletableFuture<Byte> result =
                    BoltFfiAsync.poll(stream, poll);
                if (!result.isDone()) {
                    result.whenComplete(this::finishPoll);
                    return;
                }
                byte pollResult;
                try {
                    pollResult = result.join();
                } catch (Throwable failure) {
                    finishPoll(null, failure);
                    return;
                }
                if (!processPoll(pollResult)) return;
            }
            finalizeIfIdle();
        }

        private void finishPoll(Byte pollResult, Throwable failure) {
            if (failure != null) {
                try {
                    requestTermination();
                } catch (Throwable terminationFailure) {
                    failure.addSuppressed(terminationFailure);
                }
                throw BoltFfiStream.failure(failure);
            }
            if (processPoll(pollResult)) drive();
        }

        private boolean processPoll(byte pollResult) {
            if (!processing.compareAndSet(false, true)) return false;
            Throwable failure = null;
            try {
                if (lifecycle.get() == ACTIVE) drain();
            } catch (Throwable error) {
                failure = error;
            } finally {
                processing.set(false);
                try {
                    finalizeIfIdle();
                } catch (Throwable releaseFailure) {
                    if (failure == null) failure = releaseFailure;
                    else failure.addSuppressed(releaseFailure);
                }
            }
            if (failure != null) {
                try {
                    requestTermination();
                } catch (Throwable terminationFailure) {
                    failure.addSuppressed(terminationFailure);
                }
                throw BoltFfiStream.failure(failure);
            }
            if (pollResult == CLOSED) {
                requestTermination();
                return false;
            }
            return lifecycle.get() == ACTIVE;
        }

        private void drain() {
            while (lifecycle.get() == ACTIVE) {
                java.util.List<T> items = readBatch.read(stream, batchSize);
                if (items.isEmpty()) return;
                items.forEach(deliver);
            }
        }

        private void finalizeIfIdle() {
            if (processing.get()) return;
            if (!lifecycle.compareAndSet(RELEASABLE, RELEASED)) return;
            free.apply(stream);
        }
    }
}

final class StreamSubscription<T> implements AutoCloseable {
    private enum Mode {
        BATCH,
        CALLBACK
    }

    private final java.util.concurrent.atomic.AtomicBoolean closed =
        new java.util.concurrent.atomic.AtomicBoolean(false);
    private final java.util.concurrent.atomic.AtomicBoolean publisherAttached =
        new java.util.concurrent.atomic.AtomicBoolean(false);
    private final java.util.concurrent.atomic.AtomicReference<Thread> publisherWorker =
        new java.util.concurrent.atomic.AtomicReference<>();
    private final Mode mode;
    private final long stream;
    private final Runnable cancel;
    private final BoltFfiStreamBatch<T> readBatch;
    private final BoltFfiStreamWait waitForItems;

    private StreamSubscription(
        Mode mode,
        long stream,
        Runnable cancel,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiStreamWait waitForItems
    ) {
        this.mode = mode;
        this.stream = stream;
        this.cancel = cancel;
        this.readBatch = readBatch;
        this.waitForItems = waitForItems;
    }

    static <T> StreamSubscription<T> callback(Runnable cancel) {
        return new StreamSubscription<>(Mode.CALLBACK, 0L, cancel, null, null);
    }

    static <T> StreamSubscription<T> batch(
        long stream,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiStreamWait waitForItems,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free
    ) {
        return new StreamSubscription<>(
            Mode.BATCH,
            stream,
            () -> release(stream, unsubscribe, free),
            readBatch,
            waitForItems
        );
    }

    public java.util.List<T> popBatch(long maxCount) {
        requireBatch("popBatch");
        if (stream == 0L || closed.get()) return java.util.Collections.emptyList();
        return readBatch.read(stream, maxCount);
    }

    public int waitForItems(int timeout) {
        requireBatch("waitForItems");
        if (stream == 0L || closed.get()) return -1;
        return waitForItems.waitForItems(stream, timeout);
    }

    public void unsubscribe() {
        close();
    }

    public void cancel() {
        close();
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
        try {
            cancel.run();
        } finally {
            Thread worker = publisherWorker.get();
            if (worker != null) java.util.concurrent.locks.LockSupport.unpark(worker);
        }
    }

    private void requireBatch(String operation) {
        if (mode == Mode.BATCH) return;
        throw new IllegalStateException(
            operation + " is only available for batch stream subscriptions"
        );
    }

    private static void release(
        long stream,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free
    ) {
        if (stream == 0L) return;
        Throwable failure = null;
        try {
            unsubscribe.apply(stream);
        } catch (Throwable error) {
            failure = error;
        }
        try {
            free.apply(stream);
        } catch (Throwable error) {
            if (failure == null) failure = error;
            else failure.addSuppressed(error);
        }
        if (failure != null) throw BoltFfiStream.failure(failure);
    }
}

final class BoltFfiStreamBatches {
    private BoltFfiStreamBatches() {}

    static java.util.List<Boolean> booleans(byte[] bytes) {
        boolean[] values = DirectVectorCodec.readBooleanArray(bytes);
        return new java.util.AbstractList<Boolean>() {
            public Boolean get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Byte> bytes(byte[] bytes) {
        return new java.util.AbstractList<Byte>() {
            public Byte get(int index) { return bytes[index]; }
            public int size() { return bytes.length; }
        };
    }

    static java.util.List<Short> shorts(byte[] bytes) {
        short[] values = DirectVectorCodec.readShortArray(bytes);
        return new java.util.AbstractList<Short>() {
            public Short get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Integer> ints(byte[] bytes) {
        int[] values = DirectVectorCodec.readIntArray(bytes);
        return new java.util.AbstractList<Integer>() {
            public Integer get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Long> longs(byte[] bytes) {
        long[] values = DirectVectorCodec.readLongArray(bytes);
        return new java.util.AbstractList<Long>() {
            public Long get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Float> floats(byte[] bytes) {
        float[] values = DirectVectorCodec.readFloatArray(bytes);
        return new java.util.AbstractList<Float>() {
            public Float get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Double> doubles(byte[] bytes) {
        double[] values = DirectVectorCodec.readDoubleArray(bytes);
        return new java.util.AbstractList<Double>() {
            public Double get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static <Source, Target> java.util.List<Target> map(
        java.util.List<Source> source,
        java.util.function.Function<Source, Target> transform
    ) {
        return new java.util.AbstractList<Target>() {
            public Target get(int index) { return transform.apply(source.get(index)); }
            public int size() { return source.size(); }
        };
    }
}

final class BoltFFIValueIdentity {
    private BoltFFIValueIdentity() {}

    static <T> boolean optionalEquals(
        java.util.Optional<T> left,
        java.util.Optional<T> right,
        java.util.function.BiPredicate<T, T> equals
    ) {
        if (left == right) return true;
        if (left == null || right == null) return false;
        if (left.isPresent() != right.isPresent()) return false;
        return !left.isPresent() || equals.test(left.get(), right.get());
    }

    static <T> int optionalHash(
        java.util.Optional<T> value,
        java.util.function.ToIntFunction<T> hash
    ) {
        if (value == null || !value.isPresent()) return 0;
        return 31 + hash.applyAsInt(value.get());
    }

    static <T> boolean sequenceEquals(
        java.util.List<T> left,
        java.util.List<T> right,
        java.util.function.BiPredicate<T, T> equals
    ) {
        if (left == right) return true;
        if (left == null || right == null || left.size() != right.size()) return false;
        int index = 0;
        while (index < left.size()) {
            if (!equals.test(left.get(index), right.get(index))) return false;
            index += 1;
        }
        return true;
    }

    static <T> int sequenceHash(
        java.util.List<T> values,
        java.util.function.ToIntFunction<T> hash
    ) {
        if (values == null) return 0;
        int result = 1;
        int index = 0;
        while (index < values.size()) {
            result = 31 * result + hash.applyAsInt(values.get(index));
            index += 1;
        }
        return result;
    }
}

@FunctionalInterface
interface WireRead<T> {
    T read();
}

@FunctionalInterface
interface WireWrite<T> {
    void write(T value);
}

@FunctionalInterface
interface WireSize<T> {
    int size(T value);
}

final class WireReader {
    private final java.nio.ByteBuffer buffer;

    WireReader(byte[] bytes) {
        this.buffer = java.nio.ByteBuffer
            .wrap(java.util.Objects.requireNonNull(bytes, "null buffer returned"))
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
    }

    boolean readBoolean() { return buffer.get() != 0; }
    byte readByte() { return buffer.get(); }
    short readShort() { return buffer.getShort(); }
    int readInt() { return buffer.getInt(); }
    long readLong() { return buffer.getLong(); }
    float readFloat() { return buffer.getFloat(); }
    double readDouble() { return buffer.getDouble(); }

    java.time.Duration readDuration() {
        long seconds = readLong();
        int nanos = readInt();
        if (seconds < 0 || nanos < 0) {
            throw new IllegalArgumentException("duration out of range");
        }
        return java.time.Duration.ofSeconds(seconds, nanos);
    }

    java.time.Instant readInstant() {
        long seconds = readLong();
        int nanos = readInt();
        if (nanos < 0) {
            throw new IllegalArgumentException("instant nanos out of range");
        }
        return java.time.Instant.ofEpochSecond(seconds, nanos);
    }

    java.util.UUID readUuid() {
        return new java.util.UUID(readLong(), readLong());
    }

    java.net.URI readUri() { return java.net.URI.create(readString()); }

    String readString() {
        int length = readLength();
        String value = new String(
            buffer.array(),
            buffer.arrayOffset() + buffer.position(),
            length,
            java.nio.charset.StandardCharsets.UTF_8
        );
        buffer.position(buffer.position() + length);
        return value;
    }

    byte[] readBytes() {
        int length = readLength();
        byte[] value = new byte[length];
        buffer.get(value);
        return value;
    }

    <T> java.util.Optional<T> readOptional(WireRead<T> read) {
        return readBoolean()
            ? java.util.Optional.ofNullable(read.read())
            : java.util.Optional.empty();
    }

    <T> java.util.List<T> readSequence(WireRead<T> read) {
        int length = readCount();
        java.util.ArrayList<T> values = new java.util.ArrayList<>(length);
        int index = 0;
        while (index < length) {
            values.add(read.read());
            index += 1;
        }
        return values;
    }

    <K, V> java.util.Map<K, V> readMap(WireRead<K> readKey, WireRead<V> readValue) {
        int length = readCount();
        java.util.HashMap<K, V> values = new java.util.HashMap<>(mapCapacity(length));
        int index = 0;
        while (index < length) {
            K key = readKey.read();
            if (values.containsKey(key)) {
                throw new IllegalArgumentException("duplicate wire map key");
            }
            values.put(key, readValue.read());
            index += 1;
        }
        return values;
    }

    java.util.List<String> readStringSequence() {
        int length = readCount();
        java.util.ArrayList<String> values = new java.util.ArrayList<>(length);
        int index = 0;
        while (index < length) {
            values.add(readString());
            index += 1;
        }
        return values;
    }

    boolean[] readBooleanArray() {
        int length = readCount();
        boolean[] values = new boolean[length];
        int index = 0;
        while (index < length) {
            values[index] = readBoolean();
            index += 1;
        }
        return values;
    }

    byte[] readByteArray() { return readBytes(); }

    short[] readShortArray() {
        int length = readArrayLength(2);
        short[] values = new short[length];
        int byteCount = length * 2;
        buffer.asShortBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    int[] readIntArray() {
        int length = readArrayLength(4);
        int[] values = new int[length];
        int byteCount = length * 4;
        buffer.asIntBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    long[] readLongArray() {
        int length = readArrayLength(8);
        long[] values = new long[length];
        int byteCount = length * 8;
        buffer.asLongBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    float[] readFloatArray() {
        int length = readArrayLength(4);
        float[] values = new float[length];
        int byteCount = length * 4;
        buffer.asFloatBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    double[] readDoubleArray() {
        int length = readArrayLength(8);
        double[] values = new double[length];
        int byteCount = length * 8;
        buffer.asDoubleBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    private int readLength() {
        int length = buffer.getInt();
        if (length < 0 || length > buffer.remaining()) {
            throw new IllegalArgumentException("invalid wire length");
        }
        return length;
    }

    private int readCount() {
        int count = buffer.getInt();
        if (count < 0) {
            throw new IllegalArgumentException("invalid wire count");
        }
        return count;
    }

    private int readArrayLength(int width) {
        int length = readCount();
        if (length > buffer.remaining() / width) {
            throw new IllegalArgumentException("invalid wire array length");
        }
        return length;
    }

    private int mapCapacity(int count) {
        return count < 3 ? count + 1 : Math.min(1 << 30, (int) Math.ceil(count / 0.75d));
    }
}

final class BoltFfiErrorBufferException extends RuntimeException {
    private final byte[] bytes;

    BoltFfiErrorBufferException(byte[] bytes) {
        super("BoltFFI call failed");
        this.bytes = bytes;
    }

    byte[] bytes() { return bytes; }
}

final class WireWriter {
    private final java.nio.ByteBuffer buffer;

    WireWriter(java.nio.ByteBuffer buffer) {
        this.buffer = buffer;
    }

    int size() { return buffer.position(); }
    void writeBoolean(boolean value) { buffer.put(value ? (byte) 1 : (byte) 0); }
    void writeByte(byte value) { buffer.put(value); }
    void writeShort(short value) { buffer.putShort(value); }
    void writeInt(int value) { buffer.putInt(value); }
    void writeLong(long value) { buffer.putLong(value); }
    void writeFloat(float value) { buffer.putFloat(value); }
    void writeDouble(double value) { buffer.putDouble(value); }

    void writeDuration(java.time.Duration value) {
        if (value.isNegative()) {
            throw new IllegalArgumentException("duration must be non-negative");
        }
        writeLong(value.getSeconds());
        writeInt(value.getNano());
    }

    void writeInstant(java.time.Instant value) {
        writeLong(value.getEpochSecond());
        writeInt(value.getNano());
    }

    void writeUuid(java.util.UUID value) {
        writeLong(value.getMostSignificantBits());
        writeLong(value.getLeastSignificantBits());
    }

    void writeUri(java.net.URI value) { writeString(value.toString()); }

    void writeString(String value) {
        writeBytes(value.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    void writeBytes(byte[] value) {
        writeInt(value.length);
        buffer.put(value);
    }

    <T> void writeOptional(java.util.Optional<T> value, WireWrite<T> write) {
        writeBoolean(value.isPresent());
        if (value.isPresent()) {
            write.write(value.get());
        }
    }

    <T> void writeSequence(java.util.List<T> values, WireWrite<T> write) {
        writeInt(values.size());
        int index = 0;
        while (index < values.size()) {
            write.write(values.get(index));
            index += 1;
        }
    }

    <K, V> void writeMap(
        java.util.Map<K, V> values,
        WireWrite<K> writeKey,
        WireWrite<V> writeValue
    ) {
        writeInt(values.size());
        java.util.Iterator<java.util.Map.Entry<K, V>> entries = values.entrySet().iterator();
        while (entries.hasNext()) {
            java.util.Map.Entry<K, V> entry = entries.next();
            writeKey.write(entry.getKey());
            writeValue.write(entry.getValue());
        }
    }

    void writeStringSequence(java.util.List<String> values) {
        writeInt(values.size());
        int index = 0;
        while (index < values.size()) {
            writeString(values.get(index));
            index += 1;
        }
    }

    void writeBooleanArray(boolean[] values) {
        writeInt(values.length);
        int index = 0;
        while (index < values.length) {
            writeBoolean(values[index]);
            index += 1;
        }
    }

    void writeByteArray(byte[] values) { writeBytes(values); }

    void writeShortArray(short[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 2);
        buffer.asShortBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeIntArray(int[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 4);
        buffer.asIntBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeLongArray(long[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 8);
        buffer.asLongBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeFloatArray(float[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 4);
        buffer.asFloatBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeDoubleArray(double[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 8);
        buffer.asDoubleBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }
}

final class WireSizes {
    private WireSizes() {}

    static int string(String value) {
        return Math.addExact(4, Math.multiplyExact(value.length(), 3));
    }

    static <T> int optional(java.util.Optional<T> value, WireSize<T> size) {
        return value.isPresent() ? Math.addExact(1, size.size(value.get())) : 1;
    }

    static <T> int sequence(java.util.List<T> values, WireSize<T> size) {
        int total = 4;
        int index = 0;
        while (index < values.size()) {
            total = Math.addExact(total, size.size(values.get(index)));
            index += 1;
        }
        return total;
    }

    static <K, V> int map(
        java.util.Map<K, V> values,
        WireSize<K> keySize,
        WireSize<V> valueSize
    ) {
        int total = 4;
        java.util.Iterator<java.util.Map.Entry<K, V>> entries = values.entrySet().iterator();
        while (entries.hasNext()) {
            java.util.Map.Entry<K, V> entry = entries.next();
            total = Math.addExact(total, keySize.size(entry.getKey()));
            total = Math.addExact(total, valueSize.size(entry.getValue()));
        }
        return total;
    }

    static int stringSequence(java.util.List<String> values) {
        int total = 4;
        int index = 0;
        while (index < values.size()) {
            total = Math.addExact(total, string(values.get(index)));
            index += 1;
        }
        return total;
    }

}

final class WireLease implements AutoCloseable {
    private final WireWriterPoolState owner;
    private final java.nio.ByteBuffer buffer;
    private final WireWriter writer;
    private boolean closed;

    WireLease(WireWriterPoolState owner, java.nio.ByteBuffer buffer) {
        this.owner = owner;
        this.buffer = buffer;
        this.writer = new WireWriter(buffer);
        reopen();
    }

    WireWriter writer() { return writer; }
    java.nio.ByteBuffer directBuffer() { return buffer; }
    int size() { return writer.size(); }
    int capacity() { return buffer.capacity(); }

    WireLease reopen() {
        buffer.clear();
        buffer.order(java.nio.ByteOrder.LITTLE_ENDIAN);
        closed = false;
        return this;
    }

    byte[] bytes() {
        java.nio.ByteBuffer source = buffer.duplicate();
        source.flip();
        byte[] bytes = new byte[source.remaining()];
        source.get(bytes);
        return bytes;
    }

    @Override
    public void close() {
        if (!closed) {
            closed = true;
            owner.release(this);
        }
    }
}

final class WireWriterPoolState {
    private static final int CACHE_SIZE = 4;
    private final java.util.ArrayDeque<WireLease> leases = new java.util.ArrayDeque<>(CACHE_SIZE);

    WireLease acquire(int capacity) {
        int required = Math.max(capacity, 1);
        int remaining = leases.size();
        while (remaining > 0) {
            WireLease candidate = leases.pollFirst();
            if (candidate.capacity() >= required) return candidate.reopen();
            leases.offerLast(candidate);
            remaining -= 1;
        }
        return new WireLease(this, java.nio.ByteBuffer.allocateDirect(required));
    }

    void release(WireLease lease) {
        if (leases.size() < CACHE_SIZE) {
            leases.addFirst(lease);
        }
    }
}

final class WireWriterPool {
    private static final ThreadLocal<WireWriterPoolState> STATE =
        ThreadLocal.withInitial(WireWriterPoolState::new);

    private WireWriterPool() {}

    static WireLease acquire(int capacity) {
        return STATE.get().acquire(capacity);
    }
}

public final class Tarwyn {
    private Tarwyn() {}
}