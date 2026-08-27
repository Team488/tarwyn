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

/**
 * A connection to an TARWYN server, over the native client through FFM.
 *
 * Method names match the original TARWYN client, so {@code put}/{@code get}
 * call sites port across unchanged. Constructing it never blocks - ZeroMQ dials
 * in the background - and nothing is received until {@link #start()}.
 *
 * {@snippet :
 * try (TarwynClient client = new TarwynClient("127.0.0.1")) {
 *     client.start();
 *     client.putDouble("pose", 1.5);
 * }
 * }
 *
 * Requires {@code --enable-native-access} on the module or the unnamed module.
 * Closing it releases the native handle; any {@link Subscription} it handed out
 * stops working at that point.
 */
public final class TarwynClient extends TarwynApi implements AutoCloseable {
    private final MemorySegment scratch;
    private final ConcurrentHashMap<Consumer<byte[]>, Poller> pollers = new ConcurrentHashMap<>();
    private ScheduledExecutorService pollExecutor;
    private boolean closed = false;

    /**
     * Connect to {@code host} on the default ports, loading the native library from
     * {@code library}.
     *
     * @param library the native library to load
     * @param host the machine running the server
     */
    public TarwynClient(Path library, String host) {
        this(library, host, 5557, 5556, 5555, 500, 500);
    }

    /**
     * Connect to {@code host} on the default ports, extracting the native library
     * bundled in the jar.
     *
     * @param host the machine running the server
     */
    public TarwynClient(String host) {
        this(TarwynClientManager.defaultLibrary(), host);
    }

    /**
     * Call {@code consumer} with every payload published to {@code channel}, polling
     * on a shared daemon thread every 10 ms.
     *
     * @param channel the channel to watch
     * @param consumer what to run for each payload
     * @return false if this consumer is already subscribed
     */
    public boolean subscribe(String channel, Consumer<byte[]> consumer) {
        return subscribe(channel, consumer, 256, 4096, 10);
    }

    /**
     * As {@link #subscribe(String, Consumer)}, with the ring size and poll interval
     * spelled out.
     *
     * A ring that laps between polls loses the values it overwrote, so size it for
     * the publish rate: {@code records} slots must outlast {@code pollMillis}.
     *
     * @param channel the channel to watch
     * @param consumer what to run for each payload
     * @param records how many payloads the ring holds
     * @param recordBytes the size of each slot, including an 8-byte length prefix
     * @param pollMillis how often the ring is drained
     * @return false if this consumer is already subscribed
     */
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

    /**
     * Stop polling for a consumer and release its ring.
     *
     * @param consumer the consumer passed to subscribe
     * @return false if it was not subscribed
     */
    public boolean unsubscribe(Consumer<byte[]> consumer) {
        Poller poller = pollers.remove(consumer);
        if (poller == null) {
            return false;
        }
        poller.task.cancel(false);
        poller.subscription.close();
        return true;
    }

    /**
     * Close the client. Equivalent to {@link #close()}, named to match the original
     * TARWYN client.
     */
    public void shutdown() {
        close();
    }

    private record Poller(Subscription subscription, ScheduledFuture<?> task) {}

    /**
     * Connect with the ports, request timeout and high-water mark spelled out.
     *
     * @param library the native library to load
     * @param host the machine running the server
     * @param pushPort the port publishes are sent to
     * @param reqPort the port reads and control commands are sent to
     * @param subPort the port subscriptions are received on
     * @param requestTimeoutMillis how long a read waits before giving up
     * @param sendHighWaterMark how many publishes may queue before they are dropped
     * @throws IllegalStateException if the native client could not be constructed
     */
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

    /**
     * Start the receive threads, so subscriptions begin delivering.
     *
     * Publishing and reading work without this.
     */
    public void start() {
        check(xt_client_start(handle), "start");
    }

    /**
     * Publish a double on the UDP telemetry plane, which trades delivery guarantees
     * for latency.
     *
     * @param channel the channel to publish to
     * @param value the value to publish
     */
    public void publish(String channel, double value) {
        check(xt_publish_double(handle, channel(channel), value), "publish double");
    }

    /**
     * Publish a boolean on the UDP telemetry plane.
     *
     * @param channel the channel to publish to
     * @param value the value to publish
     */
    public void publish(String channel, boolean value) {
        check(xt_publish_bool(handle, channel(channel), value), "publish bool");
    }

    /**
     * Publish a string on the UDP telemetry plane.
     *
     * @param channel the channel to publish to
     * @param value the value to publish
     */
    public void publish(String channel, String value) {
        try (Arena call = Arena.ofConfined()) {
            check(xt_publish_string(handle, channel(channel), call.allocateFrom(value)),
                "publish string");
        }
    }

    /**
     * Publish raw bytes on the UDP telemetry plane.
     *
     * A datagram that cannot be sent is counted by {@link #droppedPublishes()},
     * not retried.
     *
     * @param channel the channel to publish to
     * @param value the payload
     */
    public void publish(String channel, byte[] value) {
        putBytes(channel, value);
    }

    /**
     * Publish raw bytes over ZeroMQ, which is reliable and framed.
     *
     * @param channel the channel to publish to
     * @param value the payload
     */
    public void putBytes(String channel, byte[] value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, value);
            check(xt_publish_bytes(handle, channel(channel), body, (long) value.length),
                "putBytes");
        }
    }

    /**
     * Read the bytes on {@code channel}.
     *
     * @param channel the channel to read
     * @return the payload, or null if the channel is unset or holds another type
     */
    public byte[] getBytes(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_bytes(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBytes");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_bytes(handle, channel(channel), out, needed, size), "getBytes");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed).toArray(ValueLayout.JAVA_BYTE);
        }
    }

    /**
     * Delete {@code channel}.
     *
     * @param channel the channel to delete
     * @return how many were removed - 0 or 1
     */
    public int delete(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_INT);
            check(xt_delete(handle, channel(channel), out), "delete");
            return out.get(ValueLayout.JAVA_INT, 0);
        }
    }

    /**
     * Delete every channel.
     *
     * @return how many were removed
     */
    public int deleteAll() {
        return delete("");
    }

    /**
     * Every channel name the server holds.
     *
     * @return the channel names
     */
    public String[] getTables() {
        return getTables("");
    }

    /**
     * The channel names beginning with {@code prefix}.
     *
     * @param prefix the prefix to match; pass "" for all of them
     * @return the channel names
     */
    public String[] getTables(String prefix) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 8192;
            MemorySegment out = call.allocate(capacity);
            check(xt_tables(handle, channel(prefix), out, capacity, size), "getTables");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_tables(handle, channel(prefix), out, needed, size), "getTables");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            java.nio.ByteBuffer buffer = out.asSlice(0, needed).asByteBuffer()
                .order(java.nio.ByteOrder.LITTLE_ENDIAN);
            String[] channels = new String[buffer.getInt()];
            for (int index = 0; index < channels.length; index++) {
                byte[] item = new byte[buffer.getInt()];
                buffer.get(item);
                channels[index] = new String(item, StandardCharsets.UTF_8);
            }
            return channels;
        }
    }

    /**
     * Round-trip time to the server, in nanoseconds.
     *
     * @return the round trip, or -1 if the server did not answer
     */
    public long getPing() {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_LONG);
            int code = xt_ping(handle, out);
            if (code == XT_ERR_NO_VALUE()) {
                return -1;
            }
            check(code, "getPing");
            return out.get(ValueLayout.JAVA_LONG, 0);
        }
    }

    /**
     * The server's counters.
     *
     * @return the statistics, or null if the server did not answer
     */
    public ServerStatistics getServerStatistics() {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment fields = call.allocate(ValueLayout.JAVA_LONG, 4);
            MemorySegment version = call.allocate(64);
            int code = xt_statistics(handle, fields, 4, version, 64);
            if (code == XT_ERR_NO_VALUE()) {
                return null;
            }
            check(code, "getServerStatistics");
            return new ServerStatistics(
                fields.getAtIndex(ValueLayout.JAVA_LONG, 0),
                fields.getAtIndex(ValueLayout.JAVA_LONG, 1),
                fields.getAtIndex(ValueLayout.JAVA_LONG, 2),
                fields.getAtIndex(ValueLayout.JAVA_LONG, 3),
                version.getString(0));
        }
    }

    /**
     * Every channel the server holds, as a JSON document.
     *
     * @return the document, or "{@code {}}" if the server did not answer
     */
    public String getRawJson() {
        return getRawJson("");
    }

    /**
     * The channels beginning with {@code prefix}, as a JSON document.
     *
     * @param prefix the prefix to match; pass "" for all of them
     * @return the document, or "{@code {}}" if the server did not answer
     */
    public String getRawJson(String prefix) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 16384;
            MemorySegment out = call.allocate(capacity);
            check(xt_raw_json(handle, channel(prefix), out, capacity, size), "getRawJson");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_raw_json(handle, channel(prefix), out, needed, size), "getRawJson");
            }
            return out.getString(0);
        }
    }

    /**
     * Publish a list of coordinates, flat - {@code x}, {@code y}, {@code x}, {@code y}.
     *
     * @param channel the channel to publish to
     * @param xy the coordinates, of even length
     */
    public void putCoordinates(String channel, double[] xy) {
        if (xy.length % 2 != 0) {
            throw new IllegalArgumentException("coordinates come in x,y pairs");
        }
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_DOUBLE, xy);
            check(xt_put_coordinates(handle, channel(channel), body, (long) xy.length),
                "putCoordinates");
        }
    }

    /**
     * Read the coordinate list on {@code channel}, flat.
     *
     * @param channel the channel to read
     * @return the coordinates, or null if the channel is unset or holds another type
     */
    public double[] getCoordinates(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 512;
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, capacity);
            int code = xt_get_coordinates(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getCoordinates");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_DOUBLE, needed);
                check(xt_get_coordinates(handle, channel(channel), out, needed, size),
                    "getCoordinates");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * 8).toArray(ValueLayout.JAVA_DOUBLE);
        }
    }

    /**
     * Publish a bezier path as encoded protobuf.
     *
     * Byte-identical to TARWYN' own encoding, so a {@code BezierCurves} built with
     * its generated classes passes straight through {@code toByteArray()}.
     *
     * @param channel the channel to publish to
     * @param encoded the encoded BezierCurves message
     */
    public void putBezierCurves(String channel, byte[] encoded) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, encoded);
            check(xt_put_bezier_curves(handle, channel(channel), body, (long) encoded.length),
                "putBezierCurves");
        }
    }

    /**
     * Read the bezier path on {@code channel} as encoded protobuf.
     *
     * @param channel the channel to read
     * @return the encoded message, or null if the channel is unset or holds another type
     */
    public byte[] getBezierCurves(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 8192;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_bezier_curves(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBezierCurves");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_bezier_curves(handle, channel(channel), out, needed, size),
                    "getBezierCurves");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed).toArray(ValueLayout.JAVA_BYTE);
        }
    }

    /**
     * Publish one bezier curve as encoded protobuf.
     *
     * @param channel the channel to publish to
     * @param encoded the encoded BezierCurve message
     */
    public void putBezierCurve(String channel, byte[] encoded) {
        putEncoded(channel, encoded, "putBezierCurve", true);
    }

    /**
     * Read the bezier curve on {@code channel} as encoded protobuf.
     *
     * @param channel the channel to read
     * @return the encoded message, or null if the channel is unset or holds another type
     */
    public byte[] getBezierCurve(String channel) {
        return getEncoded(channel, "getBezierCurve", true);
    }

    /**
     * Publish a list of bezier paths as encoded protobuf.
     *
     * @param channel the channel to publish to
     * @param encoded the encoded BezierCurvesList message
     */
    public void putBezierCurvesList(String channel, byte[] encoded) {
        putEncoded(channel, encoded, "putBezierCurvesList", false);
    }

    /**
     * Read the list of bezier paths on {@code channel} as encoded protobuf.
     *
     * @param channel the channel to read
     * @return the encoded message, or null if the channel is unset or holds another type
     */
    public byte[] getBezierCurvesList(String channel) {
        return getEncoded(channel, "getBezierCurvesList", false);
    }

    /**
     * Publish bytes whose type the caller does not know. Equivalent to
     * {@link #putBytes(String, byte[])}; present to match TARWYN.
     *
     * @param channel the channel to publish to
     * @param value the payload
     */
    public void putUnknownBytes(String channel, byte[] value) {
        putBytes(channel, value);
    }

    /**
     * Read raw bytes from {@code channel}.
     *
     * @param channel the channel to read
     * @return the payload, or null if the channel is unset or holds another type
     */
    public byte[] getUnknownBytes(String channel) {
        return getBytes(channel);
    }

    /**
     * Publish a value already encoded in TARWYN' own byte layout.
     *
     * Scalars are big-endian, matching {@code ByteBuffer}'s default; the list and
     * geometry types are protobuf.
     *
     * @param channel the channel to publish to
     * @param tarwynType the TARWYN type tag
     * @param value the encoded value
     * @return false, publishing nothing, if the tag is unknown or the bytes do not decode as that type
     */
    public boolean putTypedBytes(String channel, int tarwynType, byte[] value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, value);
            int code = xt_put_typed_bytes(handle, channel(channel), tarwynType, body,
                (long) value.length);
            if (code == XT_ERR_WRONG_TYPE()) {
                return false;
            }
            check(code, "putTypedBytes");
            return true;
        }
    }

    private void putEncoded(String channel, byte[] encoded, String what, boolean single) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, encoded);
            int code = single
                ? xt_put_bezier_curve(handle, channel(channel), body, (long) encoded.length)
                : xt_put_bezier_curves_list(handle, channel(channel), body, (long) encoded.length);
            check(code, what);
        }
    }

    private byte[] getEncoded(String channel, String what, boolean single) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 8192;
            MemorySegment out = call.allocate(capacity);
            int code = single
                ? xt_get_bezier_curve(handle, channel(channel), out, capacity, size)
                : xt_get_bezier_curves_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, what);
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                code = single
                    ? xt_get_bezier_curve(handle, channel(channel), out, needed, size)
                    : xt_get_bezier_curves_list(handle, channel(channel), out, needed, size);
                check(code, what);
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed).toArray(ValueLayout.JAVA_BYTE);
        }
    }

    /**
     * The server's counters, as returned by {@link #getServerStatistics()}.
     *
     * @param channels how many channels hold a value
     * @param values how many values have been stored
     * @param telemetrySubscribers how many telemetry registrations are live
     * @param uptimeSeconds how long the server has been running
     * @param version the server's version
     */
    public record ServerStatistics(long channels, long values, long telemetrySubscribers,
                                   long uptimeSeconds, String version) {}

    /**
     * How many publishes were dropped rather than queued, across both transports.
     *
     * @return the count
     */
    public long droppedPublishes() {
        check(xt_dropped_publishes(handle, scratch), "dropped publishes");
        return scratch.get(ValueLayout.JAVA_LONG, 0);
    }

    /**
     * Mirror every published value into a WPILOG file, which AdvantageScope, Elastic
     * and the WPILib DataLogTool open directly.
     *
     * Records cross a bounded queue and are flushed every 250 ms, so a publish never
     * waits on the filesystem.
     *
     * @param path where to write the log
     * @throws IllegalStateException if logging has already started or the file cannot be opened
     */
    public void logTo(Path path) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment name = call.allocateFrom(path.toAbsolutePath().toString());
            check(xt_log_to(handle, name), "start logging");
        }
    }

    /**
     * As {@link #logTo(Path)}, but onto the first writable removable drive that
     * accepts the file.
     *
     * @param filename the name to give the log
     * @return the path chosen
     * @throws IllegalStateException if no removable drive accepted it
     */
    public String logToDrive(String filename) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment name = call.allocateFrom(filename);
            MemorySegment out = call.allocate(4096);
            check(xt_log_to_drive(handle, name, out, 4096), "start logging to a drive");
            return out.getString(0);
        }
    }

    /**
     * How many log records were dropped because the writer queue was full.
     *
     * @return the count, or 0 if logging was never started
     */
    public long droppedLogRecords() {
        check(xt_log_dropped(handle, scratch), "dropped log records");
        return scratch.get(ValueLayout.JAVA_LONG, 0);
    }

    /**
     * Whether the log writer is still succeeding.
     *
     * An I/O error latches the writer off rather than throwing into a publish, so
     * this is the only way to notice.
     *
     * @return true when the writer is healthy, or logging was never started
     */
    public boolean loggingHealthy() {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(xt_logging_healthy(handle, out), "logging health");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Subscribe to {@code channel}, delivering payloads into a ring the caller drains
     * itself.
     *
     * Use {@link #subscribe(String, Consumer)} instead unless you want to control
     * when draining happens.
     *
     * @param channel the channel to watch
     * @param records how many payloads the ring holds
     * @param recordBytes the size of each slot, including an 8-byte length prefix
     * @return the subscription, which must be closed
     */
    public Subscription subscribe(String channel, int records, int recordBytes) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_LONG);
            check(xt_subscribe_ring(handle, channel(channel), records, recordBytes, out), "subscribe");
            long id = out.get(ValueLayout.JAVA_LONG, 0);
            return new Subscription(id, records, recordBytes);
        }
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
    /**
     * Stop the client, cancel every subscription, and release the native handle.
     *
     * Any {@link Subscription} it handed out stops working at that point.
     */
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

    /**
     * A ring of payloads written by the native client and read here directly.
     *
     * The bytes are read out of the mapped segment without copying through the FFI,
     * which is what keeps a subscription cheap. A writer that laps the reader
     * overwrites slots it has not drained yet; {@link #lapped()} reports that.
     */
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

        /**
         * How many payloads have been pushed into the ring since it was created.
         *
         * Every slot below the returned index is fully written.
         *
         * @return the write index
         */
        public long writeIndex() {
            requireLive();
            try (Arena call = Arena.ofConfined()) {
                MemorySegment out = call.allocate(ValueLayout.JAVA_LONG);
                check(xt_ring_write_index(handle, id, out), "read write index");
                return out.get(ValueLayout.JAVA_LONG, 0);
            }
        }

        /**
         * Take every payload written since the last drain.
         *
         * Payloads the writer overwrote while this was copying are left out rather than
         * returned torn, so a lapped ring returns fewer values than were published.
         *
         * @return the payloads, oldest first
         */
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

        /**
         * Whether the writer has overwritten payloads this subscription never drained.
         *
         * @return true when values were lost
         */
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

    /**
     * Decode a payload as a UTF-8 string.
     *
     * @param value the payload
     * @return the decoded string
     */
    public static String utf8(byte[] value) {
        return new String(value, StandardCharsets.UTF_8);
    }
}
