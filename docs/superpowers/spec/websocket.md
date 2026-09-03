# Tarwyn NT4 WebSocket Server

## Goal

Expose Tarwyn as a **NetworkTables 4.1 server** for compatibility with NT4 clients and tools such as AdvantageScope and WPILib tooling.

The server should expose a single endpoint such as:

```text
example_ip:4881
```

NT4 uses **WebSocket over TCP**. UDP is not part of the NT4 compatibility transport.

## Architecture

```text
TCP :4881
   ↓
WebSocket (RFC 6455)
   ↓
NetworkTables 4.1
   ↓
Tarwyn core
```

The Tarwyn core must remain independent of WebSocket and NT4 wire formats.

## WebSocket implementation

**Decision:** tungstenite **0.29** was benchmarked and chosen (p50 ~12.9 µs at
16 B on loopback); an in-house implementation is deferred unless benchmarks
later show a meaningful advantage (per the design rule below).

Evaluated approaches:

### High-performance crate

Benchmark a low-level implementation such as:

* `fastwebsockets`
* `nexus-web`

Prefer APIs with low allocation overhead, borrowed parsing, minimal abstraction, and efficient writes.

### In-house implementation

If the crate implementation cannot meet the performance target, build a small purpose-built RFC 6455 implementation inspired by the design of `fastwebsockets` and `nexus-web`.

The in-house layer should use:

* explicit frame-parser state machines
* borrowed input buffers
* reusable `Bytes`/`BytesMut`-style storage
* batched/vectored writes
* minimal intermediate objects
* no unnecessary copies or allocations

It must correctly handle:

* client masking
* fragmentation
* continuation frames
* text and binary frames
* ping/pong
* close
* payload limits
* protocol errors

Do not implement unnecessary WebSocket features such as compression or client mode.

## NT4 protocol

Implement NetworkTables 4.1 semantics over WebSocket.

Control messages use JSON.

Value messages use MessagePack.

The implementation must support:

* connection/handshake
* publish
* unpublish
* subscribe
* unsubscribe
* announce
* unannounce
* properties
* timestamps
* typed values
* cached/current values
* multiple simultaneous clients
* reconnects

## Hot path

Value updates should follow:

```text
Tarwyn value update
        ↓
NT4 MessagePack encode once
        ↓
shared immutable buffer
        ↓
fan out to subscribed clients
        ↓
batched WebSocket write
```

Do not serialize independently for every subscriber.

Avoid per-update heap allocations where practical.

## Performance

Use the current implementation's approximately **29 µs median** benchmark as the baseline.

Benchmark the complete path rather than only the WebSocket parser.

Measure:

* p50
* p99
* p99.9
* throughput
* CPU usage
* allocations

Initial target:

```text
NT4 median < 50 µs
```

Stretch target:

```text
~35–40 µs median
```

Tail latency and allocation behavior matter alongside median latency.

Initial NT4 end-to-end measurement is currently ~54 ms median; the 50 µs target
is the Task 12 optimization goal (server-side 100 ms poll batching is the
identified bottleneck).

## Testing

The WebSocket layer must be tested for RFC 6455 compliance.

The NT4 layer must be tested against real NT4 clients/tools.

At minimum test:

* handshake
* topic discovery
* publish/subscribe
* value updates
* cached values
* properties
* multiple clients
* reconnects
* malformed input
* slow subscribers
* high-frequency updates

## Design rule

Prefer the **simplest implementation that meets the compatibility and performance targets**.

Start with a high-performance WebSocket crate as the baseline. Move to an in-house implementation only when benchmarking demonstrates that it provides a meaningful advantage or enables important NT4-specific optimizations.

The final system should optimize for **real end-to-end NT4 performance**, not theoretical WebSocket microbenchmarks.
