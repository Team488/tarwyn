from __future__ import annotations

import uuid


from dataclasses import dataclass




from collections.abc import Sequence



MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None

@dataclass(frozen=True, slots=True)
class ServerStatistics:
    """Server counters, as reported by [`TarwynClient::get_server_statistics`]."""
    channels: int
    values: int
    telemetry_subscribers: int
    uptime_seconds: int
    dropped_publishes: int
    dropped_logs: int
    version: str



@dataclass(frozen=True, slots=True)
class Coordinate:
    """An `(x, y)` pair, as carried by the coordinate list type."""
    x: float
    y: float



@dataclass(frozen=True, slots=True)
class Point:
    """One control point of a bezier curve. `rotation_degrees` is absent for a point
    that does not constrain heading.
    """
    x: float
    y: float
    rotation_degrees: float | None



@dataclass(frozen=True, slots=True)
class Pose2d:
    """A pose on the field plane."""
    x: float
    y: float
    rotation: float



@dataclass(frozen=True, slots=True)
class Pose3d:
    """A pose in space, with its rotation as a quaternion.

    The field order is WPILib's `Pose3d` struct layout - a `Translation3d`
    followed by a `Rotation3d`, which is a `Quaternion` written `w` first - so a
    value written here reads back through WPILib's own deserialiser.

    Rotation is a quaternion rather than roll, pitch and yaw because converting
    between the two means committing to a rotation order, and getting that wrong
    is silent. `Rotation3d` converts in both directions: construct one from
    `roll`, `pitch`, `yaw` and read `getQuaternion()`, or take `getX()`, `getY()`
    and `getZ()` back out.
    """
    x: float
    y: float
    z: float
    qw: float
    qx: float
    qy: float
    qz: float



@dataclass(frozen=True, slots=True)
class Update:
    """A value published to a channel, delivered to a subscriber.

    The payload is the encoded value; `channel` names what it arrived on, so one
    subscription can carry several channels.
    """
    channel: str
    value: bytes



@dataclass(frozen=True, slots=True)
class Telemetry:
    """A telemetry datagram, with the publisher's clock."""
    timestamp_micros: int
    payload: bytes





class TarwynClient:
    _handle: int


    def __init__(self) -> None:
        """Connect to a server on localhost with the default ports."""


    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClient": ...
    def __del__(self) -> None: ...
    @classmethod
    def connect(cls, host: str) -> "TarwynClient":
        """Connect to a server on another machine - a coprocessor, or the robot controller."""
    @classmethod
    def with_ports(cls, host: str, push_port: int, req_port: int, sub_port: int, telemetry_port: int, request_timeout_ms: int, send_high_water_mark: int) -> "TarwynClient":
        """Connect with every port and the request timeout spelled out."""
    def start(self) -> None:
        """Start the receive threads, so subscriptions begin delivering.

        Publishing and reading work without this.
        """
    def stop(self) -> None:
        """Stop the receive threads. Subscriptions survive and resume on the next start."""
    def put_string(self, channel: str, value: str) -> None:
        """Publish a string."""
    def put_integer(self, channel: str, value: int) -> None:
        """Publish a 32-bit signed integer."""
    def put_long(self, channel: str, value: int) -> None:
        """Publish a 64-bit signed integer."""
    def put_double(self, channel: str, value: float) -> None:
        """Publish a double."""
    def put_float(self, channel: str, value: float) -> None:
        """Publish a float."""
    def put_boolean(self, channel: str, value: bool) -> None:
        """Publish a boolean."""
    def put_bytes(self, channel: str, value: bytes) -> None:
        """Publish raw bytes."""
    def put_string_list(self, channel: str, value: Sequence[str]) -> None:
        """Publish a list of strings."""
    def put_bytes_list(self, channel: str, value: Sequence[bytes]) -> None:
        """Publish a list of byte strings."""
    def put_double_list(self, channel: str, value: Sequence[float]) -> None:
        """Publish a list of doubles."""
    def put_float_list(self, channel: str, value: Sequence[float]) -> None:
        """Publish a list of floats."""
    def put_integer_list(self, channel: str, value: Sequence[int]) -> None:
        """Publish a list of 32-bit integers."""
    def put_long_list(self, channel: str, value: Sequence[int]) -> None:
        """Publish a list of 64-bit integers."""
    def put_boolean_list(self, channel: str, value: Sequence[bool]) -> None:
        """Publish a list of booleans."""
    def put_coordinates(self, channel: str, value: Sequence[Coordinate]) -> None:
        """Publish a list of `(x, y)` coordinates."""
    def put_pose2d(self, channel: str, value: Pose2d) -> None:
        """Publish a pose on the field plane."""
    def put_pose3d(self, channel: str, value: Pose3d) -> None:
        """Publish a pose in space."""
    def put_bezier_curve(self, channel: str, value: Sequence[Point]) -> None:
        """Publish one bezier curve."""
    def put_bezier_curves(self, channel: str, value: bytes) -> bool:
        """Publish a bezier path already encoded as protobuf, byte-identical to TARWYN'."""
    def put_bezier_curves_list(self, channel: str, value: bytes) -> bool:
        """Publish several bezier paths, encoded as protobuf."""
    def put_unknown_bytes(self, channel: str, value: bytes) -> None:
        """Publish bytes whose type the caller does not know."""
    def put_typed_bytes(self, channel: str, tarwyn_type: int, value: bytes) -> bool:
        """Publish a value already encoded in TARWYN' byte layout, given its type tag.

        Returns false, publishing nothing, when a recognised tag comes with bytes
        that are not a valid value of that type.
        """
    def get_string(self, channel: str) -> str | None:
        """Read a string. Absent if the channel holds nothing, or another type."""
    def get_integer(self, channel: str) -> int | None:
        """Read a 32-bit signed integer."""
    def get_long(self, channel: str) -> int | None:
        """Read a 64-bit signed integer."""
    def get_double(self, channel: str) -> float | None:
        """Read a double."""
    def get_float(self, channel: str) -> float | None:
        """Read a float."""
    def get_boolean(self, channel: str) -> bool | None:
        """Read a boolean."""
    def get_bytes(self, channel: str) -> bytes | None:
        """Read raw bytes."""
    def get_string_list(self, channel: str) -> list[str] | None:
        """Read a list of strings."""
    def get_bytes_list(self, channel: str) -> list[bytes] | None:
        """Read a list of byte strings."""
    def get_double_list(self, channel: str) -> list[float] | None:
        """Read a list of doubles."""
    def get_float_list(self, channel: str) -> list[float] | None:
        """Read a list of floats."""
    def get_integer_list(self, channel: str) -> list[int] | None:
        """Read a list of 32-bit integers."""
    def get_long_list(self, channel: str) -> list[int] | None:
        """Read a list of 64-bit integers."""
    def get_boolean_list(self, channel: str) -> list[bool] | None:
        """Read a list of booleans."""
    def get_coordinates(self, channel: str) -> list[Coordinate] | None:
        """Read a coordinate list."""
    def get_pose2d(self, channel: str) -> Pose2d | None:
        """Read a pose on the field plane."""
    def get_pose3d(self, channel: str) -> Pose3d | None:
        """Read a pose in space."""
    def get_bezier_curve(self, channel: str) -> list[Point] | None:
        """Read one bezier curve as its control points."""
    def get_bezier_curves(self, channel: str) -> bytes | None:
        """Read a bezier path as encoded protobuf, byte-identical to TARWYN'."""
    def get_bezier_curves_list(self, channel: str) -> bytes | None:
        """Read a list of bezier paths as encoded protobuf."""
    def get_unknown_bytes(self, channel: str) -> bytes | None:
        """Read a channel holding raw bytes whose type the caller does not know."""
    def delete(self, channel: str) -> int:
        """Delete a channel. Returns how many were removed, 0 or 1."""
    def delete_all(self) -> int:
        """Delete every channel. Returns how many were removed."""
    def get_tables(self, prefix: str) -> list[str]:
        """List the channel names beginning with `prefix`. Pass \"" for all of them."""
    def get_ping(self) -> int | None:
        """Round-trip time to the server in nanoseconds, absent if it does not answer."""
    def get_server_statistics(self) -> ServerStatistics | None:
        """Server counters. Absent if the server does not answer."""
    def get_raw_json(self, prefix: str) -> str:
        """The channels beginning with `prefix`, as a JSON document."""
    def compare_and_set_absent_string(self, channel: str, value: str) -> bool:
        """Set a channel to `value` only while it is empty, and report whether it swapped."""
    def compare_and_set_string(self, channel: str, expected: str, value: str) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
    def compare_and_set_double(self, channel: str, expected: float, value: float) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
    def compare_and_set_long(self, channel: str, expected: int, value: int) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
    def compare_and_set_boolean(self, channel: str, expected: bool, value: bool) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
    def publish_telemetry(self, channel: str, payload: bytes) -> None:
        """Publish on the UDP telemetry plane, which trades delivery guarantees for latency."""
    def log_to(self, path: str) -> bool:
        """Mirror every published value into a WPILOG file."""
    def log_to_drive(self, filename: str) -> str | None:
        """As `log_to`, onto the first writable removable mount. Returns the path chosen."""
    def dropped_log_records(self) -> int:
        """How many log records were dropped because the writer queue was full."""
    def logging_healthy(self) -> bool:
        """Whether the log writer is still succeeding."""
    def dropped_publishes(self) -> int:
        """How many publishes were dropped rather than queued, across both transports."""
    def subscribe(self, channel: str) -> bool:
        """Deliver every value published to `channel`.

        Values arrive as soon as they are published: the consumer is woken rather
        than polling, so delivery is not paced by an interval.
        """
    def unsubscribe(self, channel: str) -> bool:
        """Stop delivering values from `channel`. False if it was not subscribed."""
    def subscribe_telemetry(self, channel: str) -> bool:
        """Receive telemetry on `channel`. Absent if another channel already claimed
        this one's topic hash - a collision is refused rather than cross-wired.
        """
    def unsubscribe_telemetry(self, channel: str) -> bool:
        """Stop delivering telemetry from `channel`. False if it was not subscribed."""
    def subscribe_to_logs(self) -> bool:
        """Deliver every log line the server emits."""
    def unsubscribe_from_logs(self) -> bool:
        """Stop delivering log lines. False if they were not subscribed."""
    def updates(self) -> "TarwynClientUpdatesSubscription":
        """The stream every [`Self::subscribe`] call feeds."""
    def telemetry(self) -> "TarwynClientTelemetrySubscription":
        """The stream every [`Self::subscribe_telemetry`] call feeds."""
    def logs(self) -> "TarwynClientLogsSubscription":
        """The stream [`Self::subscribe_to_logs`] feeds."""


class TarwynClientUpdatesSubscription:
    _handle: int | None
    def __init__(self) -> None: ...
    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientUpdatesSubscription": ...
    def __del__(self) -> None: ...
    def pop_batch(self, max_count: int = 16) -> list[Update]: ...
    def wait(self, timeout_milliseconds: int) -> int: ...
    def unsubscribe(self) -> None: ...


class TarwynClientTelemetrySubscription:
    _handle: int | None
    def __init__(self) -> None: ...
    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientTelemetrySubscription": ...
    def __del__(self) -> None: ...
    def pop_batch(self, max_count: int = 16) -> list[Telemetry]: ...
    def wait(self, timeout_milliseconds: int) -> int: ...
    def unsubscribe(self) -> None: ...


class TarwynClientLogsSubscription:
    _handle: int | None
    def __init__(self) -> None: ...
    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientLogsSubscription": ...
    def __del__(self) -> None: ...
    def pop_batch(self, max_count: int = 16) -> list[str]: ...
    def wait(self, timeout_milliseconds: int) -> int: ...
    def unsubscribe(self) -> None: ...




