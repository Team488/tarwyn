# Benchmark suite: operations, a catalog, and a machine-readable record

## Why

The suite measures one thing: a value published on one process arriving at a
subscriber on another. That is the transport's headline number and it is worth
keeping, but it is a small part of what the client offers. `get`,
`compare_and_set`, `delete`, `tables`, `ping` and `statistics` all block the
caller on a reply and none of them are measured, so nobody can answer what a
`get` costs a robot without writing a program.

The structure resists adding them. A subject lives in three places at once: a
Rust module under `bench/src/subjects/`, an arm of the `Subject` enum in
`main.rs`, and a shell function plus a dispatch line in `generate.sh`. Miss the
third and the subject silently measures nothing: the `client` subject was added,
built, and run against a full benchmark before anyone noticed its row was absent
from the results, because the shell never learned to call it.

This redesign makes an operation the unit of measurement, declares each one in a
single place, and writes the run out in a form a program can read.

## Scope

In scope:

- A case catalog: every benchmark case declared once, in Rust.
- Operation-level cases across both timing modes, described below.
- One entry point, `bench run --case <name> --impl <name>`, with the shell
  reduced to orchestration it cannot avoid (starting servers, pinning, settling).
- A normalized `results.json` per run, and `RESULTS.md` generated from it.
- A run-conditions block recording the machine and the versions measured.

Out of scope, deliberately:

- Round-trip cases for the Java TARWYN harness. The catalog can express them
  and the cells stay blank until someone writes them; doing it now means writing
  a second language's harness for cells most readers reach last.
- A dashboard repository or CI publishing. This suite measures network latency
  on one quiet desktop, where run-to-run spread is already 8 to 20 per cent. A
  shared CI runner would be worse, and publishing those numbers on a schedule
  would lend them an authority the measurement cannot support.
- Any change to what the server or client does. This is measurement only.

## Cases

A case declares five things:

| Field | Meaning |
|---|---|
| `name` | `publish`, `get`, `compare_and_set`, `subscribe_first`, ... |
| `group` | which table it appears in: delivery, round trip, best effort |
| `implementations` | which of `tarwyn-rust`, `ntcore`, `tarwyn` can run it |
| `mode` | `Delivery` or `RoundTrip`, defined below |
| `params` | payload sizes and publish rate |

The initial catalog:

| Case | Group | Mode | tarwyn-rust | ntcore | tarwyn |
|---|---|---|---|---|---|
| `publish` | delivery | Delivery | yes | yes | yes |
| `subscribe_first` | delivery | Delivery | yes | yes | no |
| `telemetry_publish` | best effort | Delivery | yes | no | no |
| `udp_floor` | best effort | Delivery | reference | no | no |
| `get` | round trip | RoundTrip | yes | no | later |
| `compare_and_set` | round trip | RoundTrip | yes | no | later |
| `delete` | round trip | RoundTrip | yes | no | later |
| `tables` | round trip | RoundTrip | yes | no | later |
| `ping` | round trip | RoundTrip | yes | no | later |

`ntcore` has no request/reply plane: an NT4 client reads a local cache, so a
`get` there measures a hash lookup and belongs in no table with ours. Those
cells stay blank rather than carrying a number that would flatter us.

## Timing modes

The two modes exist because the operations differ in what returning means.

`subscribe_first` is the interval between issuing a subscription and the first
value reaching the callback, against a topic already being published to. It
measures what a dashboard waits for when it attaches, which is a different
question from steady-state publish latency and the one users notice on connect.

**Delivery.** `publish` and `subscribe_first` return immediately; timing the call
measures the queueing, not the transport. The number is the interval between the
time the publisher was due to send, stamped into the payload, and the time the
subscriber decoded it. This is what the suite does today, and it keeps the
existing pacer, recorder, coordinated-omission correction and loss accounting.
Three processes: server, publisher, subscriber.

**RoundTrip.** `get` and its relatives block the caller until the server answers,
so the honest number is the call's own wall time, measured in the calling
process, which is also what the caller experiences. Two processes: server and
caller. Loss is not meaningful; a failed call is an error, not a dropped sample.

Both modes are paced at the same rate. This is not a detail: the same code
measures 41 us at 500 Hz and 22 us at 40 kHz, because cores idle between
messages and pay to wake. Round-trip calls issued back to back would run warm
and read faster than delivery rows for reasons that have nothing to do with the
operation, so every case is paced identically and the rate is recorded.

## Layout

```
bench/src/
  main.rs        run, list-cases
  catalog.rs     every case declared once
  harness.rs     pacer, recorder, send stats (unchanged)
  cases/
    publish.rs   delivery cases
    read.rs      round-trip cases
    subscribe.rs first-value latency
  report.rs      results.json and RESULTS.md
```

`bench list-cases` prints the catalog as tab-separated fields; `generate.sh`
loops over that rather than naming subjects itself. Adding a case becomes an
edit to `catalog.rs` and one file under `cases/`, and the shell needs no change,
which removes the failure that motivated this work.

Orchestration the shell keeps: starting and stopping servers, core pinning,
settling between runs, retries, and driving the Java and Python harnesses.

## Results

`RESULTS.md` has three parts.

A **conditions block**, new, recording machine, kernel, governor and boost state,
load average at start, the measurement parameters, and the version of every
implementation measured. A run that cannot say which binary it measured is a run
that can mislead, which has already happened once: a stale server held the port
and three benchmarks measured it without a word in the output.

A **matrix per group**: operations down, implementations across, medians in the
cells, `-` where the operation does not exist. This is the part meant to be read.

A **detail table per case**: the full percentile spread, loss, and sample count
as the suite reports today. This is the part meant to be argued with.

The spread and coordinated-omission tables stay as appendices.

`results.json` carries one flat record per case, implementation and payload:

```json
{"schema": 1,
 "generated": "2026-09-07T00:00:00Z",
 "conditions": {"kernel": "7.2.3-arch1-2", "cpu": "AMD Ryzen 5 5600X",
                "governor": "powersave", "epp": "balance_performance",
                "boost": true, "loadavg": 0.54,
                "rate_hz": 500, "samples": 3000, "warmup": 500, "reps": 3},
 "implementations": {"tarwyn-rust": "0.1.0", "ntcore": "2027.0.0a6.post4"},
 "cases": [{"case": "publish", "group": "delivery", "mode": "delivery",
            "impl": "tarwyn-rust", "payload_bytes": 96,
            "runs": 3, "samples": 3000, "median_us": 34.1, "p99_us": 132.4,
            "max_us": 2852.9, "loss_pct": 0.0, "spread_pct": 4.2,
            "achieved_hz": 500.0, "corrected_median_us": 34.1}]}
```

Flat records, so comparing two runs is a filter and a subtraction rather than a
walk over nested groups.

## Migration

The existing subjects become cases without changing what they measure, so the
numbers stay comparable across the change:

| Today | Becomes |
|---|---|
| `tarwyn-rust` | `publish`, implementation `tarwyn-rust` |
| `client` | `publish`, driven through the client library |
| `ntcore-server` | `publish`, implementation `ntcore` |
| `ntcore` | `publish`, through pyntcore |
| `tarwyn` | `publish`, through the Java client |
| `telemetry` | `telemetry_publish` |
| `udp-floor` | `udp_floor` |

The distinction the current file draws between a server driven raw and one
driven through its library becomes a property of the case row rather than a
separate section, since the catalog can say which client drove it.

## Testing

The harness is measurement code, so its tests are about not lying:

- The catalog round-trips: every case `list-cases` prints can be run by name,
  which is what stops a case existing in code but never being invoked.
- A round-trip case measures the call and not the pacing: with a stub server
  answering after a known delay, the reported median tracks that delay.
- The report writer emits one JSON record per case, implementation and payload,
  and the markdown matrix contains a cell for each.
- The existing pacer and recorder tests stay as they are.

Verification that the redesign changed no numbers: run the current suite, run
the new one on the same machine, and confirm the `publish` rows agree within the
run-to-run spread the file already reports.
