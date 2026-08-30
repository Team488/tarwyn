package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Optional;
import java.util.function.Function;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.MethodSource;

final class OfflineClientTest {
    private static TarwynClient offline() {
        return new TarwynClient(
            "127.0.0.1", (short) 26882, (short) 26883, (short) 26881, (short) 26884, 150L, 500);
    }

    static List<Arguments> readers() {
        return List.of(
            Arguments.of("getString", (Function<TarwynClient, Optional<?>>) c -> c.getString("absent")),
            Arguments.of("getInteger", (Function<TarwynClient, Optional<?>>) c -> c.getInteger("absent")),
            Arguments.of("getLong", (Function<TarwynClient, Optional<?>>) c -> c.getLong("absent")),
            Arguments.of("getDouble", (Function<TarwynClient, Optional<?>>) c -> c.getDouble("absent")),
            Arguments.of("getFloat", (Function<TarwynClient, Optional<?>>) c -> c.getFloat("absent")),
            Arguments.of("getBoolean", (Function<TarwynClient, Optional<?>>) c -> c.getBoolean("absent")),
            Arguments.of("getBytes", (Function<TarwynClient, Optional<?>>) c -> c.getBytes("absent")),
            Arguments.of("getStringList", (Function<TarwynClient, Optional<?>>) c -> c.getStringList("absent")),
            Arguments.of("getBytesList", (Function<TarwynClient, Optional<?>>) c -> c.getBytesList("absent")),
            Arguments.of("getDoubleList", (Function<TarwynClient, Optional<?>>) c -> c.getDoubleList("absent")),
            Arguments.of("getFloatList", (Function<TarwynClient, Optional<?>>) c -> c.getFloatList("absent")),
            Arguments.of("getIntegerList", (Function<TarwynClient, Optional<?>>) c -> c.getIntegerList("absent")),
            Arguments.of("getLongList", (Function<TarwynClient, Optional<?>>) c -> c.getLongList("absent")),
            Arguments.of("getBooleanList", (Function<TarwynClient, Optional<?>>) c -> c.getBooleanList("absent")),
            Arguments.of("getCoordinates", (Function<TarwynClient, Optional<?>>) c -> c.getCoordinates("absent")),
            Arguments.of("getPose2d", (Function<TarwynClient, Optional<?>>) c -> c.getPose2d("absent")),
            Arguments.of("getPose3d", (Function<TarwynClient, Optional<?>>) c -> c.getPose3d("absent")),
            Arguments.of("getBezierCurve", (Function<TarwynClient, Optional<?>>) c -> c.getBezierCurve("absent")),
            Arguments.of("getUnknownBytes", (Function<TarwynClient, Optional<?>>) c -> c.getUnknownBytes("absent")),
            Arguments.of("getPing", (Function<TarwynClient, Optional<?>>) c -> c.getPing()),
            Arguments.of("getServerStatistics",
                (Function<TarwynClient, Optional<?>>) c -> c.getServerStatistics()));
    }

    @ParameterizedTest(name = "{0}")
    @MethodSource("readers")
    void a_read_reports_absence_rather_than_inventing_a_value(
        String name, Function<TarwynClient, Optional<?>> read) {
        try (TarwynClient client = offline()) {
            assertTrue(read.apply(client).isEmpty(), name + " invented a value with no server");
        }
    }

    @Test
    void publishing_into_the_void_neither_blocks_nor_throws() {
        try (TarwynClient client = offline()) {
            assertDoesNotThrow(() -> {
                client.putDouble("pose", 1.5);
                client.putString("mode", "auto");
                client.putBytes("frame", new byte[] {1, 2, 3});
                client.putDoubleList("wheels", new double[] {1.0, 2.0});
                client.publishTelemetry("fast", new byte[] {4, 5});
            });
        }
    }

    @Test
    void a_listing_is_empty_rather_than_null_when_the_server_is_absent() {
        try (TarwynClient client = offline()) {
            assertTrue(client.getTables("").isEmpty());
            assertEquals("{}", client.getRawJson(""));
            assertEquals(0, client.delete("absent"));
            assertEquals(0, client.deleteAll());
        }
    }

    @Test
    void a_compare_and_set_fails_rather_than_claiming_it_swapped() {
        try (TarwynClient client = offline()) {
            assertFalse(client.compareAndSetAbsentString("lock", "agent-a"));
            assertFalse(client.compareAndSetDouble("counter", 1.0, 2.0));
            assertFalse(client.compareAndSetLong("counter", 1L, 2L));
            assertFalse(client.compareAndSetBoolean("flag", false, true));
        }
    }

    @Test
    void logging_reports_healthy_before_it_is_started() {
        try (TarwynClient client = offline()) {
            assertTrue(client.loggingHealthy());
            assertEquals(0L, client.droppedLogRecords());
        }
    }

    @Test
    void a_typed_put_rejects_bytes_that_are_not_that_type() {
        try (TarwynClient client = offline()) {
            assertFalse(client.putTypedBytes("pose", 2, new byte[] {1, 2, 3}));
            assertTrue(client.putTypedBytes("pose", 9999, new byte[] {1, 2, 3}));
        }
    }

    @Test
    void cancelling_a_subscription_stops_it_rather_than_leaking_it() {
        try (TarwynClient client = offline()) {
            assertTrue(client.subscribe("pose"), "the first subscribe should take");
            assertFalse(client.subscribe("pose"), "a second subscribe should report the first");
            assertTrue(client.unsubscribe("pose"), "the cancel handle should have been kept");
            assertFalse(client.unsubscribe("pose"), "cancelling twice should report the first");
            assertTrue(client.subscribe("pose"));
        }
    }

    @Test
    void cancelling_a_log_subscription_frees_it_to_be_taken_again() {
        try (TarwynClient client = offline()) {
            assertTrue(client.subscribeToLogs());
            assertFalse(client.subscribeToLogs());
            assertTrue(client.unsubscribeFromLogs());
            assertFalse(client.unsubscribeFromLogs());
        }
    }

    @Test
    void a_subscription_closes_though_its_type_is_package_private() {
        try (TarwynClient client = offline()) {
            assertDoesNotThrow(() -> {
                AutoCloseable updates = client.updates(update -> { });
                updates.close();
            });
        }
    }
}
