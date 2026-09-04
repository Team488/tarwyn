# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

    cargo build --release --workspace
    bench/generate.sh

Results land in [RESULTS.md](RESULTS.md); the headline table is copied into the
root [README.md](../README.md).

`generate.sh` runs `./gradlew benchEnv` to resolve the TARWYN release jar and
what it depends on. Without a JDK the `tarwyn` subject is skipped and the rest
still run.

## Subjects

| | |
|---|---|
| `tarwyn-rust` | the WebSocket path this repo serves, publish through announce to fan-out |
| `tarwyn` | the original Java TARWYN v5.0.0 over ZeroMQ, the incumbent this replaces |
| `ntcore` | WPILib's own NetworkTables, run as both server and client |
| `udp-floor` | raw UDP with no server in between, the floor nothing layered on a datagram can beat |

Default is `tarwyn-rust tarwyn ntcore`.

`ntcore` runs through `pyntcore` (`bench/python/ntcore_subject.py`) tuned for
latency: `sendAll(True)`, `keepDuplicates(True)`, `periodic(0.001)`,
`pollStorage(1000)`, `flush()` after every set, read via `readQueue()`. It
needs no JDK; `tarwyn` does, since TARWYN v5.0.0 ships only a Java server.

## Options

| | |
|---|---|
| `SUBJECTS` | which to run, space separated |
| `PAYLOADS` | wire sizes in bytes, default `16 96` |
| `RATE` | publish rate in Hz, default `500` |
| `SAMPLES` | recorded per subject, default `3000` |
| `WARMUP` | received and discarded first, default `500` |
| `COUNT` | messages published, default `12000` |
| `LIMIT` | seconds before a subject is killed, default `90` |
| `TARWYN_WARMUP` | seconds to let the TARWYN server settle, default `8` |
| `COLD` | `0` to skip the cold pass |
| `COLD_SAMPLES` | samples recorded cold, default `200` |
| `PIN` | `0` to disable core pinning |
| `ONLY_REPORT` | `1` to rebuild the tables from the last run |

    SUBJECTS="tarwyn-rust udp-floor" RATE=1000 bench/generate.sh

A cold pass then reruns TARWYN with the warmup discard off, recording what a
freshly started JVM delivers at boot. Only TARWYN gets one. Rust has no JIT,
and ntcore came back within noise at a matched sample count.

Keep the rate below saturation. At 2000 Hz every subject queues and repeated
runs vary by more than 2x, which measures the queue rather than the transport.

TARWYN drops messages, so its rows carry a real loss figure where the others
read `0.00`. Expect a percent or so at these rates, and much more if you push
the rate up.

## Soaking the telemetry plane

`soak.sh` runs one publisher and one subscriber for an hour and reports latency
per window, which answers whether latency grows with time. A stream that queues
looks fine for the first thousand samples and worse forever after.

    DURATION=3600 WINDOW=60 bench/soak.sh

It compares the first quarter of windows against the last and fails if either
the median or the p95 grew by more than 25%. Server RSS is sampled alongside,
since a queue that costs latency usually costs memory too.

| | |
|---|---|
| `DURATION` | seconds to run, default `3600` |
| `WINDOW` | seconds per reported row, default `60` |
| `RATE` | publish rate in Hz, default `500` |
| `PAYLOAD` | wire size in bytes, default `96` |

## Attributing a change

`compare-builds.sh` measures two server builds against each other, alternating
between them so drift lands on both rather than on one:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    REPS=5 bench/compare-builds.sh /tmp/before /tmp/after

Two identical binaries still differ by a few percent this way, so treat anything
smaller as unproven.
