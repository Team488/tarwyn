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
bytes. That is 144 runs and about 100 minutes, while `--rates 500 --payloads
96` finishes in ten.

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

## Other commands

    bench soak --duration 3600 --window 60          # one pair for an hour; fails on 25% drift
    bench compare /tmp/before /tmp/after --reps 5   # two server builds, alternating
    bench run --case publish --impl tarwyn --role subscriber   # one side of one case
