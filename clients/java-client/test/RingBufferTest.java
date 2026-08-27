import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static tarwyn.ffi.tarwyn_h.XT_OK;
import static tarwyn.ffi.tarwyn_h.xt_ring_push;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/**
 * Covers the one contract neither language can test alone: Rust writes the ring,
 * Java reads the mapped bytes directly without taking Rust's lock.
 *
 * A disagreement about the byte layout or the lap guard fails rarely, under
 * load, with corrupted values, so the ring here is deliberately tiny and is
 * driven until it wraps.
 */
final class RingBufferTest {
    private static final Path LIBRARY = Path.of(System.getProperty("tarwyn.library"));
    private static final int RECORDS = 8;
    private static final int RECORD_BYTES = 64;
    private static final int PAYLOAD = RECORD_BYTES - 8;

    private static TarwynClient offlineClient() {
        return new TarwynClient(LIBRARY, "127.0.0.1", 47941, 47942, 47943, 50, 500);
    }

    private static void push(TarwynClient client, long id, MemorySegment payload) {
        int code = xt_ring_push(client.handle, id, payload, payload.byteSize());
        if (code != XT_OK()) {
            throw new IllegalStateException("ring push failed: " + code);
        }
    }

    private static long readLong(byte[] value) {
        return ByteBuffer.wrap(value).order(ByteOrder.nativeOrder()).getLong();
    }

    @Test
    void a_record_crosses_the_boundary_byte_for_byte() {
        try (TarwynClient client = offlineClient()) {
            TarwynClient.Subscription ring = client.subscribe("layout", RECORDS, RECORD_BYTES);

            byte[] written = new byte[PAYLOAD];
            for (int i = 0; i < written.length; i++) {
                written[i] = (byte) (i * 7 + 1);
            }
            try (Arena arena = Arena.ofConfined()) {
                MemorySegment slot = arena.allocate(written.length);
                MemorySegment.copy(written, 0, slot, ValueLayout.JAVA_BYTE, 0, written.length);
                push(client, ring.id(), slot);
            }

            List<byte[]> drained = ring.drain();
            assertEquals(1, drained.size(), "one record in, one record out");
            assertArrayEquals(written, drained.get(0), "Java read back bytes Rust did not write");
            ring.close();
        }
    }

    @Test
    void a_lapped_ring_drops_the_overwritten_and_keeps_the_newest() {
        try (TarwynClient client = offlineClient()) {
            TarwynClient.Subscription ring = client.subscribe("lap", RECORDS, RECORD_BYTES);

            long total = RECORDS * 3L;
            try (Arena arena = Arena.ofConfined()) {
                MemorySegment slot = arena.allocate(8);
                for (long value = 0; value < total; value++) {
                    slot.set(ValueLayout.JAVA_LONG, 0, value);
                    push(client, ring.id(), slot);
                }
            }

            assertTrue(ring.lapped(), "the writer lapped the reader but lapped() denied it");
            List<byte[]> drained = ring.drain();
            assertEquals(RECORDS, drained.size(), "a lapped ring returned more than it holds");
            for (int i = 0; i < drained.size(); i++) {
                assertEquals(total - RECORDS + i, readLong(drained.get(i)),
                    "a lapped ring returned something other than the newest records");
            }
            ring.close();
        }
    }

    @Test
    void concurrent_writes_are_never_torn_duplicated_or_reordered() throws Exception {
        try (TarwynClient client = offlineClient()) {
            TarwynClient.Subscription ring = client.subscribe("soak", RECORDS, RECORD_BYTES);

            final long total = 50_000;
            Thread writer = new Thread(() -> {
                try (Arena arena = Arena.ofConfined()) {
                    MemorySegment slot = arena.allocate(8);
                    for (long value = 0; value < total; value++) {
                        slot.set(ValueLayout.JAVA_LONG, 0, value);
                        push(client, ring.id(), slot);
                    }
                }
            }, "ring-writer");
            writer.start();

            List<Long> seen = new ArrayList<>();
            int malformed = 0;
            while (writer.isAlive()) {
                for (byte[] value : ring.drain()) {
                    if (value.length != 8) {
                        malformed++;
                    } else {
                        seen.add(readLong(value));
                    }
                }
            }
            writer.join();
            for (byte[] value : ring.drain()) {
                if (value.length != 8) {
                    malformed++;
                } else {
                    seen.add(readLong(value));
                }
            }

            assertFalse(seen.isEmpty(), "nothing was read, so nothing was tested");
            assertEquals(0, malformed, "a record came back the wrong length");

            Set<Long> unique = new HashSet<>();
            long duplicates = 0;
            long outOfOrder = 0;
            for (int i = 0; i < seen.size(); i++) {
                if (!unique.add(seen.get(i))) {
                    duplicates++;
                }
                if (i > 0 && seen.get(i) <= seen.get(i - 1)) {
                    outOfOrder++;
                }
            }
            assertEquals(0, duplicates, "the ring handed back a value twice");
            assertEquals(0, outOfOrder, "the ring handed back values out of order");
            assertTrue(seen.stream().allMatch(value -> value >= 0 && value < total),
                "the ring handed back a value the writer never wrote");
            ring.close();
        }
    }
}
