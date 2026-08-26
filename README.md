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
every subject measured back to back in a single run so they share machine
conditions. Lower is better; `Loss` is the share of published messages that never
arrived, and `P100` is the worst single sample.

Benchmark ran with a 96 byte payload, 500 Hz, 500 warmup samples discarded, every
subject at the same rate.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust|27.04|16.85|32.49|39.04|51.58|3162.11|0.00|
|tarwyn|131.31|76.12|168.19|488.24|1321.35|7521.00|1.61|
|nt4|2035.93|23.22|4026.26|4035.63|4041.45|6854.36|0.00|

The medians are the headline, but the tails are the reason for the project.
`tarwyn-rust` stays within 1.4x of its median at P90 and loses nothing, while
`tarwyn` is 3.7x its median at P90 and drops 1.61% of messages. `nt4` is flat
near 4 ms from P80 to P95 despite a P0 of 23 us, so almost every message waits on
something with a fixed period; that was not isolated, and it is not the
`periodic` setting, which is 1 ms here.

`tarwyn` is the original Java [TARWYN](https://github.com/Team488/tarwyn)
v5.0.0. `nt4` is NetworkTables 4 from WPILib 2025.3.2 configured with
`sendAll(true)`, `keepDuplicates(true)`, `periodic(0.001)`, `pollStorage(1000)`,
`flush()` after every set, and reads via `readQueue()` — 2025.3.2 because WPILib
publishes no `linuxx86-64` JNI build for 2026.

The 16 byte payload and the Rust-side transport comparison are in
[bench/RESULTS.md](bench/RESULTS.md); regenerate both with `bench/generate.sh`.

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
