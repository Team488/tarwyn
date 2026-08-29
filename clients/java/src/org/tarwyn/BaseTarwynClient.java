// Generated from clients/api.toml by codegen. Do not edit.

package org.tarwyn;

import static org.tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The generated half of the Java client: every {@code put}, {@code get} and
 * {@code compareAndSet} the API spec defines.
 *
 * Generated from {@code clients/api.toml} alongside the C ABI and the Python
 * methods, so the three clients cannot drift apart when a type is added.
 * {@code TarwynClient} extends this and supplies the rest.
 */
public abstract class BaseTarwynClient {
    /** Backs the client for its whole lifetime; holds the cached channel names. */
    protected Arena arena;
    /** The native client, from {@code xt_client_new}. */
    protected MemorySegment handle;

    private final ConcurrentHashMap<String, MemorySegment> channels = new ConcurrentHashMap<>();

    /** For subclasses only. */
    protected BaseTarwynClient() {}

    /**
     * Turn a non-zero status from the native library into an exception.
     *
     * @param code the status returned by the call
     * @param what the operation that returned it, for the message
     */
    protected abstract void check(int code, String what);

    /**
     * The native string for a channel name, allocated once and reused.
     *
     * Every call would otherwise allocate into {@link #arena}, which reclaims
     * nothing until the client closes.
     *
     * @param name the channel name
     * @return the NUL-terminated native string
     */
    protected MemorySegment channel(String name) {
        return channels.computeIfAbsent(name, key -> arena.allocateFrom(key));
    }

    /**
     * Publish a string to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putString(String channel, String value) {
        try (Arena call = Arena.ofConfined()) {
            check(xt_put_string(handle, channel(channel), call.allocateFrom(value)), "putString");
        }
    }

    /**
     * Publish an integer to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putInteger(String channel, int value) {
        check(xt_put_integer(handle, channel(channel), value), "putInteger");
    }

    /**
     * Publish a long to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putLong(String channel, long value) {
        check(xt_put_long(handle, channel(channel), value), "putLong");
    }

    /**
     * Publish a double to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putDouble(String channel, double value) {
        check(xt_put_double(handle, channel(channel), value), "putDouble");
    }

    /**
     * Publish a float to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putFloat(String channel, float value) {
        check(xt_put_float(handle, channel(channel), value), "putFloat");
    }

    /**
     * Publish a boolean to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putBoolean(String channel, boolean value) {
        check(xt_put_boolean(handle, channel(channel), value), "putBoolean");
    }

    /**
     * Read a string from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public String getString(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int code = xt_get_string(handle, channel(channel), MemorySegment.NULL, 0, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getString");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            MemorySegment out = call.allocate(needed);
            check(xt_get_string(handle, channel(channel), out, (int) needed, size), "getString");
            return out.getString(0);
        }
    }

    /**
     * Read an integer from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public Integer getInteger(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_INT);
            int code = xt_get_integer(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getInteger");
            return out.get(ValueLayout.JAVA_INT, 0);
        }
    }

    /**
     * Read a long from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public Long getLong(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_LONG);
            int code = xt_get_long(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getLong");
            return out.get(ValueLayout.JAVA_LONG, 0);
        }
    }

    /**
     * Read a double from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public Double getDouble(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE);
            int code = xt_get_double(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getDouble");
            return out.get(ValueLayout.JAVA_DOUBLE, 0);
        }
    }

    /**
     * Read a float from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public Float getFloat(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_FLOAT);
            int code = xt_get_float(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getFloat");
            return out.get(ValueLayout.JAVA_FLOAT, 0);
        }
    }

    /**
     * Read a boolean from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public Boolean getBoolean(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            int code = xt_get_boolean(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBoolean");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes a string.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetString(String channel, String expected, String value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            MemorySegment previous = expected == null
                ? MemorySegment.NULL
                : call.allocateFrom(expected);
            check(
                xt_compare_and_set_string(handle, channel(channel), previous, expected != null,
                    call.allocateFrom(value), out),
                "compareAndSetString");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes an integer.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetInteger(String channel, Integer expected, int value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_integer(handle, channel(channel),
                    expected == null ? 0 : expected, expected != null, value, out),
                "compareAndSetInteger");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes a long.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetLong(String channel, Long expected, long value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_long(handle, channel(channel),
                    expected == null ? 0 : expected, expected != null, value, out),
                "compareAndSetLong");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes a double.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetDouble(String channel, Double expected, double value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_double(handle, channel(channel),
                    expected == null ? 0 : expected, expected != null, value, out),
                "compareAndSetDouble");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes a float.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetFloat(String channel, Float expected, float value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_float(handle, channel(channel),
                    expected == null ? 0 : expected, expected != null, value, out),
                "compareAndSetFloat");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Set {@code channel} to {@code value} only if it currently holds {@code expected}, and report whether it swapped. Takes a boolean.
     *
     * @param channel the channel to swap
     * @param expected the value the channel must currently hold
     * @param value the value
     * @return whether the swap happened
     */
    public boolean compareAndSetBoolean(String channel, Boolean expected, boolean value) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN);
            check(
                xt_compare_and_set_boolean(handle, channel(channel),
                    expected == null ? false : expected, expected != null, value, out),
                "compareAndSetBoolean");
            return out.get(ValueLayout.JAVA_BOOLEAN, 0);
        }
    }

    /**
     * Publish a list of strings to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putStringList(String channel, String[] values) {
        int total = 4;
        byte[][] encoded = new byte[values.length][];
        for (int index = 0; index < values.length; index++) {
            byte[] item = values[index].getBytes(java.nio.charset.StandardCharsets.UTF_8);
            encoded[index] = item;
            total += 4 + item.length;
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer.allocate(total)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
        buffer.putInt(values.length);
        for (byte[] item : encoded) {
            buffer.putInt(item.length);
            buffer.put(item);
        }
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, buffer.array());
            check(xt_put_string_list(handle, channel(channel), body, total), "putStringList");
        }
    }

    /**
     * Read a list of strings from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public String[] getStringList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_string_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getStringList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_string_list(handle, channel(channel), out, (int) needed, size), "getStringList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            java.nio.ByteBuffer buffer = out.asSlice(0, needed).asByteBuffer()
                .order(java.nio.ByteOrder.LITTLE_ENDIAN);
            String[] items = new String[buffer.getInt()];
            for (int index = 0; index < items.length; index++) {
                byte[] item = new byte[buffer.getInt()];
                buffer.get(item);
                items[index] = new String(item, java.nio.charset.StandardCharsets.UTF_8);
            }
            return items;
        }
    }

    /**
     * Publish a list of byte arrays to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putBytesList(String channel, byte[][] values) {
        int total = 4;
        byte[][] encoded = new byte[values.length][];
        for (int index = 0; index < values.length; index++) {
            byte[] item = values[index];
            encoded[index] = item;
            total += 4 + item.length;
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer.allocate(total)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
        buffer.putInt(values.length);
        for (byte[] item : encoded) {
            buffer.putInt(item.length);
            buffer.put(item);
        }
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_BYTE, buffer.array());
            check(xt_put_bytes_list(handle, channel(channel), body, total), "putBytesList");
        }
    }

    /**
     * Read a list of byte arrays from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public byte[][] getBytesList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_bytes_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBytesList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_bytes_list(handle, channel(channel), out, (int) needed, size), "getBytesList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            java.nio.ByteBuffer buffer = out.asSlice(0, needed).asByteBuffer()
                .order(java.nio.ByteOrder.LITTLE_ENDIAN);
            byte[][] items = new byte[buffer.getInt()][];
            for (int index = 0; index < items.length; index++) {
                byte[] item = new byte[buffer.getInt()];
                buffer.get(item);
                items[index] = item;
            }
            return items;
        }
    }

    /**
     * Publish a list of doubles to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putDoubleList(String channel, double[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_DOUBLE, values);
            check(
                xt_put_double_list(handle, channel(channel), body, values.length),
                "putDoubleList");
        }
    }

    /**
     * Read a list of doubles from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public double[] getDoubleList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, capacity);
            int code = xt_get_double_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getDoubleList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_DOUBLE, needed);
                check(xt_get_double_list(handle, channel(channel), out, (int) needed, size), "getDoubleList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_DOUBLE.byteSize()).toArray(ValueLayout.JAVA_DOUBLE);
        }
    }

    /**
     * Publish a list of floats to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putFloatList(String channel, float[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_FLOAT, values);
            check(
                xt_put_float_list(handle, channel(channel), body, values.length),
                "putFloatList");
        }
    }

    /**
     * Read a list of floats from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public float[] getFloatList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_FLOAT, capacity);
            int code = xt_get_float_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getFloatList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_FLOAT, needed);
                check(xt_get_float_list(handle, channel(channel), out, (int) needed, size), "getFloatList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_FLOAT.byteSize()).toArray(ValueLayout.JAVA_FLOAT);
        }
    }

    /**
     * Publish a list of integers to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putIntegerList(String channel, int[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_INT, values);
            check(
                xt_put_integer_list(handle, channel(channel), body, values.length),
                "putIntegerList");
        }
    }

    /**
     * Read a list of integers from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public int[] getIntegerList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_INT, capacity);
            int code = xt_get_integer_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getIntegerList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_INT, needed);
                check(xt_get_integer_list(handle, channel(channel), out, (int) needed, size), "getIntegerList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_INT.byteSize()).toArray(ValueLayout.JAVA_INT);
        }
    }

    /**
     * Publish a list of longs to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putLongList(String channel, long[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_LONG, values);
            check(
                xt_put_long_list(handle, channel(channel), body, values.length),
                "putLongList");
        }
    }

    /**
     * Read a list of longs from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public long[] getLongList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_LONG, capacity);
            int code = xt_get_long_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getLongList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_LONG, needed);
                check(xt_get_long_list(handle, channel(channel), out, (int) needed, size), "getLongList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_LONG.byteSize()).toArray(ValueLayout.JAVA_LONG);
        }
    }

    /**
     * Publish a list of booleans to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param values the value
     */
    public void putBooleanList(String channel, boolean[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocate(ValueLayout.JAVA_BOOLEAN, values.length);
            for (int index = 0; index < values.length; index++) {
                body.setAtIndex(ValueLayout.JAVA_BOOLEAN, index, values[index]);
            }
            check(
                xt_put_boolean_list(handle, channel(channel), body, values.length),
                "putBooleanList");
        }
    }

    /**
     * Read a list of booleans from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public boolean[] getBooleanList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            int capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN, capacity);
            int code = xt_get_boolean_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBooleanList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_BOOLEAN, needed);
                check(xt_get_boolean_list(handle, channel(channel), out, (int) needed, size), "getBooleanList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            boolean[] items = new boolean[(int) needed];
            for (int index = 0; index < items.length; index++) {
                items[index] = out.getAtIndex(ValueLayout.JAVA_BOOLEAN, index);
            }
            return items;
        }
    }

    /**
     * Publish a Pose2d to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param x the value
     * @param y the value
     * @param rotation the value
     */
    public void putPose2d(String channel, double x, double y, double rotation) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment values = call.allocate(ValueLayout.JAVA_DOUBLE, 3);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 0, x);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 1, y);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 2, rotation);
            check(xt_put_pose2d(handle, channel(channel), values), "putPose2d");
        }
    }

    /**
     * Publish a Pose3d to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param x the value
     * @param y the value
     * @param z the value
     * @param roll the value
     * @param pitch the value
     * @param yaw the value
     */
    public void putPose3d(String channel, double x, double y, double z, double roll, double pitch, double yaw) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment values = call.allocate(ValueLayout.JAVA_DOUBLE, 6);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 0, x);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 1, y);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 2, z);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 3, roll);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 4, pitch);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 5, yaw);
            check(xt_put_pose3d(handle, channel(channel), values), "putPose3d");
        }
    }

    /**
     * Publish a Pose2d to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putPose2d(String channel, org.wpilib.math.geometry.Pose2d value) {
        putPose2d(channel, value.getX(), value.getY(), value.getRotation().getRadians());
    }

    /**
     * Publish a Pose3d to {@code channel}.
     *
     * @param channel the channel to publish to
     * @param value the value
     */
    public void putPose3d(String channel, org.wpilib.math.geometry.Pose3d value) {
        putPose3d(channel, value.getX(), value.getY(), value.getZ(), value.getRotation().getX(), value.getRotation().getY(), value.getRotation().getZ());
    }

    /**
     * Read a Pose2d from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public org.wpilib.math.geometry.Pose2d getPose2d(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, 3);
            int code = xt_get_pose2d(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getPose2d");
            double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
            return new org.wpilib.math.geometry.Pose2d(fields[0], fields[1], new org.wpilib.math.geometry.Rotation2d(fields[2]));
        }
    }

    /**
     * Read a Pose3d from {@code channel}.
     *
     * @param channel the channel to read
     * @return the value, or null when the channel is unset
     */
    public org.wpilib.math.geometry.Pose3d getPose3d(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, 6);
            int code = xt_get_pose3d(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getPose3d");
            double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
            return new org.wpilib.math.geometry.Pose3d(fields[0], fields[1], fields[2], new org.wpilib.math.geometry.Rotation3d(fields[3], fields[4], fields[5]));
        }
    }

}
