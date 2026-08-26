# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

    cargo build --release --workspace
    bench/generate.sh

Results land in [RESULTS.md](RESULTS.md); the headline table is copied into the
root [README.md](../README.md).

`generate.sh` runs `./gradlew benchEnv` to resolve the Java subjects' jars —
WPILib, Jackson, the TARWYN release. Without a JDK those subjects are skipped
and the Rust ones still run.

## Subjects

| | |
|---|---|
| `tarwyn-rust` | the UDP telemetry plane, the fastest supported path |
| `tarwyn` | the original Java TARWYN v5.0.0, the incumbent |
| `ntcore` | NetworkTables 4, tuned for latency — see `NtcoreSubject` for the options |
| `tarwyn-zmq` | the ZeroMQ path the put/get API still uses |
| `udp-floor` | raw UDP, the floor nothing layered on a datagram can beat |

Default is `tarwyn-rust tarwyn ntcore`.

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

    SUBJECTS="tarwyn-rust tarwyn-zmq udp-floor" RATE=1000 bench/generate.sh

A cold pass then reruns TARWYN with the warmup discard off, recording what a
freshly started JVM delivers at boot. Only TARWYN gets one — ntcore is not
JIT-bound and Rust has no JIT; both came back within noise at a matched sample
count.

Keep the rate below saturation. At 2000 Hz every subject queues and repeated
runs vary by more than 2x, which measures the queue rather than the transport.

The `tarwyn` subject is flaky: its subscriber sometimes registers without
receiving anything, and the run times out with no row. Rerunning it alone
usually produces one. Cause not isolated.

## Attributing a change

`compare-builds.sh` measures two server builds against each other, alternating
between them so drift lands on both rather than on one:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    REPS=5 bench/compare-builds.sh /tmp/before /tmp/after

Two identical binaries still differ by a few percent this way, so treat anything
smaller as unproven.
