# Running the benchmark

Measures one-way latency between a publisher and a subscriber in separate
processes on one host, both reading `CLOCK_REALTIME`.

    cargo build --release --workspace
    ./target/release/bench sweep

Results land in [RESULTS.md](RESULTS.md). The harness is one binary: it starts
each case's server and probes, waits for ports, times them out, reduces the
samples and writes the report. `ntcore` runs through `uv`, so no JDK is needed.

## Cases

A case is an operation that more than one implementation can run; a row nobody
contests compares nothing. `bench list-cases` prints the catalog.

| Case | Group | Implementations |
|---|---|---|
| `publish` | servers | `tarwyn`, `tarwyn-busy`, `ntcore` |
| `publish_client` | clients (rendered as `publish`) | `tarwyn`, `tarwyn-busy`, `ntcore` |

- `publish` drives each server with this repo's raw NT4 probe, so it measures
  the server alone.
- `publish_client` publishes through each project's own client library
  (pyntcore, this repo's `Client`): the path a robot's code takes.
- `tarwyn-busy` is the same server with `--busy-poll` covering two publish
  intervals; see "What the server can and cannot shave".
- `ntcore` runs through `pyntcore`, tuned for latency (`send_all`,
  `keep_duplicates`, `periodic(0.001)`, `flush()` per set), as its own server
  process with the probes as clients. Stock settings sweep every 100 ms and
  would measure the defaults, not NetworkTables.

Adding an implementation is a line in `bench/src/catalog.rs` and one in
`run::plan`. `bench run --case <name> --impl <name> --role <role>` runs one side
of one case; `bench report` rebuilds the tables from rows already on disk.

## One harness measures

The Python probe computes nothing: it prints one
`S<TAB>sequence<TAB>due<TAB>received` line per sample, and `bench row` reduces
those through the same histogram the Rust probes use, so every row in the
report is the same arithmetic. Every row is one fixed line with one emitter:

    ROW  case  impl  version  payload  p50  p0  p80  p90  p95  p99  p99.9  p100  loss  samples  achieved_hz

`bench report` refuses any other width, and any row without an implementation
version.

## Every sample is due-stamped

Samples carry the time a send was *due*, not when it happened, and the
schedule never slips, so a stalled transport lands in the percentiles instead
of deleting the samples that would show it. That is the coordinated-omission
correction; applying another would count the delay twice.

The due time is read from the monotonic schedule with both clocks read
together. NTP disciplines the wall clock and not the monotonic one, so a
wall-clock counter kept in step with the schedule drifts by tens of
microseconds over a run (about 4 ppm on the machine in RESULTS.md), enough to
make samples read as negative. A sample received before it was due is refused,
since it can only mean the two clocks disagree.

Sends are paced. Back-to-back sends run with the caches hot and the core awake
and would read far faster for reasons unrelated to the operation measured.

## Results are one record

`bench sweep` writes `target/bench/results.json` and generates `RESULTS.md`
from it; a wrong number is fixed in the JSON.

## Options

Every setting is a flag on `bench sweep`; `--help` prints them with their
defaults.

| | |
|---|---|
| `--cases` | which cases to run, space separated by case name; all of them when unset |
| `--payloads` | wire sizes in bytes, default `16 96` |
| `--rate` | publish rate in Hz, default `500` |
| `--samples` | recorded per row, default `3000` |
| `--warmup` | received and discarded first, default `500` |
| `--count` | messages published, default `12000` |
| `--reps` | runs per row, default `3` |
| `--limit` | seconds before a probe is killed, default `90` |
| `--sub-settle` | seconds after a subscriber says it is ready, default `5` |
| `--no-pin` | do not pin each process to a physical core |
| `--only-report` | rebuild the tables from the rows a previous run left on disk |

    bench sweep --cases publish --rate 1000

## Reading a number

Rows are interleaved and each runs `--reps` times; a cell is the median run,
with the lowest and highest run, the p99 and the loss. The verdict names the
winner and the ratio, or `within noise` when the two best run-to-run ranges
overlap; a second clause judges the p99 on its own, as a plain ratio. The
spread table gives the wobble in microseconds beside the percentage: the same
few microseconds of noise are a larger percentage of a smaller median.

Runs short of `--samples` are dropped, a row that reports nothing is retried
once, and a row that received under nine tenths of its paced rate fails the
report after writing it. The report opens with the testbed, since a latency
figure only compares with one from the same machine.

Pinning gives the publisher and subscriber one physical core each (the fastest
ones, by `lscpu`'s maximum clock, on a part with two kinds of core) and the
server the rest, skipping core 0. The harness warns about the governor, boost
and load average, which account for most of the spread.

## What the `publish_client` case is for

The difference between the `publish` and `publish_client` rows is the client
library. It is why the client writes on the publishing thread: a client that
queues the frame for its reader thread waits on that thread's read timeout,
which the kernel rounds up to a millisecond. Measured that way a value took a
median of 54 ms to leave the process; written on the publishing thread, 34 µs
(33–37 over five runs, against 79–502 for the queue).

## Why the rate changes the number

Latency falls as the rate rises, and not from queueing: an idle core drops into
a deep C-state between messages, and leaving it costs more than everything the
server does with a value. Same build, payload and machine:

| Rate | Median | P99 |
|---|---|---|
| 500 Hz | 40.96 us | 1234.94 us |
| 2 kHz | 26.40 us | 1384.45 us |
| 10 kHz | 21.41 us | 1557.50 us |
| 40 kHz | 21.92 us | 178.18 us |

That machine's C2 costs 18 µs to exit, so roughly 19 µs of the 500 Hz median
is idle exit; compare like with like. `/dev/cpu_dma_latency` or disabling deep
C-states takes most of it back without a code change. Systemcore (a Pi CM5)
idles in WFI at about 1 µs, so on the robot the high-rate rows are the better
proxy, and only the scheduler's part of a wakeup remains, which is what
predictive polling removes. Keep the rate below the point where a probe
queues; reps that vary by 2x measure the queue.

## What the server can and cannot shave

Warm, the server's own work on a value is under a microsecond; the rest is the
kernel. One 500 Hz run on a laptop (Ryzen AI 7 350, `powersave`) split into
legs: 43 µs from due to the server's `read` returning, 15 µs to its `write`
returning, 33 µs to the subscriber. Of those, about 30 µs is the reader thread
being woken and 10 µs a cold core running kernel code. The wakeup is the only
term the server controls, and it has two answers:

- **Predict** (default, `--predict 200`): once two gaps agree on a period, the
  reader sleeps until 200 µs before the next message is due and spins until
  200 µs after. 500 Hz: 96 → 69 µs for 6.8% of a core (blocking: 2.2%); 50 Hz:
  0.9% vs 0.6%. An aperiodic stream blocks as before; the margin is clamped to
  a quarter of the period. The `tarwyn` row measures this.
- **Busy-poll** (`--busy-poll <MICROS>`, off): the reader spins for that long
  after every message. 500 Hz: 89 → 49 µs for a whole core, and unmoved at
  44–48 µs when a deeper power state put ntcore and blocking at 170–260 µs.
  Measured as `tarwyn-busy`.

The client has both (`Config::predict`, `Config::busy_poll`). Busy-polling both
sides: 19 µs median, 30 µs p99, against 85 and 578 blocking. What remains is
two loopback hops; allocation and lock trims in the fan-out path measured
under a microsecond, inside the spread.

## Soaking

`bench soak` runs one pair against this repo's server for an hour and reports
latency per window, since a stream that queues looks fine for the first
thousand samples and worse forever after.

    bench soak --duration 3600 --window 60

It fails if the median or p95 of the last quarter of windows grew more than 25%
over the first, and samples server RSS alongside. Flags: `--duration` (3600 s),
`--window` (60 s), `--rate` (500 Hz), `--payload` (96 B).

## Attributing a change

`bench compare` alternates between two server builds so drift lands on both:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    bench compare /tmp/before /tmp/after --reps 5

Two identical binaries still differ by a few percent; anything smaller than the
spread is unproven.
