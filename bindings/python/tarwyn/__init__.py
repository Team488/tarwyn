"""Python client for the tarwyn key/value server.

Poses, coordinates and bezier control points are taken and returned as the
``wpimath`` geometry types; everything else is a plain ``str``, ``bytes``,
number or list of them.
"""

from wpimath import Pose2d, Pose3d, Quaternion, Rotation2d, Rotation3d, Translation2d

from . import _tarwyn
from ._tarwyn import DEFAULT_PREDICT_MICROS, LOGS_CHANNEL, ServerStatistics
from .geometry import Point

__all__ = ["DEFAULT_PREDICT_MICROS", "LOGS_CHANNEL", "Point", "ServerStatistics", "TarwynClient"]


class TarwynClient(_tarwyn.TarwynClient):
    """One connection for publishes, reads, control and subscriptions.

    Reads return ``None`` when the server does not answer in time, and
    publishing without a server never blocks. Creating a client raises
    ``OSError`` only when the host does not resolve or no socket can be bound.
    """

    @classmethod
    def connect(cls, host: str) -> "TarwynClient":
        """A client for the server on ``host``, an address, not a URL."""
        return cls(host)

    @classmethod
    def with_ports(
        cls,
        host: str,
        port: int,
        telemetry_port: int,
        request_timeout_ms: int,
        send_high_water_mark: int,
        busy_poll_micros: int = 0,
        predict_micros: int = DEFAULT_PREDICT_MICROS,
    ) -> "TarwynClient":
        """A client with every port, timeout and window spelled out.

        ``busy_poll_micros`` is how long the reader spins before each blocking
        read, and 0 blocks right away. ``predict_micros`` is how long it spins
        around a predicted arrival, and 0 turns prediction off.
        """
        return cls(
            host,
            port,
            telemetry_port,
            request_timeout_ms,
            send_high_water_mark,
            busy_poll_micros,
            predict_micros,
        )

    def put_pose2d(self, channel: str, pose: Pose2d) -> None:
        super().put_pose2d(channel, pose.x, pose.y, pose.rotation().radians())

    def get_pose2d(self, channel: str) -> Pose2d | None:
        fields = super().get_pose2d(channel)
        if fields is None:
            return None
        x, y, rotation = fields
        return Pose2d(x, y, Rotation2d(rotation))

    def put_pose3d(self, channel: str, pose: Pose3d) -> None:
        q = pose.rotation().get_quaternion()
        super().put_pose3d(channel, pose.x, pose.y, pose.z, q.w, q.x, q.y, q.z)

    def get_pose3d(self, channel: str) -> Pose3d | None:
        fields = super().get_pose3d(channel)
        if fields is None:
            return None
        x, y, z, qw, qx, qy, qz = fields
        return Pose3d(x, y, z, Rotation3d(Quaternion(qw, qx, qy, qz)))

    def put_coordinates(self, channel: str, points: list[Translation2d]) -> None:
        super().put_coordinates(channel, [(point.x, point.y) for point in points])

    def get_coordinates(self, channel: str) -> list[Translation2d] | None:
        pairs = super().get_coordinates(channel)
        return None if pairs is None else [Translation2d(x, y) for x, y in pairs]

    def put_bezier_curve(self, channel: str, points: list[Point]) -> None:
        super().put_bezier_curve(
            channel, [(point.x, point.y, point.rotation_degrees) for point in points]
        )

    def get_bezier_curve(self, channel: str) -> list[Point] | None:
        triples = super().get_bezier_curve(channel)
        return None if triples is None else [Point(x, y, rotation) for x, y, rotation in triples]
