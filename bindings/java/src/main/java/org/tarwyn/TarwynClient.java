package org.tarwyn;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BOOLEAN;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.OptionalDouble;
import java.util.function.Consumer;
import org.wpilib.math.geometry.Pose2d;
import org.wpilib.math.geometry.Pose3d;
import org.wpilib.math.geometry.Quaternion;
import org.wpilib.math.geometry.Rotation2d;
import org.wpilib.math.geometry.Rotation3d;
import org.wpilib.math.geometry.Translation2d;

/**
 * One connection: publishes, reads, control and subscriptions.
 *
 * <p>Every method is safe to call from any thread. Reads return {@code null}
 * when the server does not answer within the request timeout, and publishing
 * without a server neither blocks nor throws. Subscription callbacks run on
 * the client's receive threads, never the caller's; anything they throw goes
 * to that thread's uncaught exception handler.
 */
public final class TarwynClient implements AutoCloseable {
    private final MemorySegment client;

    private TarwynClient(MemorySegment client) {
        this.client = client;
    }

    /** A client for a server on this machine. */
    public static TarwynClient create() {
        return new TarwynClient(call(arena -> (MemorySegment) Native.NEW.invokeExact()));
    }

    /** A client for the server on {@code host}, an address rather than a URL. */
    public static TarwynClient connect(String host) {
        return new TarwynClient(call(arena -> {
            Text text = Text.of(arena, host);
            return (MemorySegment) Native.CONNECT.invokeExact(text.ptr, text.len);
        }));
    }

    /** A client with every port and timeout spelled out. */
    public static TarwynClient withPorts(
        String host,
        short pushPort,
        short reqPort,
        short subPort,
        short telemetryPort,
        long requestTimeoutMs,
        int sendHighWaterMark
    ) {
        return new TarwynClient(call(arena -> {
            Text text = Text.of(arena, host);
            return (MemorySegment) Native.WITH_PORTS.invokeExact(text.ptr, text.len,
                pushPort, reqPort, subPort, telemetryPort, requestTimeoutMs, sendHighWaterMark);
        }));
    }

    /** Stops the client and cancels its subscriptions. */
    @Override
    public void close() {
        run(arena -> {
            Native.FREE.invokeExact(client);
        });
    }

    public void start() {
        run(arena -> {
            Native.START.invokeExact(client);
        });
    }

    public void stop() {
        run(arena -> {
            Native.STOP.invokeExact(client);
        });
    }

    public void putString(String channel, String value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Text text = Text.of(arena, value);
            Native.PUT_STRING.invokeExact(client, name.ptr, name.len, text.ptr, text.len);
        });
    }

    public void putInteger(String channel, int value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_INTEGER.invokeExact(client, name.ptr, name.len, value);
        });
    }

    public void putLong(String channel, long value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_LONG.invokeExact(client, name.ptr, name.len, value);
        });
    }

    public void putDouble(String channel, double value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_DOUBLE.invokeExact(client, name.ptr, name.len, value);
        });
    }

    public void putFloat(String channel, float value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_FLOAT.invokeExact(client, name.ptr, name.len, value);
        });
    }

    public void putBoolean(String channel, boolean value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_BOOLEAN.invokeExact(client, name.ptr, name.len, value);
        });
    }

    public void putBytes(String channel, byte[] value) {
        putBytes(Native.PUT_BYTES, channel, value);
    }

    public void putStringList(String channel, List<String> value) {
        putBytes(Native.PUT_STRING_LIST, channel, frame(value.stream().map(TarwynClient::utf8).toList()));
    }

    public void putBytesList(String channel, List<byte[]> value) {
        putBytes(Native.PUT_BYTES_LIST, channel, frame(value));
    }

    public void putDoubleList(String channel, double[] value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_DOUBLE_LIST.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_DOUBLE, value), (long) value.length);
        });
    }

    public void putFloatList(String channel, float[] value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_FLOAT_LIST.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_FLOAT, value), (long) value.length);
        });
    }

    public void putIntegerList(String channel, int[] value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_INTEGER_LIST.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_INT, value), (long) value.length);
        });
    }

    public void putLongList(String channel, long[] value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_LONG_LIST.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_LONG, value), (long) value.length);
        });
    }

    public void putBooleanList(String channel, boolean[] value) {
        byte[] bytes = new byte[value.length];
        for (int at = 0; at < value.length; at++) {
            bytes[at] = (byte) (value[at] ? 1 : 0);
        }
        putBytes(Native.PUT_BOOLEAN_LIST, channel, bytes);
    }

    public void putCoordinates(String channel, List<Translation2d> value) {
        double[] xy = new double[value.size() * 2];
        int at = 0;
        for (Translation2d point : value) {
            xy[at++] = point.getX();
            xy[at++] = point.getY();
        }
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_COORDINATES.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_DOUBLE, xy), (long) value.size());
        });
    }

    public void putPose2d(String channel, Pose2d pose) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_POSE2D.invokeExact(client, name.ptr, name.len,
                pose.getX(), pose.getY(), pose.getRotation().getRadians());
        });
    }

    public void putPose3d(String channel, Pose3d pose) {
        Quaternion q = pose.getRotation().getQuaternion();
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_POSE3D.invokeExact(client, name.ptr, name.len,
                pose.getX(), pose.getY(), pose.getZ(), q.getW(), q.getX(), q.getY(), q.getZ());
        });
    }

    public void putBezierCurve(String channel, List<Point> value) {
        double[] xyr = new double[value.size() * 3];
        int at = 0;
        for (Point point : value) {
            xyr[at++] = point.x();
            xyr[at++] = point.y();
            xyr[at++] = point.rotationDegrees().orElse(Double.NaN);
        }
        run(arena -> {
            Text name = Text.of(arena, channel);
            Native.PUT_BEZIER_CURVE.invokeExact(client, name.ptr, name.len,
                arena.allocateFrom(JAVA_DOUBLE, xyr), (long) value.size());
        });
    }

    /** {@code value} is an encoded protobuf {@code BezierCurves}; false when it is not. */
    public boolean putBezierCurves(String channel, byte[] value) {
        return putBytesReporting(Native.PUT_BEZIER_CURVES, channel, value);
    }

    /** {@code value} is an encoded protobuf {@code BezierCurvesList}; false when it is not. */
    public boolean putBezierCurvesList(String channel, byte[] value) {
        return putBytesReporting(Native.PUT_BEZIER_CURVES_LIST, channel, value);
    }

    /** False when {@code value} does not decode as {@code tarwynType}. */
    public boolean putTypedBytes(String channel, int tarwynType, byte[] value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.PUT_TYPED_BYTES.invokeExact(client, name.ptr, name.len,
                tarwynType, bytes(arena, value), (long) value.length);
        });
    }

    public void putUnknownBytes(String channel, byte[] value) {
        putBytes(Native.PUT_UNKNOWN_BYTES, channel, value);
    }

    /**
     * Publishes {@code packed} as a struct topic of {@code typeName}, announcing
     * each struct name and schema in {@code schemas} once.
     */
    public void putStruct(String channel, String typeName, Map<String, String> schemas, byte[] packed) {
        List<byte[]> flat = new ArrayList<>();
        schemas.forEach((name, schema) -> {
            flat.add(utf8(name));
            flat.add(utf8(schema));
        });
        byte[] framed = frame(flat);
        run(arena -> {
            Text name = Text.of(arena, channel);
            Text type = Text.of(arena, typeName);
            Native.PUT_STRUCT.invokeExact(client, name.ptr, name.len, type.ptr, type.len,
                bytes(arena, framed), (long) framed.length, bytes(arena, packed), (long) packed.length);
        });
    }

    public String getString(String channel) {
        byte[] bytes = read(Native.GET_STRING, channel);
        return bytes == null ? null : new String(bytes, StandardCharsets.UTF_8);
    }

    public Integer getInteger(String channel) {
        MemorySegment out = scalar(Native.GET_INTEGER, channel, JAVA_INT.byteSize());
        return out == null ? null : out.get(JAVA_INT, 0);
    }

    public Long getLong(String channel) {
        MemorySegment out = scalar(Native.GET_LONG, channel, JAVA_LONG.byteSize());
        return out == null ? null : out.get(JAVA_LONG, 0);
    }

    public Double getDouble(String channel) {
        MemorySegment out = scalar(Native.GET_DOUBLE, channel, JAVA_DOUBLE.byteSize());
        return out == null ? null : out.get(JAVA_DOUBLE, 0);
    }

    public Float getFloat(String channel) {
        MemorySegment out = scalar(Native.GET_FLOAT, channel, JAVA_FLOAT.byteSize());
        return out == null ? null : out.get(JAVA_FLOAT, 0);
    }

    public Boolean getBoolean(String channel) {
        MemorySegment out = scalar(Native.GET_BOOLEAN, channel, JAVA_BOOLEAN.byteSize());
        return out == null ? null : out.get(JAVA_BOOLEAN, 0);
    }

    public byte[] getBytes(String channel) {
        return read(Native.GET_BYTES, channel);
    }

    public List<String> getStringList(String channel) {
        byte[] framed = read(Native.GET_STRING_LIST, channel);
        return framed == null ? null : strings(unframe(framed));
    }

    public List<byte[]> getBytesList(String channel) {
        byte[] framed = read(Native.GET_BYTES_LIST, channel);
        return framed == null ? null : unframe(framed);
    }

    public double[] getDoubleList(String channel) {
        ByteBuffer packed = packed(Native.GET_DOUBLE_LIST, channel);
        if (packed == null) {
            return null;
        }
        double[] values = new double[packed.remaining() / Double.BYTES];
        packed.asDoubleBuffer().get(values);
        return values;
    }

    public float[] getFloatList(String channel) {
        ByteBuffer packed = packed(Native.GET_FLOAT_LIST, channel);
        if (packed == null) {
            return null;
        }
        float[] values = new float[packed.remaining() / Float.BYTES];
        packed.asFloatBuffer().get(values);
        return values;
    }

    public int[] getIntegerList(String channel) {
        ByteBuffer packed = packed(Native.GET_INTEGER_LIST, channel);
        if (packed == null) {
            return null;
        }
        int[] values = new int[packed.remaining() / Integer.BYTES];
        packed.asIntBuffer().get(values);
        return values;
    }

    public long[] getLongList(String channel) {
        ByteBuffer packed = packed(Native.GET_LONG_LIST, channel);
        if (packed == null) {
            return null;
        }
        long[] values = new long[packed.remaining() / Long.BYTES];
        packed.asLongBuffer().get(values);
        return values;
    }

    public boolean[] getBooleanList(String channel) {
        byte[] packed = read(Native.GET_BOOLEAN_LIST, channel);
        if (packed == null) {
            return null;
        }
        boolean[] values = new boolean[packed.length];
        for (int at = 0; at < packed.length; at++) {
            values[at] = packed[at] != 0;
        }
        return values;
    }

    public List<Translation2d> getCoordinates(String channel) {
        double[] xy = getDoubleList(Native.GET_COORDINATES, channel);
        if (xy == null) {
            return null;
        }
        List<Translation2d> points = new ArrayList<>(xy.length / 2);
        for (int at = 0; at + 1 < xy.length; at += 2) {
            points.add(new Translation2d(xy[at], xy[at + 1]));
        }
        return points;
    }

    public List<Point> getBezierCurve(String channel) {
        double[] xyr = getDoubleList(Native.GET_BEZIER_CURVE, channel);
        if (xyr == null) {
            return null;
        }
        List<Point> points = new ArrayList<>(xyr.length / 3);
        for (int at = 0; at + 2 < xyr.length; at += 3) {
            double rotation = xyr[at + 2];
            points.add(new Point(xyr[at], xyr[at + 1],
                Double.isNaN(rotation) ? OptionalDouble.empty() : OptionalDouble.of(rotation)));
        }
        return points;
    }

    /** The encoded protobuf {@code BezierCurves} on {@code channel}. */
    public byte[] getBezierCurves(String channel) {
        return read(Native.GET_BEZIER_CURVES, channel);
    }

    /** The encoded protobuf {@code BezierCurvesList} on {@code channel}. */
    public byte[] getBezierCurvesList(String channel) {
        return read(Native.GET_BEZIER_CURVES_LIST, channel);
    }

    public Pose2d getPose2d(String channel) {
        MemorySegment f = scalar(Native.GET_POSE2D, channel, JAVA_DOUBLE.byteSize() * 3);
        if (f == null) {
            return null;
        }
        return new Pose2d(f.getAtIndex(JAVA_DOUBLE, 0), f.getAtIndex(JAVA_DOUBLE, 1),
            new Rotation2d(f.getAtIndex(JAVA_DOUBLE, 2)));
    }

    public Pose3d getPose3d(String channel) {
        MemorySegment f = scalar(Native.GET_POSE3D, channel, JAVA_DOUBLE.byteSize() * 7);
        if (f == null) {
            return null;
        }
        return new Pose3d(f.getAtIndex(JAVA_DOUBLE, 0), f.getAtIndex(JAVA_DOUBLE, 1), f.getAtIndex(JAVA_DOUBLE, 2),
            new Rotation3d(new Quaternion(f.getAtIndex(JAVA_DOUBLE, 3), f.getAtIndex(JAVA_DOUBLE, 4),
                f.getAtIndex(JAVA_DOUBLE, 5), f.getAtIndex(JAVA_DOUBLE, 6))));
    }

    public byte[] getUnknownBytes(String channel) {
        return read(Native.GET_UNKNOWN_BYTES, channel);
    }

    /** How many channels were removed: 0 or 1. */
    public int delete(String channel) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (int) Native.DELETE.invokeExact(client, name.ptr, name.len);
        });
    }

    public int deleteAll() {
        return call(arena -> (int) Native.DELETE_ALL.invokeExact(client));
    }

    public List<String> getTables(String prefix) {
        return strings(unframe(read(Native.GET_TABLES, prefix)));
    }

    /** The round trip to the server in nanoseconds. */
    public Long getPing() {
        return call(arena -> {
            MemorySegment out = arena.allocate(JAVA_LONG);
            boolean present = (boolean) Native.GET_PING.invokeExact(client, out);
            return present ? out.get(JAVA_LONG, 0) : null;
        });
    }

    public ServerStatistics getServerStatistics() {
        return call(arena -> {
            MemorySegment out = arena.allocate(Native.STATISTICS);
            MemorySegment version = arena.allocate(ADDRESS);
            MemorySegment versionLength = arena.allocate(JAVA_LONG);
            boolean present = (boolean) Native.GET_SERVER_STATISTICS.invokeExact(client, out, version, versionLength);
            if (!present) {
                return null;
            }
            return new ServerStatistics(
                out.getAtIndex(JAVA_LONG, 0), out.getAtIndex(JAVA_LONG, 1), out.getAtIndex(JAVA_LONG, 2),
                out.getAtIndex(JAVA_LONG, 3), out.getAtIndex(JAVA_LONG, 4), out.getAtIndex(JAVA_LONG, 5),
                new String(take(version.get(ADDRESS, 0), versionLength.get(JAVA_LONG, 0)), StandardCharsets.UTF_8));
        });
    }

    /** The JSON of everything under {@code prefix}; {@code {}} when the server is absent. */
    public String getRawJson(String prefix) {
        return new String(read(Native.GET_RAW_JSON, prefix), StandardCharsets.UTF_8);
    }

    public boolean compareAndSetAbsentString(String channel, String value) {
        return putBytesReporting(Native.CAS_ABSENT_STRING, channel, utf8(value));
    }

    public boolean compareAndSetString(String channel, String expected, String value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            Text was = Text.of(arena, expected);
            Text text = Text.of(arena, value);
            return (boolean) Native.CAS_STRING.invokeExact(
                client, name.ptr, name.len, was.ptr, was.len, text.ptr, text.len);
        });
    }

    public boolean compareAndSetDouble(String channel, double expected, double value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.CAS_DOUBLE.invokeExact(client, name.ptr, name.len, expected, value);
        });
    }

    public boolean compareAndSetLong(String channel, long expected, long value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.CAS_LONG.invokeExact(client, name.ptr, name.len, expected, value);
        });
    }

    public boolean compareAndSetBoolean(String channel, boolean expected, boolean value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.CAS_BOOLEAN.invokeExact(client, name.ptr, name.len, expected, value);
        });
    }

    public void publishTelemetry(String channel, byte[] payload) {
        putBytes(Native.PUBLISH_TELEMETRY, channel, payload);
    }

    public boolean logTo(String path) {
        return call(arena -> {
            Text text = Text.of(arena, path);
            return (boolean) Native.LOG_TO.invokeExact(client, text.ptr, text.len);
        });
    }

    /** The path the log landed at, or {@code null} when it could not be opened. */
    public String logToDrive(String filename) {
        byte[] path = read(Native.LOG_TO_DRIVE, filename);
        return path == null ? null : new String(path, StandardCharsets.UTF_8);
    }

    public long droppedLogRecords() {
        return call(arena -> (long) Native.DROPPED_LOG_RECORDS.invokeExact(client));
    }

    public boolean loggingHealthy() {
        return call(arena -> (boolean) Native.LOGGING_HEALTHY.invokeExact(client));
    }

    public long droppedPublishes() {
        return call(arena -> (long) Native.DROPPED_PUBLISHES.invokeExact(client));
    }

    /** Calls back with each value on {@code channel}; false when it already has a subscription. */
    public boolean subscribe(String channel, Consumer<Sample> callback) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.SUBSCRIBE.invokeExact(client, name.ptr, name.len,
                Native.ON_SAMPLE, Native.register(callback), Native.ON_DROP);
        });
    }

    public boolean unsubscribe(String channel) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.UNSUBSCRIBE.invokeExact(client, name.ptr, name.len);
        });
    }

    /**
     * Calls back with each telemetry sample on {@code channel}; false when it
     * already has a subscription or the telemetry plane refused it.
     */
    public boolean subscribeTelemetry(String channel, Consumer<TelemetrySample> callback) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.SUBSCRIBE_TELEMETRY.invokeExact(client, name.ptr, name.len,
                Native.ON_TELEMETRY, Native.registerTelemetry(callback), Native.ON_DROP);
        });
    }

    public boolean unsubscribeTelemetry(String channel) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) Native.UNSUBSCRIBE_TELEMETRY.invokeExact(client, name.ptr, name.len);
        });
    }

    /** Calls back with each server log line on the channel {@code logs}; false when already subscribed. */
    public boolean subscribeToLogs(Consumer<Sample> callback) {
        return call(arena -> (boolean) Native.SUBSCRIBE_TO_LOGS.invokeExact(
            client, Native.ON_SAMPLE, Native.register(callback), Native.ON_DROP));
    }

    public boolean unsubscribeFromLogs() {
        return call(arena -> (boolean) Native.UNSUBSCRIBE_FROM_LOGS.invokeExact(client));
    }

    private interface Call<T> {
        T run(Arena arena) throws Throwable;
    }

    private interface Action {
        void run(Arena arena) throws Throwable;
    }

    /** A string as the library takes it: UTF-8 bytes in {@code arena} and their length. */
    private record Text(MemorySegment ptr, long len) {
        static Text of(Arena arena, String text) {
            byte[] bytes = utf8(text);
            return new Text(TarwynClient.bytes(arena, bytes), bytes.length);
        }
    }

    private static <T> T call(Call<T> call) {
        try (Arena arena = Arena.ofConfined()) {
            return call.run(arena);
        } catch (RuntimeException | Error failure) {
            throw failure;
        } catch (Throwable failure) {
            throw new IllegalStateException("the native client failed", failure);
        }
    }

    private static void run(Action action) {
        call(arena -> {
            action.run(arena);
            return null;
        });
    }

    private void putBytes(MethodHandle put, String channel, byte[] value) {
        run(arena -> {
            Text name = Text.of(arena, channel);
            put.invokeExact(client, name.ptr, name.len, bytes(arena, value), (long) value.length);
        });
    }

    private boolean putBytesReporting(MethodHandle put, String channel, byte[] value) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            return (boolean) put.invokeExact(client, name.ptr, name.len, bytes(arena, value), (long) value.length);
        });
    }

    private byte[] read(MethodHandle get, String channel) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            MemorySegment length = arena.allocate(JAVA_LONG);
            MemorySegment pointer = (MemorySegment) get.invokeExact(client, name.ptr, name.len, length);
            return pointer.address() == 0 ? null : take(pointer, length.get(JAVA_LONG, 0));
        });
    }

    private MemorySegment scalar(MethodHandle get, String channel, long size) {
        return call(arena -> {
            Text name = Text.of(arena, channel);
            MemorySegment out = Arena.ofAuto().allocate(size);
            boolean present = (boolean) get.invokeExact(client, name.ptr, name.len, out);
            return present ? out : null;
        });
    }

    private ByteBuffer packed(MethodHandle get, String channel) {
        byte[] bytes = read(get, channel);
        return bytes == null ? null : ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder());
    }

    private double[] getDoubleList(MethodHandle get, String channel) {
        ByteBuffer packed = packed(get, channel);
        if (packed == null) {
            return null;
        }
        double[] values = new double[packed.remaining() / Double.BYTES];
        packed.asDoubleBuffer().get(values);
        return values;
    }

    private static byte[] take(MemorySegment pointer, long length) throws Throwable {
        byte[] bytes = Native.bytes(pointer, length);
        Native.BYTES_FREE.invokeExact(pointer, length);
        return bytes;
    }

    private static byte[] utf8(String text) {
        return text.getBytes(StandardCharsets.UTF_8);
    }

    private static MemorySegment bytes(Arena arena, byte[] bytes) {
        return arena.allocateFrom(JAVA_BYTE, bytes);
    }

    private static List<String> strings(List<byte[]> items) {
        return items.stream().map(item -> new String(item, StandardCharsets.UTF_8)).toList();
    }

    private static byte[] frame(List<byte[]> items) {
        int size = 0;
        for (byte[] item : items) {
            size += Integer.BYTES + item.length;
        }
        ByteBuffer out = ByteBuffer.allocate(size).order(ByteOrder.nativeOrder());
        for (byte[] item : items) {
            out.putInt(item.length).put(item);
        }
        return out.array();
    }

    private static List<byte[]> unframe(byte[] framed) {
        List<byte[]> items = new ArrayList<>();
        ByteBuffer in = ByteBuffer.wrap(framed).order(ByteOrder.nativeOrder());
        while (in.remaining() >= Integer.BYTES) {
            int length = in.getInt();
            if (length < 0 || length > in.remaining()) {
                break;
            }
            byte[] item = new byte[length];
            in.get(item);
            items.add(item);
        }
        return items;
    }
}
