"""Covers what the Python client promises when no server is listening.

A coprocessor starts before the server it talks to, so every path here runs on
a real robot. None of them may block, raise, or invent a value.

Run with the built extension module importable:

    cargo build --release -p tarwyn_python
    cp target/release/libtarwyn.so tarwyn.so
    python3 -m unittest discover -s clients/python-client
"""

import struct
import time
import unittest

import tarwyn

TIMEOUT_MS = 50


def offline_client():
    return tarwyn.TarwynClient(
        host="127.0.0.1",
        push_port=47961,
        req_port=47962,
        sub_port=47963,
        request_timeout_ms=TIMEOUT_MS,
    )


class OfflineClient(unittest.TestCase):
    def test_construction_does_not_wait_for_a_server(self):
        started = time.monotonic()
        client = offline_client()
        elapsed = time.monotonic() - started
        self.assertIsNotNone(client)
        self.assertLess(elapsed, 2.0, "construction blocked; ZeroMQ should dial in the background")

    def test_publishing_into_the_void_neither_blocks_nor_raises(self):
        client = offline_client()
        started = time.monotonic()
        for index in range(200):
            client.put_double("nobody-is-listening", float(index))
        elapsed = time.monotonic() - started
        self.assertLess(elapsed, 2.0, "publishing blocked; it should drop rather than queue")

    def test_reads_report_absence_rather_than_inventing_a_value(self):
        client = offline_client()
        self.assertIsNone(client.get("absent"))
        self.assertIsNone(client.get_string("absent"))
        self.assertIsNone(client.get_double("absent"))
        self.assertIsNone(client.get_coordinates("absent"))
        self.assertIsNone(client.get_unknown_bytes("absent"))
        self.assertIsNone(client.get_ping())
        self.assertIsNone(client.get_server_statistics())
        self.assertEqual("{}", client.get_raw_json())
        self.assertEqual([], client.get_tables())
        self.assertEqual(0, client.delete_all())

    def test_a_typed_byte_payload_is_validated_before_it_is_published(self):
        client = offline_client()
        self.assertTrue(
            client.put_typed_bytes("typed", 999, b"\x01"),
            "an unrecognised tag should be kept as raw bytes, as TARWYN does",
        )
        self.assertFalse(
            client.put_typed_bytes("typed", 2, b"\x01\x02\x03"),
            "a double tag was accepted with three bytes",
        )
        self.assertTrue(
            client.put_typed_bytes("typed", 2, struct.pack(">d", 1.0)),
            "a big-endian 1.0 was rejected",
        )

    def test_a_buffered_subscription_starts_empty_and_closes(self):
        client = offline_client()
        subscription = client.subscribe_buffered("nothing", 4)
        self.assertEqual(0, len(subscription))
        self.assertEqual([], subscription.drain())
        subscription.close()

    def test_a_subscription_works_as_a_context_manager(self):
        client = offline_client()
        with client.subscribe_buffered("nothing", 4) as subscription:
            self.assertEqual([], subscription.drain())

    def test_logging_reports_healthy_before_it_is_started(self):
        client = offline_client()
        self.assertTrue(client.logging_healthy())
        self.assertEqual(0, client.log_dropped())


class PublicSurface(unittest.TestCase):
    def test_every_public_method_is_documented(self):
        undocumented = [
            name
            for name in dir(tarwyn.TarwynClient)
            if not name.startswith("_")
            and not getattr(getattr(tarwyn.TarwynClient, name), "__doc__", None)
        ]
        self.assertEqual([], undocumented, "these methods lost their docstrings")
        self.assertTrue(tarwyn.TarwynClient.__doc__, "the class lost its docstring")
        self.assertTrue(tarwyn.Subscription.__doc__, "Subscription lost its docstring")

    def test_the_tarwyn_put_get_surface_is_present(self):
        for name in [
            "put_string", "put_integer", "put_long", "put_double", "put_float",
            "put_boolean", "put_string_list", "put_bytes_list", "put_double_list",
            "put_float_list", "put_integer_list", "put_long_list", "put_boolean_list",
            "put_pose2d", "put_pose3d", "put_coordinates", "put_bytes",
            "put_unknown_bytes", "put_typed_bytes",
            "get_string", "get_integer", "get_long", "get_double", "get_float",
            "get_boolean", "get_string_list", "get_bytes_list", "get_double_list",
            "get_float_list", "get_integer_list", "get_long_list", "get_boolean_list",
            "get_pose2d", "get_pose3d", "get_coordinates", "get_unknown_bytes",
            "get_tables", "get_ping", "get_server_statistics", "get_raw_json",
            "compare_and_set_string", "compare_and_set_double",
            "delete", "delete_all", "start", "stop",
            "subscribe_callback", "unsubscribe", "subscribe_buffered",
            "log_to", "log_to_drive", "log_dropped", "logging_healthy",
            "dropped_publishes",
        ]:
            self.assertTrue(hasattr(tarwyn.TarwynClient, name), f"missing {name}")


if __name__ == "__main__":
    unittest.main()
