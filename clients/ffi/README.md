# tarwyn C ABI

A flat `extern "C"` surface over `tarwyn_client`, built as a `cdylib` and
consumed by the Java FFM client. There is no JNI here — foreign callers link the
shared library directly and describe the functions themselves, which is why the
header is the whole interface.

    cargo build --release -p tarwyn_ffi        # target/release/libtarwyn_ffi.so
    jextract --output ../java-client/src/gen include/tarwyn.h

## Return codes

Every function that can fail returns `int`. `XT_OK` is 0; everything else is
negative and never overlaps a valid result.

| Code | Meaning |
|---|---|
| `XT_ERR_NULL` | a required pointer was null, or a lock was poisoned |
| `XT_ERR_UTF8` | a string argument was not valid UTF-8 |
| `XT_ERR_NO_VALUE` | no value for that channel, or no such subscription |
| `XT_ERR_WRONG_TYPE` | the channel holds a different type than the one requested |
| `XT_ERR_PANIC` | a Rust panic was caught at the boundary |

No function unwinds into the caller. Every entry point is wrapped in
`catch_unwind`, because a panic crossing an FFI boundary is undefined behaviour
rather than an exception the caller can handle.

## Nothing here blocks

`xt_client_new` returns before a connection exists — ZeroMQ dials in the
background, so a client may be constructed before the server is running. That is
the normal case on a robot, where code boots before its coprocessors do.
`xt_get_double` gives up after `request_timeout_ms` and returns
`XT_ERR_NO_VALUE`; publishes discard rather than stall when the send queue is
full, and `xt_dropped_publishes` reports how many were discarded so the loss is
visible rather than silent.

## The ring

`xt_subscribe_ring` allocates a fixed ring of `records` slots of `record_bytes`
each and returns an id. Each slot is a little-endian `uint64` length followed by
that many payload bytes, so `record_bytes` must exceed 8 and bounds the largest
message a slot can hold — longer payloads are truncated, not reallocated.

`xt_ring_base` returns the base address and `xt_ring_write_index` the count of
records written so far. A reader computes a slot as
`base + (index % records) * record_bytes` and detects overwrite by re-reading the
write index after copying the payload out: if it advanced by more than `records`
in between, the slot was reused mid-read and the value must be discarded.

**The ring is freed by `xt_unsubscribe` and by `xt_client_free`.** A base pointer
obtained before either call dangles afterwards. Readers must stop using the
address before unsubscribing, which the Java client enforces by scoping the
`MemorySegment` to the subscription's lifetime rather than the client's.

The write index is published with release ordering and must be read with acquire
ordering. A plain read is a data race that will surface as rare corrupted values
under load rather than as an obvious failure, which is why the Java client uses
`VarHandle::getAcquire` and not a plain field read.
