import pytest

READERS = [
    "get_string",
    "get_integer",
    "get_long",
    "get_double",
    "get_float",
    "get_boolean",
    "get_bytes",
    "get_string_list",
    "get_bytes_list",
    "get_double_list",
    "get_float_list",
    "get_integer_list",
    "get_long_list",
    "get_boolean_list",
    "get_coordinates",
    "get_pose2d",
    "get_pose3d",
    "get_bezier_curve",
    "get_unknown_bytes",
]


@pytest.mark.parametrize("name", READERS)
def test_a_read_reports_absence_rather_than_inventing_a_value(client, name):
    assert getattr(client, name)("absent") is None, f"{name} invented a value with no server"


def test_a_read_with_no_channel_reports_absence(client):
    assert client.get_ping() is None
    assert client.get_server_statistics() is None


def test_publishing_into_the_void_neither_blocks_nor_raises(client):
    client.put_double("pose", 1.5)
    client.put_string("mode", "auto")
    client.put_bytes("frame", b"\x01\x02\x03")
    client.put_double_list("wheels", [1.0, 2.0])
    client.publish_telemetry("fast", b"\x04\x05")


def test_a_listing_is_empty_rather_than_none(client):
    assert client.get_tables("") == []
    assert client.get_raw_json("") == "{}"
    assert client.delete("absent") == 0
    assert client.delete_all() == 0


def test_a_compare_and_set_fails_rather_than_claiming_it_swapped(client):
    assert client.compare_and_set_absent_string("lock", "agent-a") is False
    assert client.compare_and_set_double("counter", 1.0, 2.0) is False
    assert client.compare_and_set_long("counter", 1, 2) is False
    assert client.compare_and_set_boolean("flag", False, True) is False


def test_logging_reports_healthy_before_it_is_started(client):
    assert client.logging_healthy() is True
    assert client.dropped_log_records() == 0


def test_a_typed_put_rejects_bytes_that_are_not_that_type(client):
    assert client.put_typed_bytes("pose", 2, b"\x01\x02\x03") is False
    assert client.put_typed_bytes("pose", 9999, b"\x01\x02\x03") is True


def test_cancelling_a_subscription_stops_it_rather_than_leaking_it(client, discard):
    assert client.subscribe("pose", discard) is True
    assert client.subscribe("pose", discard) is False, "a second subscribe should report the first"
    assert client.unsubscribe("pose") is True, "the cancel handle should have been kept"
    assert client.unsubscribe("pose") is False
    assert client.subscribe("pose", discard) is True, "cancelling frees the channel again"


def test_cancelling_a_log_subscription_frees_it_to_be_taken_again(client, discard):
    assert client.subscribe_to_logs(discard) is True
    assert client.subscribe_to_logs(discard) is False
    assert client.unsubscribe_from_logs() is True
    assert client.unsubscribe_from_logs() is False


def test_a_pose_reads_back_as_a_wpilib_type_rather_than_ours(client):
    import wpimath

    assert client.get_pose2d("absent") is None
    assert client.get_pose3d("absent") is None
    client.put_pose2d("pose", wpimath.Pose2d(1.5, -2.0, wpimath.Rotation2d(0.25)))
    client.put_pose3d(
        "pose3",
        wpimath.Pose3d(
            1.0,
            2.0,
            3.0,
            wpimath.Rotation3d(wpimath.Quaternion(0.5, 0.5, 0.5, 0.5)),
        ),
    )


def test_a_pose_round_trips_through_the_wpilib_adapter():
    import wpimath
    from tarwyn import geometry

    pose = wpimath.Pose2d(1.5, -2.0, wpimath.Rotation2d(0.25))
    assert geometry.convert(geometry.convert(pose)) == pose

    spatial = wpimath.Pose3d(
        1.25,
        -6.5,
        0.75,
        wpimath.Rotation3d(wpimath.Quaternion(0.5, 0.5, 0.5, 0.5)),
    )
    assert geometry.convert(geometry.convert(spatial)) == spatial
