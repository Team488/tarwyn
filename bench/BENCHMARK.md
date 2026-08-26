# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

    cargo build --release --workspace
    bench/generate.sh

Results are written to [RESULTS.md](RESULTS.md), and the headline table is
reproduced in the root [README.md](../README.md).

The Java subjects need jars — WPILib, Jackson and the TARWYN release — which
Gradle resolves. `generate.sh` runs `./gradlew benchEnv` when it needs them, so
there is nothing to fetch by hand. Without a JDK the Java subjects are skipped
and the Rust ones still run.

## Subjects

| | |
|---|---|
| `tarwyn-rust` | the UDP telemetry plane, the fastest supported path |
| `tarwyn` | the original Java TARWYN v5.0.0, the incumbent |
| `nt4` | NetworkTables 4, tuned for latency — see `Nt4Subject` for the options |
| `tarwyn-zmq` | the ZeroMQ path the put/get API still uses |
| `udp-floor` | raw UDP, the floor nothing layered on a datagram can beat |

Default is `tarwyn-rust tarwyn nt4`.

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
| `PIN` | `0` to disable core pinning |
| `ONLY_REPORT` | `1` to rebuild the tables from the last run |

    SUBJECTS="tarwyn-rust tarwyn-zmq udp-floor" RATE=1000 bench/generate.sh

Keep the rate below saturation. At 2000 Hz every subject queues and repeated
runs vary by more than 2x, which measures the queue rather than the transport.

The `tarwyn` subject is occasionally flaky: its subscriber can register without
receiving anything, and the run then times out and reports no row. Rerunning that
subject alone usually produces one. The cause has not been isolated.

## Attributing a change

`compare-builds.sh` measures two server builds against each other, alternating
between them so drift lands on both rather than on one:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    REPS=5 bench/compare-builds.sh /tmp/before /tmp/after

Two identical binaries measured this way still differ by a few percent, so treat
anything smaller than that as unproven. A change that looked like a win against a
number recorded earlier on a quieter machine turned out to be noise once measured
this way, which is why the script exists.
