# Running the benchmark

Measures one-way latency with the publisher and subscriber as separate
processes, both reading `CLOCK_REALTIME`. Same-host only.

Samples carry the time the publisher was *due* to send, not the time it managed
to, and the send schedule never slips: a publisher that falls behind catches up
instead of skipping the slots it missed. Both exist so a stalled transport shows
up in the percentiles rather than deleting the samples that would have shown it.

    cargo build --release --workspace
    ./target/release/bench sweep

Results land in [RESULTS.md](RESULTS.md), which the root
[README.md](../README.md) links to rather than copying, so there is one place a
number can be wrong.

The harness is one binary and no shell. It starts the servers and probes each
case needs, waits for their ports, times them out, reduces their samples and
writes the report, because every one of those is something it already had to do
and a launcher table written in another language is a second catalog that has
to agree with the first.

`bench sweep` needs no JDK: the `ntcore` server and its probe run through `uv`,
and the `tarwyn` subject is the workspace's own binaries.

## Cases

A case is an operation, not an implementation, and it earns a place only if
more than one implementation can run it: a row nobody contests says nothing
about how this project compares with the alternatives. Each case names the
implementations that can run it. `bench list-cases` prints the catalog straight
from the binary, as `name<TAB>group<TAB>implementations`, so it never drifts
from what `bench sweep` actually runs:

    ./target/release/bench list-cases

`bench run --case <name> --impl <name> --role <publisher|subscriber>` runs one
side of one case for one implementation; `bench sweep` spawns one per side and
collects their `ROW` lines. `bench report` turns a rows file into the record
and the report, which is also how to rebuild the tables from a run already on
disk.

Currently cataloged:

| Case | Group | Implementations |
|---|---|---|
| `publish` | servers | `tarwyn`, `ntcore` |
| `publish_client` | clients (rendered as `publish`) | `tarwyn`, `ntcore` |

`publish_client` is the comparison that decides anything: every implementation
publishes through its project's own library, which is what a robot's code
calls: pyntcore for `ntcore`, and this repo's client for `tarwyn`. It
renders as `publish` in the report; the case is named `publish_client` only so
it can sit in the catalog next to `publish`.

Adding an implementation is an edit to the catalog in `bench/src/catalog.rs`
and one line in `run::plan`, which says which server answers a case, which
probe touches it, the port it listens on and the seconds it needs to settle.

`publish` drives the same raw NT4 publisher and subscriber from this repo at
each server in turn, with no client library in the way, so it isolates what a
server costs on its own. That probe speaks NT4 over a WebSocket on 5810, which
is the protocol `ntcore` serves too, so the same publisher reaches both servers
unchanged.

`ntcore` is tuned for latency rather than run as shipped, which is the harder
comparison to win and the only fair one. Stock WPILib options sweep every 100 ms
and send only the newest value, so a 500 Hz publisher would lose most of what it
writes and the row would say more about the defaults than about NetworkTables.

`ntcore` runs through `pyntcore` (`bench/python/src/ntcore_probe.py`) tuned for
latency: `send_all(True)`, `keep_duplicates(True)`, `periodic(0.001)`,
`poll_storage(1000)`, `flush()` after every set, read via `read_queue()`.

Its version is the `pyntcore` pin from `bench/python/pyproject.toml`, so the
benchmark measures the same NetworkTables the client is built against.

It runs as three processes, the same shape as `tarwyn`: a server of its own
with the publisher and subscriber as clients either side. Hosting the server
inside the subscriber would measure one hop against everyone else's two.

## One harness measures, the others only move bytes

`ntcore` is driven from Python, because its protocol is only reachable from
that stack. The harness computes nothing: it connects, publishes a buffer,
receives a buffer, and prints one line per sample:

    S<TAB>sequence<TAB>due<TAB>received

`bench row --samples <file> --case <name> --impl <name> --payload <n> --version
<v>` reduces those lines to a `ROW` line through the same histogram the Rust
probes use, so warmup, loss, every percentile and the achieved rate are the
same arithmetic for every implementation in the report. Sample lines are held
in memory and printed after the run: printing inside the receive loop would
measure the print.

The `ROW` line is one fixed schema with one emitter, `Recorder::report`:

    ROW  case  impl  version  payload  p50  p0  p80  p90  p95  p99  p99.9  p100  loss  samples  achieved_hz

Every field is required. `bench report` refuses a row of any other width,
naming the line, and refuses a row with no implementation version for the same
reason: a number that cannot say what it measured cannot be compared with
anything.

## Every sample is due-stamped

Both harnesses stamp the time a send was **due**, so a send delayed by a
stalled transport charges the delay to the samples that waited it out and the
stall lands in the percentiles. A due-stamped histogram needs no
coordinated-omission correction on top; refilling it would count the same
delay twice.

The due time is derived from the send schedule's monotonic deadline with both
clocks read together, never by advancing a wall-clock counter alongside the
schedule. NTP disciplines the wall clock and leaves the monotonic one alone, so
a counter kept in step with the schedule drifts away from the clock the
subscriber stamps with — tens of microseconds over a 24-second run, measured at
about 4 ppm on the machine in RESULTS.md. That is enough to swamp a
sixty-microsecond latency, and by the end of a run it made samples read as
negative. A sample received before it was due is refused rather than clamped to
zero, because it can only mean the two clocks disagree, and then every latency
in that run is off by the same unknown amount.

Sends are paced rather than fired back to back. Messages sent back to back run
warm — the connection hot, the cache lines loaded, the
core already awake — and would read far faster than paced sends for reasons
that have nothing to do with the operation being measured; see "Why the rate
changes the number" below.

## Results are one record

`bench sweep`'s final step is always the same report call: it writes
`target/bench/results.json`, the record, and generates `bench/RESULTS.md` from
it. There is no path that edits one without the other, so the two files cannot
disagree — if a number in `RESULTS.md` looks wrong, the fix is in
`results.json`.

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

Rows are interleaved and each runs `--reps` times, so drift lands on all of
them rather than on whichever ran last. Each cell is that row's median run
across those reps, with the p99 and loss beside it. `results.json` also carries
`spread_pct` (how far the median moved between reps) and `achieved_hz` for
anything the markdown table doesn't show.

Runs ending short of `--samples` are dropped rather than averaged in, and a
row that reports nothing is retried once; a run left with zero records after
every retry fails rather than writing an empty report.

`RESULTS.md` has one matrix per group, and within a group one matrix per
payload size, so two payload sizes never collapse into a single unlabelled
cell. Each cell carries the median, the lowest and highest run behind it, the
p99 and the loss, and each row ends in a verdict: the implementation that won
and by how much, or `within noise` when the two best implementations'
run-to-run ranges overlap. A row marked that way did not measure a difference,
and the cell says so because a reader who stops at the table should reach the
same conclusion as one who reads the spread table underneath it.

The report opens with the testbed — operating system, kernel, CPU and commit
— because a latency figure is only comparable with another taken on the same
machine. `results.json` also records the governor, boost setting and load
average at the start of the run.

A run that came in below nine tenths of the rate it was paced at measured a
backlog rather than a transport, so `bench report` writes the report and then
fails, naming the rows. The files are written first so the row that failed can
be read.

Pinning reads the physical cores from `lscpu`, skips core 0 and its siblings,
gives the publisher and subscriber one core each and the server the rest. The
harness also warns up front about the governor, boost and load average, which
account for most of the spread.

## What the `publish_client` case is for

The `publish` case drives the wire directly, so it measures the server and says
nothing about the path a robot's own code takes to reach it. `publish_client`
publishes through this repo's `Client` instead, and the difference between the two
rows is the library.

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

Keep the rate below saturation. At 2000 Hz every probe queues and repeated
runs vary by more than 2x, which measures the queue rather than the transport.

## A process may only be pinned to one core if it sends on the calling thread

The publisher and subscriber probes each get a physical core of their own,
which is what keeps the run-to-run spread small, because their send is a
syscall on the thread that paced it.

The same rule applies to the servers, both of which are multi-threaded, so the
server gets every physical core the probes are not on: on a six-core machine,
probes on cores 1 and 2 and the server on 3 to 5, with core 0 and its siblings
left to the kernel throughout.

## Soaking

`bench soak` runs one publisher and one subscriber against this repo's server
for an hour and reports latency per window, which answers whether latency grows with time. A stream that queues
looks fine for the first thousand samples and worse forever after.

    bench soak --duration 3600 --window 60

It compares the first quarter of windows against the last and fails if either
the median or the p95 grew by more than 25%. Server RSS is sampled alongside,
since a queue that costs latency usually costs memory too. `--window` drives
the windowing, and the subscriber reports a `WINDOW` row that often instead of
one row at the end.

| | |
|---|---|
| `--duration` | seconds to run, default `3600` |
| `--window` | seconds per reported row, default `60` |
| `--rate` | publish rate in Hz, default `500` |
| `--payload` | wire size in bytes, default `96` |

## Attributing a change

`bench compare` measures two server builds against each other, alternating
between them so drift lands on both rather than on one:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    bench compare /tmp/before /tmp/after --reps 5

It pins and settles like `bench sweep` and prints each build's median run and
spread. Two identical binaries still differ by a few percent, so treat anything
smaller than the spread as unproven.
