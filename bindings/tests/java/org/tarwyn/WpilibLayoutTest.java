package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * Proves the pose layout against WPILib's own serialiser rather than against its
 * documentation.
 *
 * The bindings pack a pose as little-endian doubles in field order, which was
 * derived by reading WPILib's struct schemas. That is the kind of claim that
 * looks right until a robot reads a pose written by a coprocessor and gets a
 * plausible wrong answer, so it is checked here by packing the same pose with
 * WPILib and comparing bytes.
 */
final class WpilibLayoutTest {
    private static ByteBuffer little(int bytes) {
        return ByteBuffer.allocate(bytes).order(ByteOrder.LITTLE_ENDIAN);
    }

    @Test
    void a_pose2d_is_three_little_endian_doubles() {
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
    void a_pose3d_is_a_translation_then_a_w_first_quaternion() {
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

    /**
     * The layout the bindings write, built here by hand, has to be the layout
     * WPILib writes. If these ever diverge the bindings are wrong, not the test.
     */
    @Test
    void what_the_bindings_pack_is_what_wpilib_packs() {
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
