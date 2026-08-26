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

}
