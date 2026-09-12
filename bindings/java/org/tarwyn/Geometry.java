package org.tarwyn;

import org.wpilib.math.geometry.Quaternion;
import org.wpilib.math.geometry.Rotation2d;
import org.wpilib.math.geometry.Rotation3d;

/** Converts between the client's pose types and WPILib's geometry types. */
public final class Geometry {
    private Geometry() {
    }

    /** The WPILib pose these fields describe. */
    public static org.wpilib.math.geometry.Pose2d convert(Pose2d pose) {
        return new org.wpilib.math.geometry.Pose2d(
            pose.x(), pose.y(), new Rotation2d(pose.rotation()));
    }

    /** The fields this client publishes for a WPILib pose. */
    public static Pose2d convert(org.wpilib.math.geometry.Pose2d pose) {
        return new Pose2d(pose.getX(), pose.getY(), pose.getRotation().getRadians());
    }

    /** The WPILib pose in space these fields describe. */
    public static org.wpilib.math.geometry.Pose3d convert(Pose3d pose) {
        return new org.wpilib.math.geometry.Pose3d(
            pose.x(), pose.y(), pose.z(),
            new Rotation3d(new Quaternion(pose.qw(), pose.qx(), pose.qy(), pose.qz())));
    }

    /** The fields this client publishes for a WPILib pose in space. */
    public static Pose3d convert(org.wpilib.math.geometry.Pose3d pose) {
        Quaternion rotation = pose.getRotation().getQuaternion();
        return new Pose3d(
            pose.getX(), pose.getY(), pose.getZ(),
            rotation.getW(), rotation.getX(), rotation.getY(), rotation.getZ());
    }
}
