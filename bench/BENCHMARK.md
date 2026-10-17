# Benchmarks

This measures one-way latency from a publisher to a subscriber, running as
two processes on one machine.

    cargo build --release --workspace
    ./target/release/bench sweep

The sweep writes [RESULTS.md](RESULTS.md) and [results.json](results.json),
which has every percentile, and both are committed. It starts each server and
probe itself, running `ntcore` through `uv`.

## Cases

| Case | Measures |
|---|---|
| `publish` | the server alone, with raw NT4 probes on both sides |
| `publish_client` | a value leaving robot code: the client library publishes, a raw probe receives |
| `subscribe_client` | a value arriving in robot code: a raw probe publishes, the client library receives |
| `fanout` | `subscribe_client` with three subscribers, scored on the slowest one |

Both `tarwyn` and `ntcore` run with their defaults. The `ntcore` side is
`pyntcore` with `send_all`, `keep_duplicates`, `periodic(0.001)` and a
`flush()` after each set, because otherwise its 100 ms update cycle would be
the whole measurement. Adding an implementation takes one line in
`src/catalog.rs` and one in `src/run/plan.rs`.

## Reading a row

Each row runs three times at 50 and 500 Hz, with payloads of 16, 96 and 1024
bytes. That is 216 runs and about 150 minutes, while `--rates 500 --payloads
96` finishes in fifteen.

- Latency is one way, in microseconds. The receive time is stamped by the
  thread that decoded the value, and both ends read `CLOCK_REALTIME`.
- Each table shows the median run, the range of run medians, the p99 and the
  server's share of one core. A loss column appears only when a row lost
  samples. The line under a table names the faster implementation, or says
  `within noise` when the ranges overlap.
- Only compare rows at the same rate. At 50 Hz an idle core falls into a
  deeper sleep, so every row there is slower. Results from different machines
  do not compare at all.
- Each sample carries the time its send was *due*, so a stall still shows up
  in the percentiles. A row that receives less than 90% of its paced rate
  fails the report.

## Options

| | |
|---|---|
| `--cases` | case names, space separated in one argument; all of them when unset |
| `--rates` | publish rates in Hz, default `50 500` |
| `--payloads` | wire sizes in bytes, default `16 96 1024` |
| `--reps` | runs per row, default 3 |
| `--no-pin` | leave processes unpinned |
| `--only-report` | rebuild the tables from rows already on disk |

`ntcore` is pyntcore 2027.0.0a7, the version `python/pyproject.toml` pins, and
`ntcore-2026` is pyntcore 2026.2.2, which `uv` fetches on its own.

## bench sim

`bench sim` replays a whole robot's NetworkTables traffic instead of one
topic. A profile in [workloads/](workloads/) lists every client: the robot,
its coprocessors and its dashboards, each with the topics it publishes and the
subscriptions it makes. `team488-2026.json` was captured from Team 488's 2026
robot running autonomous in simulation. 488 runs a 25 Hz loop, so the profile
scales every robot topic to WPILib's standard 50 Hz loop, and its cameras are
set to 60 fps.

    cargo build --release --workspace
    ./target/release/bench sim

Each step runs the profile against one implementation as separate processes.

| Client | Against tarwyn | Against ntcore |
|---|---|---|
| robot | `tarwyn_client` beside the `tarwyn_server` daemon | pyntcore running the server and the robot's publishers in one process |
| coprocessors | one `tarwyn_client` per camera | one pyntcore client per camera, flushed every frame |
| dashboards | raw NT4 connections, the same for every server | the same |

The ntcore robot publishes the way AdvantageKit does, sending every value, and
flushes only to local listeners each loop, as WPILib's robot loop does. Values
that can hold one carry the same sequence and due-time header as the latency
probes, so receivers measure how late they arrive. Numbers and booleans have
no room for it and only add load.

The sweep has two axes.

| Axis | Steps |
|---|---|
| `rate` | every topic's rate times 1, 2, 4, 8 and so on, until a step breaks or `--max-multiplier` |
| `clients` | 0, 2, 4, 8 and 16 extra copies of the profile's spare dashboard, at 1x |

The rate axis runs once per `--budgets` entry. `none` gives the server and the
robot the server cores, `1core` pins both to one cpu, and `25pct` also caps
that cpu at a quarter of its time through `systemd-run --user`. A budget is a
share of this machine. It does not emulate a roboRIO or a SystemCore.

### Reading a row

- Each class column shows the median of the receivers' p50 and the p99 of the
  receiver closest to its allowance, as `p50 / p99`. `camera` is the
  coprocessors' frames, `struct` the topics at 10 Hz or more, and `slow` the
  rest.
- Loss counts gaps in the sequence on subscriptions that ask for every value.
  Stale is the p99 age of values on subscriptions that take only the latest.
- Connect is how long a fresh subscribe-all connection took to see every
  topic announced, then a value for each.
- CPU and RSS are the server's and the robot's together, which is what the
  robot controller pays. For ntcore they include the Python interpreter
  running the robot.
- A step breaks when a class is delivered under 95%, a class's p99 goes past
  one period of its fastest topic plus the `periodic` the receiver asked for,
  a process exits, or tarwyn drops publishes. A publisher that sent under 95%
  of its rate makes the step `generator-limited` instead, which counts against
  no server.
- The headroom table gives the last multiplier where most reps passed, and
  what stopped the next one.

A new profile comes from `bench/python/src/capture.py` run against a robot,
then `bench profile from-capture`, which can also read an Elastic layout and
an AdvantageScope layout for the dashboards' subscriptions, and with
`--loop-hz` and `--captured-loop-hz` runs the robot at another loop rate.

## Other commands

    bench soak --duration 3600 --window 60          # one pair for an hour; fails on 25% drift
    bench compare /tmp/before /tmp/after --reps 5   # two server builds, alternating
    bench compare /tmp/before /tmp/after --profile bench/workloads/team488-2026.json --multiplier 4
    bench run --case publish --impl tarwyn --role subscriber   # one side of one case
