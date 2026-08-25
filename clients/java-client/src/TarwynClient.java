import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

public final class TarwynClient implements AutoCloseable {
    private final Arena arena;
    private final TarwynNative native_;
    private final MemorySegment handle;
    private final MemorySegment scratch;
    private boolean closed = false;

    public TarwynClient(Path library, String host) {
        this(library, host, 5557, 5556, 5555, 500, 500);
    }

    public TarwynClient(Path library, String host, int pushPort, int reqPort, int subPort,
                         long requestTimeoutMillis, int sendHighWaterMark) {
        this.arena = Arena.ofShared();
        this.native_ = new TarwynNative(library, arena);
        this.scratch = arena.allocate(ValueLayout.JAVA_LONG);
        try {
            MemorySegment hostSegment = arena.allocateFrom(host);
            this.handle = (MemorySegment) native_.clientNew.invokeExact(
                hostSegment, (short) pushPort, (short) reqPort, (short) subPort,
                requestTimeoutMillis, sendHighWaterMark);
        } catch (Throwable t) {
            arena.close();
            throw new IllegalStateException("could not construct the native client", t);
        }
        if (handle.equals(MemorySegment.NULL)) {
            arena.close();
            throw new IllegalStateException("native client construction returned null");
        }
    }

    public void start() {
        check(invokeInt(() -> (int) native_.clientStart.invokeExact(handle)), "start");
    }

    public void publish(String channel, double value) {
        MemorySegment name = arena.allocateFrom(channel);
        check(invokeInt(() -> (int) native_.publishDouble.invokeExact(handle, name, value)),
            "publish double");
    }

    public void publish(String channel, boolean value) {
        MemorySegment name = arena.allocateFrom(channel);
        check(invokeInt(() -> (int) native_.publishBool.invokeExact(handle, name, value)),
            "publish bool");
    }

    public void publish(String channel, String value) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment body = arena.allocateFrom(value);
        check(invokeInt(() -> (int) native_.publishString.invokeExact(handle, name, body)),
            "publish string");
    }

    public void publish(String channel, byte[] value) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment body = arena.allocateFrom(ValueLayout.JAVA_BYTE, value);
        check(invokeInt(() ->
            (int) native_.publishBytes.invokeExact(handle, name, body, (long) value.length)),
            "publish bytes");
    }

    public Double getDouble(String channel) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment out = arena.allocate(ValueLayout.JAVA_DOUBLE);
        int code = invokeInt(() -> (int) native_.getDouble.invokeExact(handle, name, out));
        if (code == TarwynNative.XT_ERR_NO_VALUE || code == TarwynNative.XT_ERR_WRONG_TYPE) {
            return null;
        }
        check(code, "get double");
        return out.get(ValueLayout.JAVA_DOUBLE, 0);
    }

    public long droppedPublishes() {
        check(invokeInt(() -> (int) native_.droppedPublishes.invokeExact(handle, scratch)),
            "dropped publishes");
        return scratch.get(ValueLayout.JAVA_LONG, 0);
    }

    public Subscription subscribe(String channel, int records, int recordBytes) {
        MemorySegment name = arena.allocateFrom(channel);
        MemorySegment out = arena.allocate(ValueLayout.JAVA_LONG);
        check(invokeInt(() -> (int) native_.subscribeRing.invokeExact(
            handle, name, (long) records, (long) recordBytes, out)), "subscribe");
        long id = out.get(ValueLayout.JAVA_LONG, 0);
        return new Subscription(id, records, recordBytes);
    }

    private static int invokeInt(NativeCall call) {
        try {
            return call.invoke();
        } catch (Throwable t) {
            throw new IllegalStateException("native call failed", t);
        }
    }

    private static void check(int code, String what) {
        if (code != TarwynNative.XT_OK) {
            throw new IllegalStateException(what + " failed: " + TarwynNative.describe(code));
        }
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }
        closed = true;
        try {
            native_.clientFree.invokeExact(handle);
        } catch (Throwable t) {
            throw new IllegalStateException("native client teardown failed", t);
        } finally {
            arena.close();
        }
    }

    private interface NativeCall {
        int invoke() throws Throwable;
    }

    public final class Subscription implements AutoCloseable {
        private final long id;
        private final int records;
        private final int recordBytes;
        private final MemorySegment ring;
        private long readIndex = 0;
        private boolean released = false;

        private Subscription(long id, int records, int recordBytes) {
            this.id = id;
            this.records = records;
            this.recordBytes = recordBytes;
            MemorySegment base;
            try {
                base = (MemorySegment) native_.ringBase.invokeExact((MemorySegment) handle, id);
            } catch (Throwable t) {
                throw new IllegalStateException("could not obtain the ring base address", t);
            }
            if (base.equals(MemorySegment.NULL)) {
                throw new IllegalStateException("ring base address was null");
            }
            this.ring = base.reinterpret((long) records * recordBytes);
        }

        public long writeIndex() {
            requireLive();
            MemorySegment out = arena.allocate(ValueLayout.JAVA_LONG);
            check(invokeInt(() ->
                (int) native_.ringWriteIndex.invokeExact((MemorySegment) handle, id, out)),
                "read write index");
            return out.get(ValueLayout.JAVA_LONG, 0);
        }

        public List<byte[]> drain() {
            requireLive();
            long available = writeIndex();
            List<byte[]> values = new ArrayList<>();
            if (available <= readIndex) {
                return values;
            }
            long from = Math.max(readIndex, available - records);
            for (long index = from; index < available; index++) {
                long offset = (index % records) * (long) recordBytes;
                int length = (int) ring.get(ValueLayout.JAVA_LONG, offset);
                if (length < 0 || length > recordBytes - 8) {
                    continue;
                }
                byte[] payload = new byte[length];
                MemorySegment.copy(ring, ValueLayout.JAVA_BYTE, offset + 8, payload, 0, length);
                if (writeIndex() - index <= records) {
                    values.add(payload);
                }
            }
            readIndex = available;
            return values;
        }

        public boolean lapped() {
            return writeIndex() - readIndex > records;
        }

        private void requireLive() {
            if (released || closed) {
                throw new IllegalStateException(
                    "the ring is freed once the subscription or client is closed");
            }
        }

        @Override
        public void close() {
            if (released || closed) {
                return;
            }
            released = true;
            check(invokeInt(() ->
                (int) native_.unsubscribe.invokeExact((MemorySegment) handle, id)), "unsubscribe");
        }
    }

    public static String utf8(byte[] value) {
        return new String(value, StandardCharsets.UTF_8);
    }
}
