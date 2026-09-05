"""Python client for the tarwyn key/value server.

Poses cross the wire in WPILib's struct layout. The four pose calls below are
rebound so they take and return WPILib's own geometry types, matching the Java
client; :mod:`tarwyn.geometry` converts between those and the wire types.
"""

from .tarwyn import *  # noqa: F401,F403
from .tarwyn import TarwynClient
from . import geometry


def _converting_reader(read):
    def method(self, channel):
        pose = read(self, channel)
        return None if pose is None else geometry.convert(pose)

    method.__name__ = read.__name__
    method.__doc__ = read.__doc__
    return method


def _converting_writer(write):
    def method(self, channel, value):
        write(self, channel, geometry.convert(value))

    method.__name__ = write.__name__
    method.__doc__ = write.__doc__
    return method


TarwynClient.get_pose2d = _converting_reader(TarwynClient.get_pose2d)
TarwynClient.get_pose3d = _converting_reader(TarwynClient.get_pose3d)
TarwynClient.put_pose2d = _converting_writer(TarwynClient.put_pose2d)
TarwynClient.put_pose3d = _converting_writer(TarwynClient.put_pose3d)
