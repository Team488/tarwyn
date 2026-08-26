# Tarwyn RUST
[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci-rust.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci-rust.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)


Make sure you have installed Rust and use a Rust IDE. To start the server, run
```sh
cargo run -p tarwyn_server
```
This should give you an example of the public api of TARWYN server. 

This project uses protobufs to compress bandwith and zmq servers. 

`.get` holds a per-client ZeroMQ REQ socket, set to `ZMQ_REQ_CORRELATE` (a reply
to an abandoned request is discarded, not handed to the next caller) and
`ZMQ_REQ_RELAXED` (a timed-out request does not wedge the socket). Returns
`None` if the server does not answer within the timeout.

## Benchmarks

One-way latency, 96 byte payload, 500 Hz, publisher and subscriber as separate
processes on one host, every subject in one run. 500 warmup samples discarded;
the cold row discards none and records 200. Fastest first.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|23.92|15.78|28.32|33.79|42.59|2553.86|0.00|
|tarwyn v5.0.0|130.11|77.30|534.57|1258.31|1856.46|6950.88|1.38|
|tarwyn v5.0.0 (cold)|1462.52|219.63|4430.81|6415.84|22709.07|29597.06|79.53|
|ntcore v2025.3.2|2032.75|19.85|4022.91|4032.32|4037.37|5956.58|0.00|

`ntcore` runs with `sendAll(true)`, `keepDuplicates(true)`, `periodic(0.001)`,
`pollStorage(1000)`, `flush()` after every set, and reads via `readQueue()`.

16 byte results and the run instructions are in [bench/](bench/BENCHMARK.md).

## API

The client speaks the same method names as the original
[TARWYN](https://github.com/Team488/tarwyn): every public `put`/`get` on its
`Requests` class exists here, across Rust, Python and Java — scalars, the seven
list types, poses, coordinates and bezier curves. `putFloat` and `getFloat` are
additions.

Beyond that surface the server answers `delete`, `getTables`, `getPing`,
`getServerStatistics` and `getRawJson`, and one operation TARWYN has no
equivalent for:

```rs
client.compare_and_set("path-lock", None, Kind::String("agent-a".into()));
```

The swap happens inside the server's lock on the value map, so a read-modify-write
across several coprocessors cannot lose an update the way a `get` followed by a
`put` can.

Java and Python take the curve types as encoded protobuf, byte-identical to
TARWYN' own, so a `BezierCurves` built with its generated classes passes straight
through `toByteArray()`. `putTypedBytes` accepts TARWYN' type tags and decodes its
byte layout — big-endian for scalars, protobuf for the list and geometry types.

## Tools
Make sure you have nodejs, rust, python and java installed. `protoc` is *not*
required — the protobuf definitions are compiled by [`protox`](https://crates.io/crates/protox),
a pure-Rust compiler, so a clean `cargo build` needs no external toolchain.

## Example

`TarwynClient::new()` connects to localhost. For another machine — a
coprocessor, or the robot controller — pass its address:

```rs
let client = TarwynClient::connect("10.4.88.2");
```

`with_config` takes an `TarwynConfig` to override the ports or the request
timeout. Connecting never blocks — ZeroMQ dials in the background, so a client
can be built before the server exists.

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

A writer thread takes records over a bounded queue and flushes every 250 ms, so
a publish never waits on the filesystem. Overflow is dropped, not queued:
`log_dropped()` counts it, `logging_healthy()` reports whether the writer still
succeeds. Java has `logTo`, `logToDrive`, `droppedLogRecords`, `loggingHealthy`;
Python matches the Rust names.

## Notices
Please do not attempt to make anything related with TARWYN_INTERNAL, such as channel or strings starting with such prefix. If this prefix is used, it **may** conflict with internal tarwyn processing.

## Roadmap
- [x] Graceful shutdown
- [ ] Unit Testing
- [x] Custom Logging
- [x] Server Logger Interface
- [x] Further Benchmarking

## Credits

Credits to [TARWYN](https://github.com/Team488/tarwyn)
by [Team488](https://github.com/Team488) for the 
original project and implementation
