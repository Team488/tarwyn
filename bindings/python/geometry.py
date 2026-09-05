"""Converts between the client's pose types and WPILib's geometry types."""

from wpimath.geometry import Pose2d, Pose3d, Quaternion, Rotation2d, Rotation3d

from . import Pose2d as _Pose2d, Pose3d as _Pose3d


def convert(pose):
    """The WPILib pose for our fields, or our fields for a WPILib pose."""
    if isinstance(pose, _Pose3d):
        return Pose3d(
            pose.x,
            pose.y,
            pose.z,
            Rotation3d(Quaternion(pose.qw, pose.qx, pose.qy, pose.qz)),
        )
    if isinstance(pose, _Pose2d):
        return Pose2d(pose.x, pose.y, Rotation2d(pose.rotation))
    if isinstance(pose, Pose3d):
        rotation = pose.rotation().getQuaternion()
        return _Pose3d(
            pose.X(),
            pose.Y(),
            pose.Z(),
            rotation.W(),
            rotation.X(),
            rotation.Y(),
            rotation.Z(),
        )
    return _Pose2d(pose.X(), pose.Y(), pose.rotation().radians())
