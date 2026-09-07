# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).
500 Hz, 3000 samples per subject with 500 warmup discarded.
Every subject ran 3 times, subjects interleaved; each row is that
subject's median run, picked by its median column.

## 16 byte payload

### Client libraries

What a robot's own code gets. Every row publishes through its project's
client library, which for `ntcore` and `tarwyn` is the only way to
speak their protocols at all, so this is the comparison that decides
anything. Reliable ordered streams throughout, each tuned for latency.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|tarwyn-rust client v0.1.0|34.05|20.59|42.21|48.06|51.58|132.35|2357.25|2852.86|0.00|
|ntcore v2027.0.0a6.post4|49.45|33.89|60.25|65.58|69.75|861.00|2867.29|3097.70|0.00|
|tarwyn v5.0.0|104.64|69.72|462.69|1173.90|1807.08|3548.54|5535.32|7524.71|1.45|

### Transport, no library

The same server driven straight onto a socket, with no client library in
the way. Only this repo can produce such a row, so it is a reference for
what the server costs on its own rather than a competitor to the table
above: the gap between the two is what our client adds.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|33.95|19.63|43.36|47.23|57.82|93.57|2089.98|2095.10|0.00|

### Best effort, datagram

Not comparable with the table above: nothing here is retransmitted, ordered
or acknowledged, so read the loss column alongside the latency.
`udp-floor` has no server in it at all and is the floor, not a subject.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|udp-floor|9.29|7.02|12.20|13.19|14.93|19.79|960.00|1342.46|0.00|
|tarwyn-rust telemetry v0.1.0|22.18|15.37|27.21|29.52|31.63|100.73|1725.44|2725.89|0.00|

## 96 byte payload

### Client libraries

What a robot's own code gets. Every row publishes through its project's
client library, which for `ntcore` and `tarwyn` is the only way to
speak their protocols at all, so this is the comparison that decides
anything. Reliable ordered streams throughout, each tuned for latency.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|tarwyn-rust client v0.1.0|34.56|18.59|43.52|48.22|52.64|144.00|2076.67|2908.16|0.00|
|ntcore v2027.0.0a6.post4|51.98|32.99|64.01|70.62|77.90|526.44|2155.04|2336.86|0.00|
|tarwyn v5.0.0|105.21|69.72|481.30|1015.68|1799.61|3165.32|5319.11|6117.82|1.41|

### Transport, no library

The same server driven straight onto a socket, with no client library in
the way. Only this repo can produce such a row, so it is a reference for
what the server costs on its own rather than a competitor to the table
above: the gap between the two is what our client adds.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|36.22|19.62|44.41|48.99|51.97|83.78|1849.34|1857.54|0.00|

### Best effort, datagram

Not comparable with the table above: nothing here is retransmitted, ordered
or acknowledged, so read the loss column alongside the latency.
`udp-floor` has no server in it at all and is the floor, not a subject.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|udp-floor|10.26|6.93|13.12|14.57|15.43|21.55|970.24|1310.72|0.00|
|tarwyn-rust telemetry v0.1.0|21.30|14.73|27.18|29.54|32.22|70.33|1342.46|2834.43|0.00|

## Coordinated omission check

Corrected columns refill the samples a stall swallowed, assuming the
500 Hz send schedule. A corrected figure far above the raw one means the
run hit stalls the raw percentiles cannot show. Subjects whose harness does
not report this are left out.

|Subject|Payload (B)|Median|Corrected median|P99|Corrected P99|Achieved (Hz)|
|---|---|---|---|---|---|---|
|udp-floor|16|9.29|9.29|19.79|19.79|500.0|
|tarwyn-rust client v0.1.0|16|34.05|0.00|132.35|0.00|500.0|
|tarwyn-rust telemetry v0.1.0|16|22.18|22.18|100.73|100.73|500.0|
|tarwyn-rust v0.1.0|16|33.95|33.95|93.57|93.57|500.0|
|udp-floor|96|10.26|10.26|21.55|21.55|500.0|
|tarwyn-rust client v0.1.0|96|34.56|0.00|144.00|0.00|500.0|
|tarwyn-rust telemetry v0.1.0|96|21.30|21.30|70.33|70.33|500.0|
|tarwyn-rust v0.1.0|96|36.22|36.22|83.78|83.78|500.0|

## Run-to-run spread

How far the median moved across runs of the same subject. A change smaller
than the spread here is noise, not a result.

|Subject|Payload (B)|Runs|Lowest median|Highest median|Spread (%)|
|---|---|---|---|---|---|
|ntcore v2027.0.0a6.post4|16|3|48.82|53.76|10.1|
|udp-floor|16|3|9.09|9.76|7.4|
|tarwyn-rust client v0.1.0|16|3|33.34|34.40|3.2|
|tarwyn-rust telemetry v0.1.0|16|3|21.82|22.73|4.2|
|tarwyn-rust v0.1.0|16|3|33.66|39.39|17.0|
|tarwyn v5.0.0|16|2|104.64|106.92|2.2|
|ntcore v2027.0.0a6.post4|96|3|49.45|52.28|5.7|
|udp-floor|96|3|10.13|10.85|7.1|
|tarwyn-rust client v0.1.0|96|3|33.89|35.13|3.7|
|tarwyn-rust telemetry v0.1.0|96|3|20.40|22.51|10.3|
|tarwyn-rust v0.1.0|96|3|31.52|37.73|19.7|
|tarwyn v5.0.0|96|3|99.70|105.54|5.9|
