# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

Samples carry the time the publisher was *due* to send, not the time it managed
to, and the send schedule never slips: a publisher that falls behind catches up
instead of skipping the slots it missed. Both exist so a stalled transport shows
up in the percentiles rather than deleting the samples that would have shown it.

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
| `ntcore` | WPILib's own NetworkTables, a standalone server with a client at each end |
| `telemetry` | this repo's UDP telemetry plane on 5809, publisher through relay to subscriber |
| `client` | the same path as `tarwyn-rust`, published through the Rust client rather than onto a socket |
| `udp-floor` | raw UDP with no server in between, the floor nothing layered on a datagram can beat |

Default is `tarwyn-rust tarwyn ntcore`.

The subjects fall into three classes and the results file keeps them apart.

**Client libraries** is the comparison that decides anything: `client`, `ntcore`
and `tarwyn` all publish through their project's own library, which is what a
robot's code actually calls. For `ntcore` and `tarwyn` there was never another
option, since their protocols are only reachable through their stacks; the
`tarwyn` publisher is the Java `TarwynClient` and the `ntcore` one is pyntcore.
Our Java and Python clients belong in this table too when someone wires them up.

**Transport, no library** holds `tarwyn-rust`, the same server driven straight
onto a socket. Only this repo can produce such a row, so it is a reference rather
than a competitor: the gap between it and `client` is what our own library costs.
Keeping the two apart is what stopped a library problem from reading as a server
result, and a 54 ms publish path went unnoticed until the split existed.

**Best effort, datagram** is `telemetry` and `udp-floor`: nothing is
retransmitted or ordered, and a lost datagram stays lost, which is what buys the
latency. Reading them against either table above compares a delivery guarantee
with the absence of one.

`ntcore` is tuned for latency rather than run as shipped, which is the harder
comparison to win and the only fair one. Stock WPILib options sweep every 100 ms
and send only the newest value, so a 500 Hz publisher would lose most of what it
writes and the row would say more about the defaults than about NetworkTables.

`ntcore` runs through `pyntcore` (`bench/python/ntcore_subject.py`) tuned for
latency: `send_all(True)`, `keep_duplicates(True)`, `periodic(0.001)`,
`poll_storage(1000)`, `flush()` after every set, read via `read_queue()`. It
needs no JDK; `tarwyn` does, since TARWYN v5.0.0 ships only a Java server.

Its version is the `pyntcore` pin from `bindings/pyproject.toml`, so the
benchmark measures the same NetworkTables the client is built against. `PYNTCORE`
overrides it.

It runs as three processes, the same shape as `tarwyn-rust`: a server of its own
with the publisher and subscriber as clients either side. Hosting the server
inside the subscriber would measure one hop against everyone else's two.

## Options

| | |
|---|---|
| `SUBJECTS` | which to run, space separated |
| `PAYLOADS` | wire sizes in bytes, default `16 96` |
| `RATE` | publish rate in Hz, default `500` |
| `SAMPLES` | recorded per subject, default `3000` |
| `WARMUP` | received and discarded first, default `500` |
| `COUNT` | messages published, default `12000` |
| `REPS` | runs per subject, default `3` |
| `LIMIT` | seconds before a subject is killed, default `90` |
| `TARWYN_WARMUP` | seconds to let the TARWYN server settle, default `8` |
| `PIN` | `0` to disable core pinning |
| `ONLY_REPORT` | `1` to rebuild the tables from the last run |

    SUBJECTS="tarwyn-rust udp-floor" RATE=1000 bench/generate.sh

## Reading a number

Subjects are interleaved and each runs `REPS` times, so drift lands on all of
them rather than on whichever ran last. Each row is that subject's median run;
the spread table below it is how far that median moved between runs. A
difference smaller than the spread is noise.

Runs ending short of `SAMPLES` are dropped rather than averaged in, and a
subject that reports nothing is retried once.

The coordinated-omission table corrects the percentiles for stalls longer than
one send interval, and prints the achieved rate beside them. A corrected figure
far above the raw one, or a rate well under `RATE`, means the run hit stalls the
raw numbers cannot show. Publisher logs in the rows directory carry the other
half: time spent blocked inside `send` is the transport pushing back, while time
already lost before the call is this machine descheduling the publisher.

Pinning takes three distinct physical cores from `lscpu`, skipping core 0 and
its siblings. The harness also warns up front about the governor, boost and
load average, which account for most of the spread.

## What the client subject is for

Every other subject drives the wire directly, so they measure the server and say
nothing about the path a robot's own code takes to reach it. `client` publishes
through `TarwynClient` instead, and the difference is the library.

It found one. A published frame used to be handed to a queue and written by the
client's reader thread, which only reached that queue when its blocking read
returned: a publisher with nothing to receive waited a median of 54 ms for its
own value to leave the process, against 41 us for the same server driven by a
socket directly. Shortening the read did not fix it, because a socket read
timeout is rounded to the kernel's timer granularity and cannot go below a
millisecond. The client now writes on the publishing thread, which took the same
measurement to a median of 34 us over five runs, and steadied it: 33 to 37 us,
where the queue gave 79 to 502.

## Why the rate changes the number

Latency falls as the publish rate rises, and it is not a queueing effect. A core
with nothing to do drops into a deep idle state between messages, and coming back
out of it costs more than everything the server does with a value put together.
The same build, same payload, same machine:

| Rate | Median | P99 |
|---|---|---|
| 500 Hz | 40.96 us | 1234.94 us |
| 2 kHz | 26.40 us | 1384.45 us |
| 10 kHz | 21.41 us | 1557.50 us |
| 40 kHz | 21.92 us | 178.18 us |

`cpuidle` on the machine those came from reports C2 at 18 us exit latency and a
36 us residency target, so at 500 Hz, with 2 ms of quiet between messages, every
core on the path pays that on the way back. Roughly 19 us of the 500 Hz median is
this and nothing else, which is also why the tail collapses once traffic is dense
enough to keep the cores awake.

Two consequences worth keeping in mind. A number measured at one rate says
nothing about another rate, so compare like with like. And a deployment that
cares more about latency than power can take most of that 19 us back from the
outside, with `/dev/cpu_dma_latency` or by disabling the deep C-states, without
changing a line of this code.

This also decides which row predicts the robot. Systemcore is a Raspberry Pi
CM5, and Pi-class ARM parts idle in WFI, whose exit costs about a microsecond
rather than the eighteen an x86 C2 costs. The idle tax that dominates the 500 Hz
row here is therefore mostly absent on the target, and the high-rate rows are the
better proxy for it: they are what this code does once the wakeups are cheap.
It follows that work on the server's own path is worth more on the robot than
the 500 Hz row suggests, and that anything aimed at idle states is worth nothing
there.

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
since a queue that costs latency usually costs memory too. `BENCH_WINDOW_SECS`
drives the windowing and any subscriber honours it, reporting a `WINDOW` row
that often instead of one row at the end.

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

It pins and settles like `generate.sh` and prints each build's median run and
spread. Two identical binaries still differ by a few percent, so treat anything
smaller than the spread as unproven.
