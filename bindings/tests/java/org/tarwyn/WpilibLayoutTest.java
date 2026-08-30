package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

final class WpilibLayoutTest {
    private static ByteBuffer little(int bytes) {
        return ByteBuffer.allocate(bytes).order(ByteOrder.LITTLE_ENDIAN);
    }

    @Test
    void wpilib_packs_a_pose2d_as_the_three_doubles_this_writes() {
        var pose = new org.wpilib.math.geometry.Pose2d(1.5, -2.0, new org.wpilib.math.geometry.Rotation2d(0.25));

        ByteBuffer packed = little(org.wpilib.math.geometry.Pose2d.struct.getSize());
        org.wpilib.math.geometry.Pose2d.struct.pack(packed, pose);

        assertEquals(3 * 8, packed.capacity(), "a Pose2d is three doubles");
        packed.rewind();
        assertEquals(1.5, packed.getDouble(), "x comes first");
        assertEquals(-2.0, packed.getDouble(), "then y");
        assertEquals(0.25, packed.getDouble(), "then the rotation, in radians");
    }

    @Test
    void wpilib_packs_a_pose3d_with_w_before_x_y_and_z() {
        var rotation = new org.wpilib.math.geometry.Rotation3d(
            new org.wpilib.math.geometry.Quaternion(0.5, 0.5, 0.5, 0.5));
        var pose = new org.wpilib.math.geometry.Pose3d(1.0, 2.0, 3.0, rotation);

        ByteBuffer packed = little(org.wpilib.math.geometry.Pose3d.struct.getSize());
        org.wpilib.math.geometry.Pose3d.struct.pack(packed, pose);

        assertEquals(7 * 8, packed.capacity(), "three for the translation, four for the quaternion");
        packed.rewind();
        assertEquals(1.0, packed.getDouble(), "x");
        assertEquals(2.0, packed.getDouble(), "y");
        assertEquals(3.0, packed.getDouble(), "z");
        assertEquals(0.5, packed.getDouble(), "w precedes x, y and z");
    }
    @Test
    void a_pose3d_packed_here_is_byte_identical_to_wpilibs() {
        var quaternion = new org.wpilib.math.geometry.Quaternion(0.5, -0.5, 0.5, -0.5);
        var pose = new org.wpilib.math.geometry.Pose3d(
            1.25, -6.5, 0.75, new org.wpilib.math.geometry.Rotation3d(quaternion));

        ByteBuffer wpilib = little(org.wpilib.math.geometry.Pose3d.struct.getSize());
        org.wpilib.math.geometry.Pose3d.struct.pack(wpilib, pose);

        Pose3d ours = new Pose3d(
            pose.getX(), pose.getY(), pose.getZ(),
            quaternion.getW(), quaternion.getX(), quaternion.getY(), quaternion.getZ());
        ByteBuffer bindings = little(7 * 8);
        bindings.putDouble(ours.x()).putDouble(ours.y()).putDouble(ours.z())
            .putDouble(ours.qw()).putDouble(ours.qx()).putDouble(ours.qy()).putDouble(ours.qz());

        assertEquals(wpilib.rewind(), bindings.rewind(), "the two layouts must be byte-identical");
    }
}
