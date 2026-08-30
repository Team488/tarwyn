# Tarwyn RUST
[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)


Make sure you have installed Rust and use a Rust IDE. To start the server, run
```sh
cargo run -p tarwyn_server
```
This should give you an example of the public API of the TARWYN server.

This project uses protobufs to compress bandwidth, and ZeroMQ for transport.

Both transports are reachable from every client. Publishes and reads go over
ZeroMQ, which is reliable and framed; the telemetry plane goes over UDP, which is
roughly 3.6x faster and makes no delivery guarantee.

`.get` holds a per-client ZeroMQ REQ socket, set to `ZMQ_REQ_CORRELATE` (a reply
to an abandoned request is discarded, not handed to the next caller) and
`ZMQ_REQ_RELAXED` (a timed-out request does not wedge the socket). Returns
`None` if the server does not answer within the timeout.

## Benchmarks

One-way latency, 96 byte payload, 500 Hz, publisher and subscriber as separate
processes on one host, every subject in one run, 3000 samples each with 500
warmup discarded. Fastest first.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|29.23|16.48|37.02|117.25|383.23|2721.79|0.00|
|tarwyn v5.0.0|147.36|84.04|623.01|1183.40|1903.00|7034.69|1.67|
|ntcore v2025.3.2|2043.17|28.77|4030.02|4042.55|4049.07|5783.51|0.00|

`ntcore` runs with `sendAll(true)`, `keepDuplicates(true)`, `periodic(0.001)`,
`pollStorage(1000)`, `flush()` after every set, and reads via `readQueue()`.

16 byte results and the run instructions are in [bench/](bench/BENCHMARK.md).

## API

The client speaks the same method names as the original
[TARWYN](https://github.com/Team488/tarwyn): every public `put`/`get` on its
`Requests` class exists here, across Rust, Python and Java — scalars, the seven
list types, poses, coordinates and bezier curves. `putFloat` and `getFloat` are
additions.

The Rust client is written by hand. The Java and Python clients are generated
from [`bindings/src/lib.rs`](bindings/src/lib.rs) by
[BoltFFI](https://github.com/boltffi/boltffi); everything under `bindings/dist/`
is generated output. Each language gets its own idiom — Java returns
`Optional<T>` and primitive arrays, Python returns `None` and releases through
`__del__`.

An absent channel reads as `Optional.empty()` or `None`, never an exception.

```java
try (TarwynClient client = new TarwynClient("10.4.88.2")) {
    client.start();
    client.putDouble("pose", 1.5);
    client.getDouble("pose").ifPresent(Robot::use);
}
```

Subscriptions are pushed, not polled — the consumer is woken when a value
arrives. `subscribe` names a channel, `updates` opens the stream they feed, and
`update.channel()` says which one arrived. Bind it as `AutoCloseable`:
BoltFFI declares `StreamSubscription` package-private.

```java
client.subscribe("pose");
AutoCloseable updates = client.updates(update -> use(update.value()));
```

Beyond TARWYN' surface the server answers `delete`, `getTables`, `getPing`,
`getServerStatistics` and `getRawJson`, plus a compare-and-set it has no
equivalent for. The swap happens inside the server's lock on the value map, so a
read-modify-write across several coprocessors cannot lose an update the way a
`get` followed by a `put` can.

Poses use WPILib's struct layout, checked against `Pose2d.struct` and
`Pose3d.struct` in the tests: packed little-endian doubles, with `Pose3d`
carrying a quaternion written `w` first. That is the one departure from TARWYN,
which uses six euler fields — converting between the two means committing to a
rotation order, and `Rotation3d` already does it correctly.

Java and Python take the curve types as encoded protobuf, byte-identical to
TARWYN' own, so a `BezierCurves` built with its generated classes passes straight
through `toByteArray()`. `putTypedBytes` accepts TARWYN' type tags and decodes its
byte layout — big-endian for scalars, protobuf for the list and geometry types.

API documentation is rustdoc. Build and open it with

```sh
cargo doc --workspace --no-deps --open
```

## Requirements

The server and every client share one native library, so the platform rules are
the same for all of them. ZeroMQ is built from source and linked in, so nothing
needs libzmq installed.

| | Needs |
|---|---|
| Server | 64-bit Linux, macOS or Windows |
| Rust client | Rust 1.85+ (edition 2024) |
| Java client | **JDK 25+**, and `--enable-native-access` |
| Python client | Python 3.10+ |

**Platforms.** `linux-x86_64`, `linux-aarch64`, `windows-x86_64`,
`windows-aarch64`, `macos-x86_64`, `macos-aarch64`. The jar carries all six and
unpacks the right one at runtime. Linux needs glibc 2.35+.

**Not supported:** the roboRIO, musl distributions, anything 32-bit, JDK 24 and
older.

**Building from source** also needs a C++ compiler — for ZeroMQ, and for the JNI
shim BoltFFI compiles from its generated glue — plus
[BoltFFI](https://github.com/boltffi/boltffi) itself:

```sh
cargo install boltffi_cli
```

**Ports.** 4880 (PUB/SUB), 4881 (REQ/REP), 4882 (PUSH/PULL), UDP 4883
(telemetry) — team 488's number, below the ephemeral range. All four are
configurable through `TarwynServer::with_ports_and_telemetry`.

## Tools
Make sure you have rust, python and java installed. `protoc` is *not*
required — the protobuf definitions are compiled by [`protox`](https://crates.io/crates/protox),
a pure-Rust compiler, so a clean `cargo build` needs no external toolchain.

Commit hooks run through [pre-commit](https://pre-commit.com):

```sh
pip install pre-commit && pre-commit install
```

They cover formatting and clippy. Whether the committed clients still match
`bindings/src/lib.rs`, the tests and the Gradle build stay in CI.

Regenerate the clients after changing the bindings:

```sh
cd bindings && boltffi generate java && boltffi generate python
```

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
- [x] Unit Testing
- [x] Custom Logging
- [x] Server Logger Interface
- [x] Further Benchmarking

## Credits

Credits to [TARWYN](https://github.com/Team488/tarwyn)
by [Team488](https://github.com/Team488) for the
original project and implementation
