"""What the client does with no server listening.

Every case runs against ports nothing is bound to. A client that blocks, raises,
or invents a value when the server is absent is worse on a coprocessor than one
that reports absence, because the failure surfaces somewhere other than where it
happened.
"""

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
    # Tag 2 is a double, which needs eight bytes.
    assert client.put_typed_bytes("pose", 2, b"\x01\x02\x03") is False
    # An unrecognised tag is kept as raw bytes rather than refused.
    assert client.put_typed_bytes("pose", 9999, b"\x01\x02\x03") is True


def test_cancelling_a_subscription_stops_it_rather_than_leaking_it(client):
    assert client.subscribe("pose") is True
    assert client.subscribe("pose") is False, "a second subscribe should report the first"
    assert client.unsubscribe("pose") is True, "the cancel handle should have been kept"
    assert client.unsubscribe("pose") is False
    assert client.subscribe("pose") is True, "cancelling frees the channel again"


def test_cancelling_a_log_subscription_frees_it_to_be_taken_again(client):
    assert client.subscribe_to_logs() is True
    assert client.subscribe_to_logs() is False
    assert client.unsubscribe_from_logs() is True
    assert client.unsubscribe_from_logs() is False
