"""Covers what the Python client promises when no server is listening.

A coprocessor starts before the server it talks to, so every path here runs on
a real robot. None of them may block, raise, or invent a value.

    cargo build --release -p tarwyn_python
    cp target/release/libtarwyn.so tarwyn.so
    pytest clients/python
"""

import struct
import time

import pytest
import tarwyn

TIMEOUT_MS = 50


@pytest.fixture
def client():
    """A client pointed at ports nothing is listening on."""
    return tarwyn.TarwynClient(
        host="127.0.0.1",
        push_port=47961,
        req_port=47962,
        sub_port=47963,
        request_timeout_ms=TIMEOUT_MS,
    )


def test_construction_does_not_wait_for_a_server():
    started = time.monotonic()
    built = tarwyn.TarwynClient(host="127.0.0.1", request_timeout_ms=TIMEOUT_MS)
    elapsed = time.monotonic() - started
    assert built is not None
    assert elapsed < 2.0, "construction blocked; ZeroMQ should dial in the background"


def test_publishing_into_the_void_neither_blocks_nor_raises(client):
    started = time.monotonic()
    for index in range(200):
        client.put_double("nobody-is-listening", float(index))
    elapsed = time.monotonic() - started
    assert elapsed < 2.0, "publishing blocked; it should drop rather than queue"


@pytest.mark.parametrize(
    "read, absent",
    [
        ("get", None),
        ("get_string", None),
        ("get_double", None),
        ("get_coordinates", None),
        ("get_unknown_bytes", None),
    ],
)
def test_a_read_reports_absence_rather_than_inventing_a_value(client, read, absent):
    assert getattr(client, read)("absent") is absent


def test_the_control_plane_reports_absence_too(client):
    assert client.get_ping() is None
    assert client.get_server_statistics() is None
    assert client.get_raw_json() == "{}"
    assert client.get_tables() == []
    assert client.delete_all() == 0


def test_an_unrecognised_tag_is_kept_as_raw_bytes(client):
    assert client.put_typed_bytes("typed", 999, b"\x01") is True, (
        "an unrecognised tag should be stored as raw bytes, as TARWYN does"
    )


@pytest.mark.parametrize(
    "tag, payload",
    [
        (2, b"\x01\x02\x03"),
        (3, b"\x01"),
        (5, b"\x01\x02"),
    ],
)
def test_a_recognised_tag_rejects_bytes_that_are_not_that_type(client, tag, payload):
    assert client.put_typed_bytes("typed", tag, payload) is False


def test_a_well_formed_typed_payload_is_accepted(client):
    assert client.put_typed_bytes("typed", 2, struct.pack(">d", 1.0)) is True


def test_a_buffered_subscription_starts_empty_and_closes(client):
    subscription = client.subscribe_buffered("nothing", 4)
    assert len(subscription) == 0
    assert subscription.drain() == []
    subscription.close()


def test_a_subscription_works_as_a_context_manager(client):
    with client.subscribe_buffered("nothing", 4) as subscription:
        assert subscription.drain() == []


def test_logging_reports_healthy_before_it_is_started(client):
    assert client.logging_healthy() is True
    assert client.log_dropped() == 0


def public_methods():
    return sorted(
        name for name in dir(tarwyn.TarwynClient) if not name.startswith("_")
    )


@pytest.mark.parametrize("name", public_methods())
def test_every_public_method_is_documented(name):
    assert getattr(tarwyn.TarwynClient, name).__doc__, f"{name} lost its docstring"


def test_the_classes_are_documented():
    assert tarwyn.TarwynClient.__doc__
    assert tarwyn.Subscription.__doc__


@pytest.mark.parametrize(
    "name",
    [
        "put_string",
        "put_integer",
        "put_long",
        "put_double",
        "put_float",
        "put_boolean",
        "put_string_list",
        "put_bytes_list",
        "put_double_list",
        "put_float_list",
        "put_integer_list",
        "put_long_list",
        "put_boolean_list",
        "put_pose2d",
        "put_pose3d",
        "put_coordinates",
        "put_bytes",
        "put_unknown_bytes",
        "put_typed_bytes",
        "get_string",
        "get_integer",
        "get_long",
        "get_double",
        "get_float",
        "get_boolean",
        "get_string_list",
        "get_bytes_list",
        "get_double_list",
        "get_float_list",
        "get_integer_list",
        "get_long_list",
        "get_boolean_list",
        "get_pose2d",
        "get_pose3d",
        "get_coordinates",
        "get_unknown_bytes",
        "get_tables",
        "get_ping",
        "get_server_statistics",
        "get_raw_json",
        "compare_and_set_string",
        "compare_and_set_double",
        "delete",
        "delete_all",
        "start",
        "stop",
        "subscribe_callback",
        "unsubscribe",
        "subscribe_buffered",
        "log_to",
        "log_to_drive",
        "log_dropped",
        "logging_healthy",
        "dropped_publishes",
        "publish_telemetry",
        "subscribe_telemetry",
    ],
)
def test_the_tarwyn_put_get_surface_is_present(name):
    assert hasattr(tarwyn.TarwynClient, name), f"missing {name}"
