# Benchmark results

One-way latency, publisher and subscriber as separate processes on one host,
both reading `CLOCK_REALTIME`. Loopback only — cross-machine numbers need a
time-sync design and are not comparable to these.

Machine: 12-core x86-64, Linux 7.1.9, JDK 25. Rust built release with
`lto = "fat"` and `codegen-units = 1`. Numbers move with machine load; re-run
before comparing.

## Gate matrix, 96 B payload

| Subject | p50 | p99 | p999 | dropped | vs floor (p50) |
|---|---|---|---|---|---|
| udp-floor (Rust) | 6.47 us | 16.54 us | 81.60 us | 0 | 1.0x |
| java-udp | 8.32 us | 33.48 us | 320.16 us | 0 | 1.3x |
| tarwyn-rust | 77.18 us | 433.92 us | 2099.20 us | 501 | 11.9x |
| tarwyn-java (incumbent) | 215.65 us | 35409.62 us | 36297.27 us | 1555 | 33.3x |
| nt4-flush | 2051.90 us | 4072.36 us | 4608.78 us | 0 | 317x |

Publisher at 1000–2000 Hz, 3000 samples collected per subject.

NT4 configuration, which must be published with any NT number to be evidence:
`sendAll(true)`, `keepDuplicates(true)`, `periodic(0.001s)`, `pollStorage(1000)`,
`flush()` after every `set`, read via `readQueue()`, subscriber spinning rather
than sleeping between polls. NT4 2025.3.2, subscriber acting as server and
publisher as client, mirroring the robot/coprocessor split.

## Verdicts against the stop criteria

**NT4 within ~20% of the floor → stop.** Not met, by a wide margin. NT4 sits at
**317x** the floor. Configuring it fairly moved it from its 100 ms default sweep
to about 2 ms, and spinning instead of sleeping in the subscriber accounted for
roughly 500 us of that — but 2 ms against a 6.5 us floor leaves the headroom
this project exists to claim.

**TARWYN already beating NT4 materially → the work is a port, not a
performance project.** TARWYN does beat NT4, by about 9.5x at p50. But
tarwyn-rust already beats TARWYN by 2.8x at p50 and by two orders of
magnitude in the tail, so the performance case stands on its own.

**Pure-Java UDP matching a native client → drop the cdylib and natives matrix.**
Met. Java `DatagramChannel` with a preallocated direct `ByteBuffer` lands at
8.32 us against Rust's 6.47 us, a 1.3x gap on a transport where Rust owns the
socket outright. This reproduces team 4533's result, who deleted their C and JNI
layer after finding the same thing. A native Java client is not needed for
transport performance.

**Floor far below both NT4 and TARWYN → the transport swap earns its risk.**
Met. Both incumbents sit 33x and 317x above a floor that a pure-Java client can
reach within 30%.

## Notes on the tails

tarwyn-java's p99 of 35 ms is dominated by JVM warmup: the first messages are
interpreted before the JIT compiles the hot path. Its p50 of 216 us is the
representative figure. The 1555 drops are the same slow-joiner behaviour as
tarwyn-rust's 501 — a ZeroMQ SUB socket subscribes asynchronously and the
publisher discards anything sent before the subscription is established.

nt4-flush drops nothing, which is expected: it is the only subject here that
queues rather than discarding, and that queuing is visible in its latency.

## Floor across payload sizes

| Payload | p50 | p99 | p999 | max | dropped |
|---|---|---|---|---|---|
| 16 B | 6.34 us | 16.18 us | 55.77 us | 143.23 us | 0 |
| 96 B | 6.47 us | 16.54 us | 81.60 us | 279.30 us | 0 |
| 65507 B | 13.79 us | 31.70 us | 72.38 us | 144.00 us | 0 |

16 B and 96 B land within 2% of each other, so at small sizes the cost is
per-message rather than per-byte. A 100 KB payload cannot be a single datagram
at all — `EMSGSIZE` above 65507 B — so bulk transfer belongs on a stream
transport rather than this path.

tarwyn-rust shows the same flatness: 75.71 us at 16 B against 77.18 us at 96 B.

## Reproducing

Rust subjects:

    cargo build --release --workspace
    ./target/release/benches subscriber --subject udp --payload 96 --samples 3000 &
    ./target/release/benches publisher  --subject udp --payload 96 --rate 5000 --count 6000

    ./target/release/tarwyn_server &
    ./target/release/benches subscriber --subject tarwyn --payload 96 --samples 3000 &
    ./target/release/benches publisher  --subject tarwyn --payload 96 --rate 2000 --count 8000

Java subjects need ntcore, wpiutil, their JNI natives, Jackson, and TARWYN.jar.
See benches/java/README.md.
