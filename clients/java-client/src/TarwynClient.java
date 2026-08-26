import static tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

public final class TarwynClient extends TarwynApi implements AutoCloseable {
    private final MemorySegment scratch;
    private final ConcurrentHashMap<Consumer<byte[]>, Poller> pollers = new ConcurrentHashMap<>();
    private ScheduledExecutorService pollExecutor;
    private boolean closed = false;

    public TarwynClient(Path library, String host) {
        this(library, host, 5557, 5556, 5555, 500, 500);
    }

    public TarwynClient(String host) {
        this(TarwynClientManager.defaultLibrary(), host);
    }

    public boolean subscribe(String channel, Consumer<byte[]> consumer) {
        return subscribe(channel, consumer, 256, 4096, 10);
    }

    public boolean subscribe(String channel, Consumer<byte[]> consumer,
                             int records, int recordBytes, long pollMillis) {
        if (pollers.containsKey(consumer)) {
            return false;
        }
        Subscription subscription = subscribe(channel, records, recordBytes);
        synchronized (this) {
            if (pollExecutor == null) {
                pollExecutor = Executors.newSingleThreadScheduledExecutor(runnable -> {
                    Thread thread = new Thread(runnable, "tarwyn-subscribe-poll");
                    thread.setDaemon(true);
                    return thread;
                });
            }
        }
        ScheduledFuture<?> task = pollExecutor.scheduleAtFixedRate(() -> {
            try {
                for (byte[] payload : subscription.drain()) {
                    consumer.accept(payload);
                }
            } catch (RuntimeException ignored) {
                return;
            }
        }, pollMillis, pollMillis, TimeUnit.MILLISECONDS);
        pollers.put(consumer, new Poller(subscription, task));
        return true;
    }

    public boolean unsubscribe(Consumer<byte[]> consumer) {
        Poller poller = pollers.remove(consumer);
        if (poller == null) {
            return false;
        }
        poller.task.cancel(false);
        poller.subscription.close();
        return true;
    }

    public void shutdown() {
        close();
    }

    private record Poller(Subscription subscription, ScheduledFuture<?> task) {}

    public TarwynClient(Path library, String host, int pushPort, int reqPort, int subPort,
                         long requestTimeoutMillis, int sendHighWaterMark) {
        System.load(library.toAbsolutePath().toString());
        this.arena = Arena.ofShared();
        this.scratch = arena.allocate(ValueLayout.JAVA_LONG);
        this.handle = xt_client_new(
            arena.allocateFrom(host), (short) pushPort, (short) reqPort, (short) subPort,
            requestTimeoutMillis, sendHighWaterMark);
        if (handle.equals(MemorySegment.NULL)) {
            arena.close();
            throw new IllegalStateException("native client construction returned null");
        }
    }

    public void start() {
        check(xt_client_start(handle), "start");
    }

    public void publish(String channel, double value) {
        MemorySegment name = arena.allocateFrom(channel);
        check(xt_publish_double(handle, name, value), "publish double");
    }

    public void publish(String channel, boolean value) {
        MemorySegment name = arena.allocateFrom(channel);
        check(xt_publish_bool(handle, name, value), "publish bool");
    }

    public void publish(String channel, String value) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment body = arena.allocateFrom(value);
        check(xt_publish_string(handle, name, body), "publish string");
    }

    public void publish(String channel, byte[] value) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment body = arena.allocateFrom(ValueLayout.JAVA_BYTE, value);
        check(xt_publish_bytes(handle, name, body, (long) value.length), "publish bytes");
    }

    public long droppedPublishes() {
        check(xt_dropped_publishes(handle, scratch), "dropped publishes");
        return scratch.get(ValueLayout.JAVA_LONG, 0);
    }

    public void logTo(Path path) {
        MemorySegment name = arena.allocateFrom(path.toAbsolutePath().toString());
        check(xt_log_to(handle, name), "start logging");
    }

    public String logToDrive(String filename) {
        MemorySegment name = arena.allocateFrom(filename);
        MemorySegment out = arena.allocate(4096);
        check(xt_log_to_drive(handle, name, out, 4096), "start logging to a drive");
        return out.getString(0);
    }

    public long droppedLogRecords() {
        check(xt_log_dropped(handle, scratch), "dropped log records");
        return scratch.get(ValueLayout.JAVA_LONG, 0);
    }

    public boolean loggingHealthy() {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_BOOLEAN);
        check(xt_logging_healthy(handle, out), "logging health");
        return out.get(ValueLayout.JAVA_BOOLEAN, 0);
    }

    public Subscription subscribe(String channel, int records, int recordBytes) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment out = arena.allocate(ValueLayout.JAVA_LONG);
        check(xt_subscribe_ring(handle, name, records, recordBytes, out), "subscribe");
        long id = out.get(ValueLayout.JAVA_LONG, 0);
        return new Subscription(id, records, recordBytes);
    }

    @Override
    protected void check(int code, String what) {
        if (code != XT_OK()) {
            throw new IllegalStateException(what + " failed: " + describe(code));
        }
    }

    private static String describe(int code) {
        if (code == XT_ERR_NULL()) return "null pointer or poisoned lock";
        if (code == XT_ERR_UTF8()) return "argument was not valid UTF-8";
        if (code == XT_ERR_NO_VALUE()) return "no value, or no such subscription";
        if (code == XT_ERR_WRONG_TYPE()) return "channel holds a different type";
        if (code == XT_ERR_PANIC()) return "a panic was caught at the boundary";
        if (code == XT_ERR_IO()) return "the log file or drive could not be written";
        return "unknown code " + code;
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }
        closed = true;
        for (Consumer<byte[]> consumer : List.copyOf(pollers.keySet())) {
            unsubscribe(consumer);
        }
        if (pollExecutor != null) {
            pollExecutor.shutdownNow();
        }
        try {
            xt_client_free(handle);
        } finally {
            arena.close();
        }
    }

    public final class Subscription implements AutoCloseable {
        private final long id;
        private final int records;
        private final int recordBytes;
        private final MemorySegment ring;
        private long readIndex = 0;
        private boolean released = false;

        private Subscription(long id, int records, int recordBytes) {
            this.id = id;
            this.records = records;
            this.recordBytes = recordBytes;
            MemorySegment base;
            try {
                base = xt_ring_base(handle, id);
            } catch (Throwable t) {
                throw new IllegalStateException("could not obtain the ring base address", t);
            }
            if (base.equals(MemorySegment.NULL)) {
                throw new IllegalStateException("ring base address was null");
            }
            this.ring = base.reinterpret((long) records * recordBytes);
        }

        public long writeIndex() {
            requireLive();
            MemorySegment out = arena.allocate(ValueLayout.JAVA_LONG);
check(xt_ring_write_index(handle, id, out), "read write index");
            return out.get(ValueLayout.JAVA_LONG, 0);
        }

        public List<byte[]> drain() {
            requireLive();
            long available = writeIndex();
            List<byte[]> values = new ArrayList<>();
            if (available <= readIndex) {
                return values;
            }
            long from = Math.max(readIndex, available - records);
            for (long index = from; index < available; index++) {
                long offset = (index % records) * (long) recordBytes;
                int length = (int) ring.get(ValueLayout.JAVA_LONG, offset);
                if (length < 0 || length > recordBytes - 8) {
                    continue;
                }
                byte[] payload = new byte[length];
                MemorySegment.copy(ring, ValueLayout.JAVA_BYTE, offset + 8, payload, 0, length);
                if (writeIndex() - index <= records) {
                    values.add(payload);
                }
            }
            readIndex = available;
            return values;
        }

        public boolean lapped() {
            return writeIndex() - readIndex > records;
        }

        private void requireLive() {
            if (released || closed) {
                throw new IllegalStateException(
                    "the ring is freed once the subscription or client is closed");
            }
        }

        @Override
        public void close() {
            if (released || closed) {
                return;
            }
            released = true;
check(xt_unsubscribe(handle, id), "unsubscribe");
        }
    }

    public static String utf8(byte[] value) {
        return new String(value, StandardCharsets.UTF_8);
    }
}
