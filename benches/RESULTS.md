# Benchmark results

One-way latency, publisher and subscriber as separate processes on one host,
both reading `CLOCK_REALTIME`. Loopback only — cross-machine numbers need a
time-sync design and are not comparable to these.

Machine: 12-core x86-64, Linux 7.1.9, release build (`lto = "fat"`,
`codegen-units = 1`). Numbers move with machine load; re-run before comparing.

## Floor

Raw UDP. Nothing layered on a datagram socket beats this, so it is the
reference every other subject is measured against.

| Payload | p50 | p99 | p999 | max | dropped |
|---|---|---|---|---|---|
| 16 B | 6.34 us | 16.18 us | 55.77 us | 143.23 us | 0 |
| 96 B | 6.47 us | 16.54 us | 81.60 us | 279.30 us | 0 |
| 65507 B | 13.79 us | 31.70 us | 72.38 us | 144.00 us | 0 |

A 100 KB payload cannot be a single datagram — `EMSGSIZE` above 65507 B. Bulk
transfer therefore belongs on a stream transport, not on this path.

16 B and 96 B land within 2% of each other, so at small sizes the cost is
per-message rather than per-byte.

## Subjects

Publisher at 2000 Hz, 3000 samples collected.

| Subject | Payload | p50 | p99 | p999 | max | dropped |
|---|---|---|---|---|---|---|
| udp-floor | 16 B | 6.34 us | 16.18 us | 55.77 us | 143.23 us | 0 |
| udp-floor | 96 B | 6.47 us | 16.54 us | 81.60 us | 279.30 us | 0 |
| tarwyn-rust | 16 B | 75.71 us | 613.89 us | 625.66 us | 627.71 us | 501 |
| tarwyn-rust | 96 B | 77.18 us | 433.92 us | 2099.20 us | 2508.80 us | 501 |

tarwyn-rust sits about 12x the floor at p50 and 26x at p99. Two hops rather
than one accounts for some of that — publisher to server to subscriber — but
not a factor of twelve.

The 501 dropped messages are identical across runs and payload sizes, which
points at ZeroMQ's slow-joiner behaviour rather than congestion: a SUB socket
subscribes asynchronously, so messages published before the subscription is
established are discarded by the publisher.

## Pending

- TARWYN (Java/JeroMQ) — the incumbent
- NetworkTables 4 with `flush()`
- Pure-Java UDP, to test whether a native client is needed at all
