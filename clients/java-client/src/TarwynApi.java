// Generated from clients/api.toml by codegen. Do not edit.

import static tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.concurrent.ConcurrentHashMap;

public abstract class TarwynApi {
    protected Arena arena;
    protected MemorySegment handle;

    private final ConcurrentHashMap<String, MemorySegment> channels = new ConcurrentHashMap<>();

    protected abstract void check(int code, String what);

    protected MemorySegment channel(String name) {
        return channels.computeIfAbsent(name, key -> arena.allocateFrom(key));
    }

    public void putString(String channel, String value) {
        try (Arena call = Arena.ofConfined()) {
            check(xt_put_string(handle, channel(channel), call.allocateFrom(value)), "putString");
        }
    }

    public void putInteger(String channel, int value) {
        check(xt_put_integer(handle, channel(channel), value), "putInteger");
    }

    public void putLong(String channel, long value) {
        check(xt_put_long(handle, channel(channel), value), "putLong");
    }

    public void putDouble(String channel, double value) {
        check(xt_put_double(handle, channel(channel), value), "putDouble");
    }

    public void putFloat(String channel, float value) {
        check(xt_put_float(handle, channel(channel), value), "putFloat");
    }

    public void putBoolean(String channel, boolean value) {
        check(xt_put_boolean(handle, channel(channel), value), "putBoolean");
    }

    public String getString(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(4096);
            int code = xt_get_string(handle, channel(channel), out, 4096);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getString");
            return out.getString(0);
        }
    }

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
            check(xt_put_string_list(handle, channel(channel), body, (long) total), "putStringList");
        }
    }

    public String[] getStringList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_string_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getStringList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_string_list(handle, channel(channel), out, needed, size), "getStringList");
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
            check(xt_put_bytes_list(handle, channel(channel), body, (long) total), "putBytesList");
        }
    }

    public byte[][] getBytesList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 4096;
            MemorySegment out = call.allocate(capacity);
            int code = xt_get_bytes_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBytesList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(needed);
                check(xt_get_bytes_list(handle, channel(channel), out, needed, size), "getBytesList");
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

    public void putDoubleList(String channel, double[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_DOUBLE, values);
            check(
                xt_put_double_list(handle, channel(channel), body, (long) values.length),
                "putDoubleList");
        }
    }

    public double[] getDoubleList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, capacity);
            int code = xt_get_double_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getDoubleList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_DOUBLE, needed);
                check(xt_get_double_list(handle, channel(channel), out, needed, size), "getDoubleList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_DOUBLE.byteSize()).toArray(ValueLayout.JAVA_DOUBLE);
        }
    }

    public void putFloatList(String channel, float[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_FLOAT, values);
            check(
                xt_put_float_list(handle, channel(channel), body, (long) values.length),
                "putFloatList");
        }
    }

    public float[] getFloatList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_FLOAT, capacity);
            int code = xt_get_float_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getFloatList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_FLOAT, needed);
                check(xt_get_float_list(handle, channel(channel), out, needed, size), "getFloatList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_FLOAT.byteSize()).toArray(ValueLayout.JAVA_FLOAT);
        }
    }

    public void putIntegerList(String channel, int[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_INT, values);
            check(
                xt_put_integer_list(handle, channel(channel), body, (long) values.length),
                "putIntegerList");
        }
    }

    public int[] getIntegerList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_INT, capacity);
            int code = xt_get_integer_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getIntegerList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_INT, needed);
                check(xt_get_integer_list(handle, channel(channel), out, needed, size), "getIntegerList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_INT.byteSize()).toArray(ValueLayout.JAVA_INT);
        }
    }

    public void putLongList(String channel, long[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocateFrom(ValueLayout.JAVA_LONG, values);
            check(
                xt_put_long_list(handle, channel(channel), body, (long) values.length),
                "putLongList");
        }
    }

    public long[] getLongList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_LONG, capacity);
            int code = xt_get_long_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getLongList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_LONG, needed);
                check(xt_get_long_list(handle, channel(channel), out, needed, size), "getLongList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            return out.asSlice(0, needed * ValueLayout.JAVA_LONG.byteSize()).toArray(ValueLayout.JAVA_LONG);
        }
    }

    public void putBooleanList(String channel, boolean[] values) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment body = call.allocate(ValueLayout.JAVA_BOOLEAN, values.length);
            for (int index = 0; index < values.length; index++) {
                body.setAtIndex(ValueLayout.JAVA_BOOLEAN, index, values[index]);
            }
            check(
                xt_put_boolean_list(handle, channel(channel), body, (long) values.length),
                "putBooleanList");
        }
    }

    public boolean[] getBooleanList(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment size = call.allocate(ValueLayout.JAVA_LONG);
            long capacity = 256;
            MemorySegment out = call.allocate(ValueLayout.JAVA_BOOLEAN, capacity);
            int code = xt_get_boolean_list(handle, channel(channel), out, capacity, size);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getBooleanList");
            long needed = size.get(ValueLayout.JAVA_LONG, 0);
            if (needed > capacity) {
                out = call.allocate(ValueLayout.JAVA_BOOLEAN, needed);
                check(xt_get_boolean_list(handle, channel(channel), out, needed, size), "getBooleanList");
                needed = size.get(ValueLayout.JAVA_LONG, 0);
            }
            boolean[] items = new boolean[(int) needed];
            for (int index = 0; index < items.length; index++) {
                items[index] = out.getAtIndex(ValueLayout.JAVA_BOOLEAN, index);
            }
            return items;
        }
    }

    public void putPose2d(String channel, double x, double y, double rotation) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment values = call.allocate(ValueLayout.JAVA_DOUBLE, 3);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 0, x);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 1, y);
            values.setAtIndex(ValueLayout.JAVA_DOUBLE, 2, rotation);
            check(xt_put_pose2d(handle, channel(channel), values), "putPose2d");
        }
    }

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

    public void putPose2d(String channel, edu.wpi.first.math.geometry.Pose2d value) {
        putPose2d(channel, value.getX(), value.getY(), value.getRotation().getRadians());
    }

    public void putPose3d(String channel, edu.wpi.first.math.geometry.Pose3d value) {
        putPose3d(channel, value.getX(), value.getY(), value.getZ(), value.getRotation().getX(), value.getRotation().getY(), value.getRotation().getZ());
    }

    public edu.wpi.first.math.geometry.Pose2d getPose2d(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, 3);
            int code = xt_get_pose2d(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getPose2d");
            double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
            return new edu.wpi.first.math.geometry.Pose2d(fields[0], fields[1], new edu.wpi.first.math.geometry.Rotation2d(fields[2]));
        }
    }

    public edu.wpi.first.math.geometry.Pose3d getPose3d(String channel) {
        try (Arena call = Arena.ofConfined()) {
            MemorySegment out = call.allocate(ValueLayout.JAVA_DOUBLE, 6);
            int code = xt_get_pose3d(handle, channel(channel), out);
            if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
                return null;
            }
            check(code, "getPose3d");
            double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
            return new edu.wpi.first.math.geometry.Pose3d(fields[0], fields[1], fields[2], new edu.wpi.first.math.geometry.Rotation3d(fields[3], fields[4], fields[5]));
        }
    }

}
