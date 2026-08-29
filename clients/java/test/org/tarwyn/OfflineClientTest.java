package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import static org.junit.jupiter.params.provider.Arguments.arguments;

import java.nio.file.Path;
import java.time.Duration;
import java.util.concurrent.TimeUnit;
import java.util.function.Function;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.CsvSource;
import org.junit.jupiter.params.provider.MethodSource;

/**
 * Covers what the client promises when no server is listening.
 *
 * A robot boots before its coprocessors do, so every one of these paths runs on
 * a real field. None of them may block, throw, or invent a value.
 */
final class OfflineClientTest {
    private static final Path LIBRARY = Path.of(System.getProperty("tarwyn.library"));
    private static final long TIMEOUT_MS = 50;

    private static TarwynClient client() {
        return new TarwynClient(LIBRARY, "127.0.0.1", 21851, 21852, 21853, TIMEOUT_MS, 500);
    }

    @Test
    void construction_does_not_wait_for_a_server() {
        long started = System.nanoTime();
        try (TarwynClient client = client()) {
            long elapsed = Duration.ofNanos(System.nanoTime() - started).toMillis();
            assertNotNull(client);
            assertTrue(elapsed < 2000,
                "construction blocked for " + elapsed + "ms; ZeroMQ should dial in the background");
        }
    }

    @Test
    void publishing_into_the_void_neither_blocks_nor_throws() {
        try (TarwynClient client = client()) {
            long started = System.nanoTime();
            for (int i = 0; i < 200; i++) {
                client.putDouble("nobody-is-listening", i);
            }
            long elapsed = Duration.ofNanos(System.nanoTime() - started).toMillis();
            assertTrue(elapsed < 2000,
                "publishing blocked for " + elapsed + "ms; it should drop rather than queue");
        }
    }

    /// One case per reader, so a regression names the reader that broke rather than
    /// reporting that something among nine did.
    static Stream<Arguments> readers() {
        return Stream.of(
            arguments("getString", (Function<TarwynClient, Object>) c -> c.getString("absent")),
            arguments("getBytes", (Function<TarwynClient, Object>) c -> c.getBytes("absent")),
            arguments("getDouble", (Function<TarwynClient, Object>) c -> c.getDouble("absent")),
            arguments("getInteger", (Function<TarwynClient, Object>) c -> c.getInteger("absent")),
            arguments("getLong", (Function<TarwynClient, Object>) c -> c.getLong("absent")),
            arguments("getFloat", (Function<TarwynClient, Object>) c -> c.getFloat("absent")),
            arguments("getBoolean", (Function<TarwynClient, Object>) c -> c.getBoolean("absent")),
            arguments("getStringList",
                (Function<TarwynClient, Object>) c -> c.getStringList("absent")),
            arguments("getDoubleList",
                (Function<TarwynClient, Object>) c -> c.getDoubleList("absent")),
            arguments("getBooleanList",
                (Function<TarwynClient, Object>) c -> c.getBooleanList("absent")),
            arguments("getCoordinates",
                (Function<TarwynClient, Object>) c -> c.getCoordinates("absent")),
            arguments("getPose2d", (Function<TarwynClient, Object>) c -> c.getPose2d("absent")),
            arguments("getPose3d", (Function<TarwynClient, Object>) c -> c.getPose3d("absent")),
            arguments("getBezierCurve",
                (Function<TarwynClient, Object>) c -> c.getBezierCurve("absent")),
            arguments("getUnknownBytes",
                (Function<TarwynClient, Object>) c -> c.getUnknownBytes("absent")),
            arguments("getServerStatistics",
                (Function<TarwynClient, Object>) TarwynClient::getServerStatistics));
    }

    @ParameterizedTest(name = "{0}")
    @MethodSource("readers")
    void a_read_reports_absence_rather_than_inventing_a_value(
        String name, Function<TarwynClient, Object> read) {
        try (TarwynClient client = client()) {
            assertNull(read.apply(client), name + " invented a value");
        }
    }

    @Test
    void the_control_plane_reports_absence_too() {
        try (TarwynClient client = client()) {
            assertEquals(-1, client.getPing(), "getPing should report failure as -1");
            assertEquals("{}", client.getRawJson(), "raw json should be an empty document");
            assertArrayEquals(new String[0], client.getTables(), "tables invented channels");
            assertEquals(0, client.deleteAll(), "delete claimed to remove something");
        }
    }

    @Test
    void an_unrecognised_tag_is_kept_as_raw_bytes() {
        try (TarwynClient client = client()) {
            assertTrue(client.putTypedBytes("typed", 999, new byte[] {1}),
                "an unrecognised tag should be kept as raw bytes, as TARWYN does");
        }
    }

    /// A recognised tag carrying the wrong number of bytes is not that type.
    @ParameterizedTest(name = "tag {0} rejects {1} bytes")
    @CsvSource({"2, 3", "3, 1", "5, 2", "2, 0"})
    void a_recognised_tag_rejects_bytes_that_are_not_that_type(int tag, int length) {
        try (TarwynClient client = client()) {
            assertFalse(client.putTypedBytes("typed", tag, new byte[length]));
        }
    }

    @Test
    void a_well_formed_typed_payload_is_accepted() {
        try (TarwynClient client = client()) {
            assertTrue(client.putTypedBytes("typed", 2, new byte[] {63, -16, 0, 0, 0, 0, 0, 0}),
                "a big-endian 1.0 was rejected");
        }
    }

    @Test
    void logging_reports_healthy_before_it_is_started() {
        try (TarwynClient client = client()) {
            assertTrue(client.loggingHealthy());
            assertEquals(0, client.droppedLogRecords());
        }
    }

    @Test
    void a_subscription_is_unusable_once_the_client_closes() {
        TarwynClient client = client();
        TarwynClient.Subscription ring = client.subscribe("closing", 8, 64);
        client.close();
        assertThrows(IllegalStateException.class, ring::drain,
            "draining a freed ring must throw rather than read released memory");
        client.close();
    }

    /// A poll that throws is caught so the schedule survives; this pins that it is
    /// counted rather than discarded. A healthy subscription must report nothing.
    @Test
    void a_healthy_subscription_reports_no_poll_failures() throws Exception {
        try (TarwynClient client = client()) {
            assertEquals(0, client.pollFailures());
            assertNull(client.lastPollFailure());

            assertTrue(client.subscribe("polled", payload -> { }, 8, 64, 5));
            Thread.sleep(120);

            assertEquals(0, client.pollFailures(),
                "draining an idle subscription must not be reported as a failure");
            assertNull(client.lastPollFailure());
        }
    }

    @Test
    void utf8_decodes_a_payload() {
        assertEquals("hello", TarwynClient.utf8("hello".getBytes(java.nio.charset.StandardCharsets.UTF_8)));
    }

    @Test
    void the_manager_builds_a_client_without_blocking_the_caller() throws Exception {
        TarwynClientManager manager = TarwynClientManager.getClientAsynchronously(
            "127.0.0.1", LIBRARY);
        assertNotNull(manager.getClientFuture());
        TarwynClient client = manager.getClientFuture().get(30, TimeUnit.SECONDS);
        assertNotNull(client);
        assertTrue(manager.isReady(), "isReady stayed false after the future completed");
        assertNotNull(manager.getOrNull(), "getOrNull returned null after the future completed");
        manager.shutdown();
    }

    @Test
    void a_manager_that_cannot_build_says_so_rather_than_polling_forever() {
        TarwynClientManager manager = TarwynClientManager.getClientAsynchronously(
            "127.0.0.1", Path.of("/nonexistent/libtarwyn_ffi.so"));

        assertThrows(java.util.concurrent.ExecutionException.class,
            () -> manager.getClientFuture().get(30, TimeUnit.SECONDS));
        assertNotNull(manager.failure(), "the failure was swallowed");
        assertThrows(IllegalStateException.class, manager::isReady,
            "isReady returned rather than reporting a client that will never arrive");
        manager.shutdown();
    }

    @Test
    void the_packaged_platform_names_match_what_the_jar_carries() {
        String platform = TarwynClientManager.platform();
        assertTrue(platform.matches("(linux|macos|windows)-(x86_64|aarch64)"),
            "unexpected platform name: " + platform);
        String library = TarwynClientManager.libraryName();
        assertTrue(library.equals("libtarwyn_ffi.so")
                || library.equals("libtarwyn_ffi.dylib")
                || library.equals("tarwyn_ffi.dll"),
            "unexpected library name: " + library);
    }
}
