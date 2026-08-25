# tarwyn Java client

Binds the [C ABI](../ffi) through Java's Foreign Function & Memory API. There is
no JNI and no hand-written native glue: the functions are described in Java with
`FunctionDescriptor` and called through `MethodHandle`.

Requires **JDK 22 or newer**, where FFM is final. That means it targets the 2027
season — WPILib 2026 runs JDK 17, where FFM does not exist even as a preview.

    cargo build --release -p tarwyn_ffi
    javac -d out src/*.java
    java --enable-native-access=ALL-UNNAMED -cp out SmokeTest

## Use

```java
Path library = Path.of("target/release/libtarwyn_ffi.so");
try (TarwynClient client = new TarwynClient(library, "10.4.88.2")) {
    client.publish("target_pose", 1.5);
    client.start();

    try (var subscription = client.subscribe("BEZIER_PATH", 64, 4096)) {
        for (byte[] payload : subscription.drain()) {
            // decode however the publisher encoded it
        }
    }
}
```

Construction does not block. ZeroMQ dials in the background, so a client may be
built before the server exists — which is the normal case on a robot, where code
boots before its coprocessors. `getDouble` returns `null` rather than waiting
forever when there is no value, and publishing discards rather than stalling when
the send queue is full. `droppedPublishes()` reports how much was discarded, so
loss is visible instead of silent.

## Subscriptions read shared memory, not callbacks

A subscription allocates a ring in native memory and returns a handle. Java reads
it through a `MemorySegment` and never receives an upcall, because native-to-Java
calls cost roughly 300ns and that is the single most expensive thing on this path.

`drain()` returns everything written since the last call. Call it from
`periodic()`; the data sits in the ring until then, which is the same shape as
NetworkTables' `readQueue()` and means callbacks never fire on a background
thread you have to synchronise against.

Size the ring for the gap between drains: `records` must exceed the number of
messages that can arrive between two calls, or the writer laps the reader and the
oldest values are overwritten. `lapped()` reports whether that has happened, and
`drain()` skips records that were overwritten while being copied rather than
returning torn data.

`recordBytes` bounds a single message — eight bytes of length prefix plus the
payload. Larger payloads are truncated, not reallocated.

**A subscription's memory is freed when it or the client is closed.** Both are
`AutoCloseable` and the ring is scoped to the subscription rather than the client,
so try-with-resources releases it at the right time. Using a subscription after
closing it throws rather than reading freed memory.

## Memory ordering

The ring's write index is published from Rust with release ordering. Java reads it
through `xt_ring_write_index` rather than reading the shared memory directly, so
the acquire load happens on the Rust side where it is expressed once and
correctly.

That costs a downcall per poll instead of a plain memory read. It is deliberate:
a plain read of that index from Java is a data race, and its failure mode is rare
corrupted values under load rather than an obvious crash — the worst thing to
debug from a competition. The downcall is a few nanoseconds against a path whose
cheapest hop is already several microseconds.
