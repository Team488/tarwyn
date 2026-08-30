"""What the generated Python client does with no server listening.

Every case runs against ports nothing is bound to. A client that blocks, raises,
or invents a value when the server is absent is worse on a coprocessor than one
that reports absence, because the failure surfaces somewhere other than where it
happened.
"""

import pytest
import tarwyn

# Python has no constructor overloading, so the extra constructors are generated
# as classmethods rather than the overloads Java gets.
OFFLINE = ("127.0.0.1", 26982, 26983, 26981, 26984, 150, 500)


@pytest.fixture
def client():
    # The generated client releases through __del__ rather than an explicit
    # close, so dropping the reference is the whole teardown.
    yield tarwyn.TarwynClient.with_ports(*OFFLINE)


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


def test_the_surface_matches_what_tarwyn_promises(client):
    for name in READERS:
        assert hasattr(client, name), f"missing {name}"
    for name in [
        "put_string",
        "put_integer",
        "put_long",
        "put_double",
        "put_float",
        "put_boolean",
        "put_bytes",
        "put_string_list",
        "put_bytes_list",
        "put_double_list",
        "put_float_list",
        "put_integer_list",
        "put_long_list",
        "put_boolean_list",
        "put_coordinates",
        "put_pose2d",
        "put_pose3d",
        "put_bezier_curve",
        "put_unknown_bytes",
        "put_typed_bytes",
        "delete",
        "delete_all",
        "get_tables",
        "get_ping",
        "get_server_statistics",
        "get_raw_json",
        "start",
        "stop",
        "publish_telemetry",
        "subscribe",
        "subscribe_telemetry",
        "subscribe_to_logs",
        "dropped_publishes",
        "logging_healthy",
    ]:
        assert hasattr(client, name), f"missing {name}"
