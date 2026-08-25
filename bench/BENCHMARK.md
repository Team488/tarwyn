# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

    cargo build --release --workspace
    bench/generate.sh

Results are written into the table in the root [README.md](../README.md)
between the `BENCHMARK TABLE` markers. Nothing else is generated.

## Subjects

| | |
|---|---|
| `tarwyn-rust` | the UDP telemetry plane, the fastest supported path |
| `tarwyn-zmq` | the ZeroMQ path the put/get API still uses |
| `udp-floor` | raw UDP, the floor nothing layered on a datagram can beat |
| `zmq-direct` | one hop of ZeroMQ, no broker — separates ZeroMQ's cost from the relay's |

Default is `tarwyn-rust tarwyn-zmq udp-floor`.

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
| `PIN` | `0` to disable core pinning |
| `ONLY_REPORT` | `1` to rebuild the table from the last run |

    SUBJECTS="tarwyn-rust zmq-direct udp-floor" RATE=1000 bench/generate.sh

Keep the rate below saturation. At 2000 Hz every subject queues and repeated
runs vary by more than 2x, which measures the queue rather than the transport.

## A/B a change

`ab.sh` alternates two builds of the server and measures each under matched
conditions, which is the only way to attribute a difference on a machine this
noisy:

    REPS=3 bench/ab.sh A B
