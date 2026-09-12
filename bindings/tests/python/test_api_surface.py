import pytest

PUBLISHERS = [
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
    "put_bezier_curves",
    "put_bezier_curves_list",
    "put_unknown_bytes",
    "put_typed_bytes",
]

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
    "get_bezier_curves",
    "get_bezier_curves_list",
    "get_unknown_bytes",
]

CONTROL_PLANE = [
    "delete",
    "delete_all",
    "get_tables",
    "get_ping",
    "get_server_statistics",
    "get_raw_json",
    "start",
    "stop",
]

COMPARE_AND_SET = [
    "compare_and_set_absent_string",
    "compare_and_set_string",
    "compare_and_set_double",
    "compare_and_set_long",
    "compare_and_set_boolean",
]

SUBSCRIPTIONS_AND_LOGGING = [
    "subscribe",
    "subscribe_telemetry",
    "subscribe_to_logs",
    "unsubscribe",
    "unsubscribe_telemetry",
    "unsubscribe_from_logs",
    "publish_telemetry",
    "dropped_publishes",
    "dropped_log_records",
    "log_to",
    "log_to_drive",
    "logging_healthy",
]


@pytest.mark.parametrize("name", PUBLISHERS)
def test_a_publisher_exists(client, name):
    assert hasattr(client, name), f"missing {name}"


@pytest.mark.parametrize("name", READERS)
def test_a_reader_exists(client, name):
    assert hasattr(client, name), f"missing {name}"


@pytest.mark.parametrize("name", CONTROL_PLANE)
def test_a_control_plane_call_exists(client, name):
    assert hasattr(client, name), f"missing {name}"


@pytest.mark.parametrize("name", COMPARE_AND_SET)
def test_a_compare_and_set_exists(client, name):
    assert hasattr(client, name), f"missing {name}"


@pytest.mark.parametrize("name", SUBSCRIPTIONS_AND_LOGGING)
def test_a_subscription_or_logging_call_exists(client, name):
    assert hasattr(client, name), f"missing {name}"
