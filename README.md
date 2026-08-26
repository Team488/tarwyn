# Tarwyn RUST
[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci-rust.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci-rust.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)


Make sure you have installed rust and use a rust ide. To start the server, run
```sh
cargo run -p tarwyn_server
```
This should give you an example of the public api of tarwyn server. 

This project uses protobufs to compress bandwith and zmq servers. 

Note: `.get` uses a ZeroMQ REQ/REP socket pair. Each client holds its own REQ
socket, so replies cannot be delivered to the wrong client. The socket is
configured with `ZMQ_REQ_CORRELATE` so a reply to an abandoned request is
discarded rather than returned to the next caller, and with `ZMQ_REQ_RELAXED`
so a timed-out request does not wedge the socket. `.get` returns `None` when the
server does not answer within the configured timeout.

## Benchmarks

One-way latency, publisher and subscriber as separate processes on one host,
every subject measured back to back in a single run.

Benchmark ran with a 96 byte payload, 500 Hz, 500 warmup samples discarded, every
subject at the same rate.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|23.92|15.78|28.32|33.79|42.59|2553.86|0.00|
|tarwyn v5.0.0|130.11|77.30|534.57|1258.31|1856.46|6950.88|1.38|
|ntcore v2025.3.2|2032.75|19.85|4022.91|4032.32|4037.37|5956.58|0.00|

Cold, with no warmup discarded — the first 200 messages a freshly started process
sees, which is what a robot gets at boot:

|Subject (us)|Median|P0|P90|P100|Loss (%)|
|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|27.36|18.53|34.53|164.99|0.00|
|tarwyn v5.0.0|1462.52|219.63|6415.84|29597.06|79.53|
|ntcore v2025.3.2|2041.86|30.13|4043.89|5121.15|0.00|

TARWYN is 11x slower cold than warm and drops 79.53% of messages before the JIT
catches up. ntcore is unchanged cold, because its latency is not JIT-bound. The
Rust client costs 14% cold, and its worst sample is better cold than warm.

`ntcore` runs with `sendAll(true)`, `keepDuplicates(true)`, `periodic(0.001)`,
`pollStorage(1000)`, `flush()` after every set, and reads via `readQueue()`.
2025.3.2 is pinned because WPILib publishes no JNI classifiers for 2026.

16 byte results and the run instructions are in [bench/](bench/BENCHMARK.md).

## Tools
Make sure you have nodejs, rust, python and java installed. `protoc` is *not*
required — the protobuf definitions are compiled by [`protox`](https://crates.io/crates/protox),
a pure-Rust compiler, so a clean `cargo build` needs no external toolchain.

## Example

`TarwynClient::new()` connects to a server on localhost. To reach one on
another machine — a coprocessor, or the robot controller — pass its address:

```rs
let client = TarwynClient::connect("10.4.88.2");
```

`TarwynClient::with_config` takes an `TarwynConfig` if you also need to
override the ports or the request timeout. Connecting never blocks: ZeroMQ dials
in the background, so a client can be built before the server exists.

```rs
use tarwyn_client::tarwyn_client::TarwynClient;

fn main() {
    println!("Starting tarwyn client...");
    let client = TarwynClient::new();

    let _ = client.subscribe_to_logs(|logs| {
        println!("{}", logs);
    });

    let _ = client.subscribe("test", |data| {
        println!("Received data on 'test': {:?}", data);
    });
    client.start();

    client.send_bool("test", true);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
```

## Logging

Every published value can be mirrored to a [WPILOG](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
file, which AdvantageScope, Elastic and the WPILib DataLogTool open directly.

```rs
client.log_to("/home/lvuser/match.wpilog")?;
```

`log_to_drive` picks the first writable removable mount under `/media`,
`/run/media` or `/mnt` and returns the path it chose:

```rs
let path = client.log_to_drive("match.wpilog")?;
```

Records are handed to a writer thread over a bounded queue and flushed every
250 ms, so a publish never waits on the filesystem and a yanked drive cannot
stall the robot. Anything that does not fit the queue is dropped rather than
queued — `log_dropped()` counts it and `logging_healthy()` reports whether the
writer is still succeeding. The same four calls exist on the Java client
(`logTo`, `logToDrive`, `droppedLogRecords`, `loggingHealthy`) and the Python
client (`log_to`, `log_to_drive`, `log_dropped`, `logging_healthy`).

## Notices
Please do not attempt to make anything related with TARWYN_INTERNAL, such as channel or strings starting with such prefix. If this prefix is used, it **may** conflict with internal tarwyn processing.

## Roadmap
- [x] Graceful shutdown
- [ ] Unit Testing
- [x] Custom Logging
- [x] Server Logger Interface
- [x] Further Benchmarking
