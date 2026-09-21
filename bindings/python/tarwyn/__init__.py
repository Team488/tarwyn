"""Python client for the tarwyn key/value server.

Poses, coordinates and bezier control points are taken and returned as the
``wpimath`` geometry types; everything else is a plain ``str``, ``bytes``,
number or list of them.
"""

from wpimath import Pose2d, Pose3d, Quaternion, Rotation2d, Rotation3d, Translation2d

from . import _tarwyn
from ._tarwyn import LOGS_CHANNEL, ServerStatistics
from .geometry import Point

__all__ = ["LOGS_CHANNEL", "Point", "ServerStatistics", "TarwynClient"]


class TarwynClient(_tarwyn.TarwynClient):
    """One connection: publishes, reads, control and subscriptions.

    ``TarwynClient()`` reaches a server on this machine; ``connect`` and
    ``with_ports`` reach one elsewhere. Reads return ``None`` when the server
    does not answer within the request timeout, and publishing without a
    server neither blocks nor raises.
    """

    @classmethod
    def connect(cls, host: str) -> "TarwynClient":
        """A client for the server on ``host``, an address rather than a URL."""
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
        predict_micros: int = 200,
    ) -> "TarwynClient":
        """A client with every port, timeout and window spelled out.

        ``busy_poll_micros`` is how long the reader spins on its socket before
        it blocks, so a subscribed value is delivered without a thread wakeup;
        0 blocks at once. ``predict_micros`` is how far around a predicted
        arrival the reader spins instead, once the stream has shown a period;
        0 turns prediction off, and the default matches ``connect``.
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
