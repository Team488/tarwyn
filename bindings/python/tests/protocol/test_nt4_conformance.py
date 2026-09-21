import time

import nt4_server
import pytest
from wpimath import Pose2d, Rotation2d

TYPES = [
    ("boolean", "get_boolean_topic", True, False),
    ("double", "get_double_topic", 1.5, 0.0),
    ("int", "get_integer_topic", -7, 0),
    ("float", "get_float_topic", 2.5, 0.0),
    ("string", "get_string_topic", "hello", ""),
    ("boolean[]", "get_boolean_array_topic", [True, False, True], []),
    ("double[]", "get_double_array_topic", [1.5, -2.5], []),
    ("int[]", "get_integer_array_topic", [1, -2, 3], []),
    ("float[]", "get_float_array_topic", [0.5, 1.5], []),
    ("string[]", "get_string_array_topic", ["a", "b"], []),
]


def until(predicate, timeout=15.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def announced_type(inst, name):
    for topic in inst.get_topics():
        if topic.get_name() == name:
            return topic.get_type_string()
    return None


@pytest.mark.parametrize("type_string,accessor,value,default", TYPES)
def test_every_nt4_type_round_trips_between_two_clients(
    nt_client, type_string, accessor, value, default
):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    channel = f"types/{type_string.replace('[]', '_array')}"

    pub = getattr(robot.get_table("t"), accessor)(channel).publish()
    sub = getattr(scope.get_table("t"), accessor)(channel).subscribe(default)

    def published():
        pub.set(value)
        robot.flush()
        return list(sub.get()) == list(value) if isinstance(value, list) else sub.get() == value

    assert until(published), f"{type_string} did not reach the second client"
    assert announced_type(scope, f"/t/{channel}") == type_string


def test_raw_values_round_trip(nt_client):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    pub = robot.get_table("t").get_raw_topic("blob").publish("raw")
    sub = scope.get_table("t").get_raw_topic("blob").subscribe("raw", b"")

    def published():
        pub.set(b"\x01\x02\x03")
        robot.flush()
        return sub.get() == b"\x01\x02\x03"

    assert until(published)
    assert announced_type(scope, "/t/blob") == "raw"


def test_struct_topics_keep_their_own_type_string(nt_client):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    pose = Pose2d(1.0, 2.0, Rotation2d(0.5))

    pub = robot.get_table("t").get_struct_topic("pose", Pose2d).publish()
    sub = scope.get_table("t").get_struct_topic("pose", Pose2d).subscribe(Pose2d())

    def published():
        pub.set(pose)
        robot.flush()
        return abs(sub.get().x - 1.0) < 1e-9

    assert until(published), "a struct value must survive the server"
    assert announced_type(scope, "/t/pose") == "struct:Pose2d", (
        "an unknown type string must be echoed back, not flattened to raw"
    )


def test_a_persistent_topic_outlives_its_publisher(nt_client):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    topic = robot.get_table("t").get_double_topic("persist")
    pub = topic.publish()
    sub = scope.get_table("t").get_double_topic("persist").subscribe(-1.0)

    def published():
        pub.set(7.0)
        robot.flush()
        return sub.get() == 7.0

    assert until(published)
    topic.set_persistent(True)
    robot.flush()
    assert until(lambda: scope.get_table("t").get_double_topic("persist").is_persistent())

    pub.close()
    robot.flush()
    time.sleep(2.0)
    assert scope.get_table("t").get_double_topic("persist").exists(), (
        "a persistent topic must not be deleted when its last publisher leaves"
    )


def test_an_ordinary_topic_is_unannounced_with_its_last_publisher(nt_client):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    pub = robot.get_table("t").get_double_topic("temp").publish()
    sub = scope.get_table("t").get_double_topic("temp").subscribe(-1.0)

    def published():
        pub.set(42.0)
        robot.flush()
        return sub.get() == 42.0

    assert until(published)
    pub.close()
    robot.flush()
    assert until(lambda: not scope.get_table("t").get_double_topic("temp").exists())


def test_a_reconnecting_client_is_re_announced(nt_client):
    robot = nt_client("robot")
    scope = nt_client("dashboard")
    pub = robot.get_table("t").get_double_topic("gyro").publish()

    def published(sub):
        pub.set(3.5)
        robot.flush()
        return sub.get() == 3.5

    first = scope.get_table("t").get_double_topic("gyro").subscribe(-1.0)
    assert until(lambda: published(first))

    scope.stop_client()
    time.sleep(1.0)
    scope.start_client("dashboard")
    scope.set_server(nt4_server.HOST, nt4_server.NT4_PORT)
    assert until(lambda: scope.is_connected())

    second = scope.get_table("t").get_double_topic("gyro").subscribe(-1.0)
    assert until(lambda: published(second)), "the server must re-announce after a reconnect"
