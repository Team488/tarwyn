"""Geometry the client carries that ``wpimath`` has no type for."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Point:
    """A control point of a bezier curve; ``rotation_degrees`` is ``None`` for a
    point with no heading."""

    x: float
    y: float
    rotation_degrees: float | None = None
