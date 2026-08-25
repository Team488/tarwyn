# Benchmarks

Publisher and subscriber as separate processes on one host, both reading
`CLOCK_REALTIME`. Same-host only; cross-machine numbers need clock sync.

Every subject runs at the same rate with the same 16-byte header, pinned to separate
cores. The first 500 received messages are discarded so figures are steady state,
not JIT warmup. A subject that cannot keep up shows it under Loss rather than being
given an easier run.

NetworkTables is configured at its best, not its 100 ms default: `sendAll(true)`,
`keepDuplicates(true)`, `periodic(0.001s)`, `pollStorage(1000)`, `flush()` per
set, read via `readQueue()`, subscriber spinning rather than sleeping.

500 Hz is below saturation. Pushed to 2000 Hz every subject queues and repeated runs
vary by more than 2x, which measures the queue rather than the transport.

`tarwyn-rust` is measured on its UDP telemetry plane, its fastest supported path.
Run `SUBJECTS="tarwyn-zmq udp-floor zmq-direct java-udp ..."` to measure the
ZeroMQ path or decompose the gap.

## Results

Microseconds, lower is better, fastest first. `Loss` is published messages that
never arrived, from gaps in the sequence numbers.

### 16 byte payload, 500 Hz
|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust|24.94|15.62|29.84|32.14|34.62|732.16|0.00|
|tarwyn-java|109.26|77.59|155.81|977.94|1782.10|5180.58|0.55|
|nt4|2034.34|20.73|4024.14|4034.32|4046.65|11141.34|0.00|

### 96 byte payload, 500 Hz
|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust|24.78|15.14|29.30|31.82|34.34|1121.28|0.00|
|tarwyn-java|117.86|82.00|539.48|1094.34|1814.26|6237.23|1.23|
|nt4|2030.91|20.43|4022.29|4030.62|4034.71|7290.25|0.00|

## Environment

`2026-08-25 09:49:31 UTC`  
tarwyn-rust `0.0.3` · TARWYN `v5.0.0` · NetworkTables `2025.3.2`  
rustc `1.98.0` · java `25.0.4.1` · libzmq `4.3.5`  
`AMD Ryzen 5 5600X 6-Core Processor` · 12 threads · 31Gi · kernel `7.1.9-arch1-2`  

## Reproduce

```sh
cargo build --release --workspace
JARS=/path/to/jars benches/generate.sh     # see java/README.md for the jars
```
