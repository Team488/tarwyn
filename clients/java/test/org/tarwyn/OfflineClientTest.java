package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Path;
import java.time.Duration;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

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
        return new TarwynClient(LIBRARY, "127.0.0.1", 47951, 47952, 47953, TIMEOUT_MS, 500);
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

    @Test
    void reads_report_absence_rather_than_inventing_a_value() {
        try (TarwynClient client = client()) {
            assertNull(client.getString("absent"), "getString invented a value");
            assertNull(client.getBytes("absent"), "getBytes invented a value");
            assertNull(client.getDouble("absent"), "getDouble invented a value");
            assertNull(client.getPose2d("absent"), "getPose2d invented a value");
            assertEquals(-1, client.getPing(), "getPing should report failure as -1");
            assertNull(client.getServerStatistics(), "statistics invented a server");
            assertEquals("{}", client.getRawJson(), "raw json should be an empty document");
            assertArrayEquals(new String[0], client.getTables(), "tables invented channels");
            assertEquals(0, client.deleteAll(), "delete claimed to remove something");
        }
    }

    @Test
    void a_typed_byte_payload_is_validated_before_it_is_published() {
        try (TarwynClient client = client()) {
            assertTrue(client.putTypedBytes("typed", 999, new byte[] {1}),
                "an unrecognised tag should be kept as raw bytes, as TARWYN does");
            assertFalse(client.putTypedBytes("typed", 2, new byte[] {1, 2, 3}),
                "a double tag was accepted with three bytes");
            assertFalse(client.putTypedBytes("typed", 3, new byte[] {1}),
                "an int32 tag was accepted with one byte");
            assertTrue(client.putTypedBytes("typed", 2, new byte[] {63, -16, 0, 0, 0, 0, 0, 0}),
                "a big-endian 1.0 was rejected");
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
