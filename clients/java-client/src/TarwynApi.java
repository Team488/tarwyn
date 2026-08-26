// Generated from clients/api.toml by codegen. Do not edit.

import static tarwyn.ffi.tarwyn_h.*;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public abstract class TarwynApi {
    protected Arena arena;
    protected MemorySegment handle;

    protected abstract void check(int code, String what);

    public void putString(String channel, String value) {
        check(xt_put_string(handle, arena.allocateFrom(channel), arena.allocateFrom(value)), "putString");
    }

    public void putInteger(String channel, int value) {
        check(xt_put_integer(handle, arena.allocateFrom(channel), value), "putInteger");
    }

    public void putLong(String channel, long value) {
        check(xt_put_long(handle, arena.allocateFrom(channel), value), "putLong");
    }

    public void putDouble(String channel, double value) {
        check(xt_put_double(handle, arena.allocateFrom(channel), value), "putDouble");
    }

    public void putFloat(String channel, float value) {
        check(xt_put_float(handle, arena.allocateFrom(channel), value), "putFloat");
    }

    public void putBoolean(String channel, boolean value) {
        check(xt_put_boolean(handle, arena.allocateFrom(channel), value), "putBoolean");
    }

    public String getString(String channel) {
        MemorySegment out = arena.allocate(4096);
        int code = xt_get_string(handle, arena.allocateFrom(channel), out, 4096);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getString");
        return out.getString(0);
    }

    public Integer getInteger(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_INT);
        int code = xt_get_integer(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getInteger");
        return out.get(ValueLayout.JAVA_INT, 0);
    }

    public Long getLong(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_LONG);
        int code = xt_get_long(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getLong");
        return out.get(ValueLayout.JAVA_LONG, 0);
    }

    public Double getDouble(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_DOUBLE);
        int code = xt_get_double(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getDouble");
        return out.get(ValueLayout.JAVA_DOUBLE, 0);
    }

    public Float getFloat(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_FLOAT);
        int code = xt_get_float(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getFloat");
        return out.get(ValueLayout.JAVA_FLOAT, 0);
    }

    public Boolean getBoolean(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_BOOLEAN);
        int code = xt_get_boolean(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getBoolean");
        return out.get(ValueLayout.JAVA_BOOLEAN, 0);
    }

    public void putPose2d(String channel, double x, double y, double rotation) {
        MemorySegment values = arena.allocate(ValueLayout.JAVA_DOUBLE, 3);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 0, x);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 1, y);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 2, rotation);
        check(xt_put_pose2d(handle, arena.allocateFrom(channel), values), "putPose2d");
    }

    public void putPose3d(String channel, double x, double y, double z, double roll, double pitch, double yaw) {
        MemorySegment values = arena.allocate(ValueLayout.JAVA_DOUBLE, 6);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 0, x);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 1, y);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 2, z);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 3, roll);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 4, pitch);
        values.setAtIndex(ValueLayout.JAVA_DOUBLE, 5, yaw);
        check(xt_put_pose3d(handle, arena.allocateFrom(channel), values), "putPose3d");
    }

    public void putPose2d(String channel, edu.wpi.first.math.geometry.Pose2d value) {
        putPose2d(channel, value.getX(), value.getY(), value.getRotation().getRadians());
    }

    public void putPose3d(String channel, edu.wpi.first.math.geometry.Pose3d value) {
        putPose3d(channel, value.getX(), value.getY(), value.getZ(), value.getRotation().getX(), value.getRotation().getY(), value.getRotation().getZ());
    }

    public edu.wpi.first.math.geometry.Pose2d getPose2d(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_DOUBLE, 3);
        int code = xt_get_pose2d(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getPose2d");
        double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
        return new edu.wpi.first.math.geometry.Pose2d(fields[0], fields[1], new edu.wpi.first.math.geometry.Rotation2d(fields[2]));
    }

    public edu.wpi.first.math.geometry.Pose3d getPose3d(String channel) {
        MemorySegment out = arena.allocate(ValueLayout.JAVA_DOUBLE, 6);
        int code = xt_get_pose3d(handle, arena.allocateFrom(channel), out);
        if (code == XT_ERR_NO_VALUE() || code == XT_ERR_WRONG_TYPE()) {
            return null;
        }
        check(code, "getPose3d");
        double[] fields = out.toArray(ValueLayout.JAVA_DOUBLE);
        return new edu.wpi.first.math.geometry.Pose3d(fields[0], fields[1], fields[2], new edu.wpi.first.math.geometry.Rotation3d(fields[3], fields[4], fields[5]));
    }

}
