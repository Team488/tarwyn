package org.tarwyn;

import java.util.concurrent.atomic.AtomicBoolean;

public final class TarwynClient implements AutoCloseable {
    private final long handle;
    private final AtomicBoolean closed = new AtomicBoolean(false);

    TarwynClient(long handle) {
        this.handle = handle;
    }

    /**
     * Connect to a server on localhost with the default ports.
     */
    public TarwynClient() {
        this(TarwynClient.__boltffiCreateHandle0());
    }

    private static long __boltffiCreateHandle0() {
        return Native.boltffi_init_class_tarwyn_bindings_x_tables_client_new();
    }

    /**
     * Connect to a server on another machine - a coprocessor, or the robot controller.
     */
    public TarwynClient(String host) {
        this(TarwynClient.__boltffiCreateHandle1(host));
    }

    private static long __boltffiCreateHandle1(String host) {
        WireLease __boltffi_host_wire = WireWriterPool.acquire(WireSizes.string(host));
        try {
    WireWriter __boltffi_host_writer = __boltffi_host_wire.writer();
    __boltffi_host_writer.writeString(host);
    return Native.boltffi_init_class_tarwyn_bindings_x_tables_client_connect(__boltffi_host_wire.directBuffer(), __boltffi_host_wire.size());
} finally {
    __boltffi_host_wire.close();
}
    }

    /**
     * Connect with every port and the request timeout spelled out.
     */
    public TarwynClient(String host, short pushPort, short reqPort, short subPort, short telemetryPort, long requestTimeoutMs, int sendHighWaterMark) {
        this(TarwynClient.__boltffiCreateHandle2(host, pushPort, reqPort, subPort, telemetryPort, requestTimeoutMs, sendHighWaterMark));
    }

    private static long __boltffiCreateHandle2(String host, short pushPort, short reqPort, short subPort, short telemetryPort, long requestTimeoutMs, int sendHighWaterMark) {
        WireLease __boltffi_host_wire = WireWriterPool.acquire(WireSizes.string(host));
        try {
    WireWriter __boltffi_host_writer = __boltffi_host_wire.writer();
    __boltffi_host_writer.writeString(host);
    return Native.boltffi_init_class_tarwyn_bindings_x_tables_client_with_ports(__boltffi_host_wire.directBuffer(), __boltffi_host_wire.size(), pushPort, reqPort, subPort, telemetryPort, requestTimeoutMs, sendHighWaterMark);
} finally {
    __boltffi_host_wire.close();
}
    }

    long rawHandle() {
        if (closed.get()) throw new IllegalStateException("TarwynClient is closed");
        return handle;
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
        Native.boltffi_release_class_tarwyn_bindings_x_tables_client(this.handle);
    }

    /**
     * Start the receive threads, so subscriptions begin delivering.
     * 
     * Publishing and reading work without this.
     */
    public void start() {
        Native.boltffi_method_class_tarwyn_bindings_x_tables_client_start(this.rawHandle());
    }

    /**
     * Stop the receive threads. Subscriptions survive and resume on the next start.
     */
    public void stop() {
        Native.boltffi_method_class_tarwyn_bindings_x_tables_client_stop(this.rawHandle());
    }

    /**
     * Publish a string.
     */
    public void putString(String channel, String value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.string(value));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeString(value);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_string(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a 32-bit signed integer.
     */
    public void putInteger(String channel, int value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a 64-bit signed integer.
     */
    public void putLong(String channel, long value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_long(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a double.
     */
    public void putDouble(String channel, double value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_double(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a float.
     */
    public void putFloat(String channel, float value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_float(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a boolean.
     */
    public void putBoolean(String channel, boolean value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish raw bytes.
     */
    public void putBytes(String channel, byte[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire((4 + value.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeBytes(value);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a list of strings.
     */
    public void putStringList(String channel, java.util.List<String> value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.stringSequence(value));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeStringSequence(value);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_string_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a list of byte strings.
     */
    public void putBytesList(String channel, java.util.List<byte[]> value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.sequence(value, (__boltffi_value_0) -> (4 + __boltffi_value_0.length)));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeSequence(value, (__boltffi_value_0) -> { __boltffi_value_writer.writeBytes(__boltffi_value_0); });
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_bytes_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a list of doubles.
     */
    public void putDoubleList(String channel, double[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_double_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a list of floats.
     */
    public void putFloatList(String channel, float[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_float_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a list of 32-bit integers.
     */
    public void putIntegerList(String channel, int[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_integer_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a list of 64-bit integers.
     */
    public void putLongList(String channel, long[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_long_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a list of booleans.
     */
    public void putBooleanList(String channel, boolean[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_boolean_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a list of `(x, y)` coordinates.
     */
    public void putCoordinates(String channel, java.util.List<Coordinate> value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_coordinates(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), DirectVectorCodec.writeRecords(value, 16, (item, buffer, offset) -> { item.writeToDirectBuffer(buffer, offset); }));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a pose on the field plane.
     */
    public void putPose2d(String channel, Pose2d value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose2d(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value.toDirectBuffer());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish a pose in space.
     */
    public void putPose3d(String channel, Pose3d value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_pose3d(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), value.toDirectBuffer());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish one bezier curve.
     */
    public void putBezierCurve(String channel, java.util.List<Point> value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.sequence(value, (__boltffi_value_0) -> __boltffi_value_0.wireSize()));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeSequence(value, (__boltffi_value_0) -> { __boltffi_value_0.writeTo(__boltffi_value_writer); });
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curve(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a bezier path already encoded as protobuf, byte-identical to TARWYN'.
     */
    public boolean putBezierCurves(String channel, byte[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire((4 + value.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeBytes(value);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish several bezier paths, encoded as protobuf.
     */
    public boolean putBezierCurvesList(String channel, byte[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire((4 + value.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeBytes(value);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_bezier_curves_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish bytes whose type the caller does not know.
     */
    public void putUnknownBytes(String channel, byte[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire((4 + value.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeBytes(value);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_unknown_bytes(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Publish a value already encoded in TARWYN' byte layout, given its type tag.
     * 
     * Returns false, publishing nothing, when a recognised tag comes with bytes
     * that are not a valid value of that type.
     */
    public boolean putTypedBytes(String channel, int tarwynType, byte[] value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire((4 + value.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeBytes(value);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_put_typed_bytes(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), tarwynType, __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Read a string. Absent if the channel holds nothing, or another type.
     */
    public java.util.Optional<String> getString(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_string(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readString());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a 32-bit signed integer.
     */
    public java.util.Optional<Integer> getInteger(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readInt());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a 64-bit signed integer.
     */
    public java.util.Optional<Long> getLong(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_long(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readLong());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a double.
     */
    public java.util.Optional<Double> getDouble(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_double(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readDouble());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a float.
     */
    public java.util.Optional<Float> getFloat(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_float(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readFloat());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a boolean.
     */
    public java.util.Optional<Boolean> getBoolean(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBoolean());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read raw bytes.
     */
    public java.util.Optional<byte[]> getBytes(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBytes());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of strings.
     */
    public java.util.Optional<java.util.List<String>> getStringList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_string_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readStringSequence());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of byte strings.
     */
    public java.util.Optional<java.util.List<byte[]>> getBytesList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_bytes_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readSequence(() -> __boltffi_reader.readBytes()));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of doubles.
     */
    public java.util.Optional<double[]> getDoubleList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_double_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readDoubleArray());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of floats.
     */
    public java.util.Optional<float[]> getFloatList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_float_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readFloatArray());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of 32-bit integers.
     */
    public java.util.Optional<int[]> getIntegerList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_integer_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readIntArray());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of 64-bit integers.
     */
    public java.util.Optional<long[]> getLongList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_long_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readLongArray());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of booleans.
     */
    public java.util.Optional<boolean[]> getBooleanList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_boolean_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBooleanArray());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a coordinate list.
     */
    public java.util.Optional<java.util.List<Coordinate>> getCoordinates(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_coordinates(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readSequence(() -> Coordinate.fromReader(__boltffi_reader)));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a pose on the field plane.
     */
    public java.util.Optional<Pose2d> getPose2d(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose2d(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> Pose2d.fromReader(__boltffi_reader));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a pose in space.
     */
    public java.util.Optional<Pose3d> getPose3d(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_pose3d(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> Pose3d.fromReader(__boltffi_reader));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read one bezier curve as its control points.
     */
    public java.util.Optional<java.util.List<Point>> getBezierCurve(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curve(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readSequence(() -> Point.fromReader(__boltffi_reader)));
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a bezier path as encoded protobuf, byte-identical to TARWYN'.
     */
    public java.util.Optional<byte[]> getBezierCurves(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBytes());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a list of bezier paths as encoded protobuf.
     */
    public java.util.Optional<byte[]> getBezierCurvesList(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_bezier_curves_list(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBytes());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Read a channel holding raw bytes whose type the caller does not know.
     */
    public java.util.Optional<byte[]> getUnknownBytes(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_unknown_bytes(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readBytes());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Delete a channel. Returns how many were removed, 0 or 1.
     */
    public int delete(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_delete(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Delete every channel. Returns how many were removed.
     */
    public int deleteAll() {
        return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_delete_all(this.rawHandle());
    }

    /**
     * List the channel names beginning with `prefix`. Pass "" for all of them.
     */
    public java.util.List<String> getTables(String prefix) {
        WireLease __boltffi_prefix_wire = WireWriterPool.acquire(WireSizes.string(prefix));
        try {
    WireWriter __boltffi_prefix_writer = __boltffi_prefix_wire.writer();
    __boltffi_prefix_writer.writeString(prefix);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_tables(this.rawHandle(), __boltffi_prefix_wire.directBuffer(), __boltffi_prefix_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readStringSequence();
} finally {
    __boltffi_prefix_wire.close();
}
    }

    /**
     * Round-trip time to the server in nanoseconds, absent if it does not answer.
     */
    public java.util.Optional<Long> getPing() {
        byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_ping(this.rawHandle());
        WireReader __boltffi_reader = new WireReader(__boltffi_result);
        return __boltffi_reader.readOptional(() -> __boltffi_reader.readLong());
    }

    /**
     * Server counters. Absent if the server does not answer.
     */
    public java.util.Optional<ServerStatistics> getServerStatistics() {
        byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_server_statistics(this.rawHandle());
        WireReader __boltffi_reader = new WireReader(__boltffi_result);
        return __boltffi_reader.readOptional(() -> ServerStatistics.fromReader(__boltffi_reader));
    }

    /**
     * The channels beginning with `prefix`, as a JSON document.
     */
    public String getRawJson(String prefix) {
        WireLease __boltffi_prefix_wire = WireWriterPool.acquire(WireSizes.string(prefix));
        try {
    WireWriter __boltffi_prefix_writer = __boltffi_prefix_wire.writer();
    __boltffi_prefix_writer.writeString(prefix);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_get_raw_json(this.rawHandle(), __boltffi_prefix_wire.directBuffer(), __boltffi_prefix_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readString();
} finally {
    __boltffi_prefix_wire.close();
}
    }

    /**
     * Set a channel to `value` only while it is empty, and report whether it swapped.
     */
    public boolean compareAndSetAbsentString(String channel, String value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.string(value));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeString(value);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_absent_string(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Set a channel to `value` only if it currently holds `expected`.
     */
    public boolean compareAndSetString(String channel, String expected, String value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_expected_wire = WireWriterPool.acquire(WireSizes.string(expected));
        WireLease __boltffi_value_wire = WireWriterPool.acquire(WireSizes.string(value));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_expected_writer = __boltffi_expected_wire.writer();
    __boltffi_expected_writer.writeString(expected);
    WireWriter __boltffi_value_writer = __boltffi_value_wire.writer();
    __boltffi_value_writer.writeString(value);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_string(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_expected_wire.directBuffer(), __boltffi_expected_wire.size(), __boltffi_value_wire.directBuffer(), __boltffi_value_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_expected_wire.close();
    __boltffi_value_wire.close();
}
    }

    /**
     * Set a channel to `value` only if it currently holds `expected`.
     */
    public boolean compareAndSetDouble(String channel, double expected, double value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_double(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), expected, value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Set a channel to `value` only if it currently holds `expected`.
     */
    public boolean compareAndSetLong(String channel, long expected, long value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_long(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), expected, value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Set a channel to `value` only if it currently holds `expected`.
     */
    public boolean compareAndSetBoolean(String channel, boolean expected, boolean value) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_compare_and_set_boolean(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), expected, value);
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Publish on the UDP telemetry plane, which trades delivery guarantees for latency.
     */
    public void publishTelemetry(String channel, byte[] payload) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        WireLease __boltffi_payload_wire = WireWriterPool.acquire((4 + payload.length));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    WireWriter __boltffi_payload_writer = __boltffi_payload_wire.writer();
    __boltffi_payload_writer.writeBytes(payload);
    Native.boltffi_method_class_tarwyn_bindings_x_tables_client_publish_telemetry(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size(), __boltffi_payload_wire.directBuffer(), __boltffi_payload_wire.size());
} finally {
    __boltffi_channel_wire.close();
    __boltffi_payload_wire.close();
}
    }

    /**
     * Mirror every published value into a WPILOG file.
     */
    public boolean logTo(String path) {
        WireLease __boltffi_path_wire = WireWriterPool.acquire(WireSizes.string(path));
        try {
    WireWriter __boltffi_path_writer = __boltffi_path_wire.writer();
    __boltffi_path_writer.writeString(path);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_log_to(this.rawHandle(), __boltffi_path_wire.directBuffer(), __boltffi_path_wire.size());
} finally {
    __boltffi_path_wire.close();
}
    }

    /**
     * As `log_to`, onto the first writable removable mount. Returns the path chosen.
     */
    public java.util.Optional<String> logToDrive(String filename) {
        WireLease __boltffi_filename_wire = WireWriterPool.acquire(WireSizes.string(filename));
        try {
    WireWriter __boltffi_filename_writer = __boltffi_filename_wire.writer();
    __boltffi_filename_writer.writeString(filename);
    byte[] __boltffi_result = Native.boltffi_method_class_tarwyn_bindings_x_tables_client_log_to_drive(this.rawHandle(), __boltffi_filename_wire.directBuffer(), __boltffi_filename_wire.size());
    WireReader __boltffi_reader = new WireReader(__boltffi_result);
    return __boltffi_reader.readOptional(() -> __boltffi_reader.readString());
} finally {
    __boltffi_filename_wire.close();
}
    }

    /**
     * How many log records were dropped because the writer queue was full.
     */
    public long droppedLogRecords() {
        return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_log_records(this.rawHandle());
    }

    /**
     * Whether the log writer is still succeeding.
     */
    public boolean loggingHealthy() {
        return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_logging_healthy(this.rawHandle());
    }

    /**
     * How many publishes were dropped rather than queued, across both transports.
     */
    public long droppedPublishes() {
        return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_dropped_publishes(this.rawHandle());
    }

    /**
     * Deliver every value published to `channel`.
     * 
     * Values arrive as soon as they are published: the consumer is woken rather
     * than polling, so delivery is not paced by an interval.
     */
    public boolean subscribe(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Receive telemetry on `channel`. Absent if another channel already claimed
     * this one's topic hash - a collision is refused rather than cross-wired.
     */
    public boolean subscribeTelemetry(String channel) {
        WireLease __boltffi_channel_wire = WireWriterPool.acquire(WireSizes.string(channel));
        try {
    WireWriter __boltffi_channel_writer = __boltffi_channel_wire.writer();
    __boltffi_channel_writer.writeString(channel);
    return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_telemetry(this.rawHandle(), __boltffi_channel_wire.directBuffer(), __boltffi_channel_wire.size());
} finally {
    __boltffi_channel_wire.close();
}
    }

    /**
     * Deliver every log line the server emits.
     */
    public boolean subscribeToLogs() {
        return Native.boltffi_method_class_tarwyn_bindings_x_tables_client_subscribe_to_logs(this.rawHandle());
    }

    /**
     * The stream every [`Self::subscribe`] call feeds.
     */
    public StreamSubscription<Update> updates(java.util.function.Consumer<Update> callback) {
        long subscription = Native.boltffi_stream_tarwyn_bindings_x_tables_client_updates_subscribe(this.rawHandle());
        return BoltFfiStream.callback(
            subscription,
            16L,
            (streamHandle, maxCount) -> {
                byte[] bytes = Native.boltffi_stream_tarwyn_bindings_x_tables_client_updates_pop_batch(streamHandle, maxCount);
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
                WireReader reader = new WireReader(bytes);
                return reader.readSequence(() -> Update.fromReader(reader));
            },
            (streamHandle, continuation) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_updates_poll(streamHandle, continuation),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_updates_unsubscribe(streamHandle),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_updates_free(streamHandle),
            callback
        );
    }


    /**
     * The stream every [`Self::subscribe_telemetry`] call feeds.
     */
    public StreamSubscription<Telemetry> telemetry(java.util.function.Consumer<Telemetry> callback) {
        long subscription = Native.boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_subscribe(this.rawHandle());
        return BoltFfiStream.callback(
            subscription,
            16L,
            (streamHandle, maxCount) -> {
                byte[] bytes = Native.boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_pop_batch(streamHandle, maxCount);
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
                WireReader reader = new WireReader(bytes);
                return reader.readSequence(() -> Telemetry.fromReader(reader));
            },
            (streamHandle, continuation) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_poll(streamHandle, continuation),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_unsubscribe(streamHandle),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_telemetry_free(streamHandle),
            callback
        );
    }


    /**
     * The stream [`Self::subscribe_to_logs`] feeds.
     */
    public StreamSubscription<String> logs(java.util.function.Consumer<String> callback) {
        long subscription = Native.boltffi_stream_tarwyn_bindings_x_tables_client_logs_subscribe(this.rawHandle());
        return BoltFfiStream.callback(
            subscription,
            16L,
            (streamHandle, maxCount) -> {
                byte[] bytes = Native.boltffi_stream_tarwyn_bindings_x_tables_client_logs_pop_batch(streamHandle, maxCount);
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
                WireReader reader = new WireReader(bytes);
                return reader.readSequence(() -> reader.readString());
            },
            (streamHandle, continuation) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_logs_poll(streamHandle, continuation),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_logs_unsubscribe(streamHandle),
            (streamHandle) -> Native.boltffi_stream_tarwyn_bindings_x_tables_client_logs_free(streamHandle),
            callback
        );
    }

}