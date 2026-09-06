# Tarwyn RUST
[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)


A key/value server for FRC robots, rewritten in Rust from
[TARWYN](https://github.com/Team488/tarwyn). It speaks NetworkTables 4.1, so
AdvantageScope and other NT4 tools connect to it directly, and it ships Rust,
Java and Python clients.

Start the server with:
```sh
cargo run -p tarwyn_server
```

Publishers, readers and control all share one WebSocket connection (tungstenite,
on 5810). Control messages are protobuf Request/Reply and values are
MessagePack, both chosen to keep the bytes on the wire small. The telemetry
plane stays on UDP 5809, fire and forget, with no delivery guarantee.

`.get` and the other control reads send a binary protobuf `Request` over that
connection and wait for the matching `Reply`. If the server does not answer
within the request timeout, they return `None`.

The API reference is the rustdoc: `cargo doc --workspace --open`.

Poses go on the wire in WPILib's struct layout, announced as `struct:Pose2d`
and `struct:Pose3d`, so AdvantageScope decodes them and robot code can hand the
raw bytes straight to WPILib without a conversion layer:

```java
TarwynClient client = TarwynClient.connect("10.4.88.2");
Pose2d pose = client.getPose2d("pose");
client.putPose2d("pose", pose);
```

```py
pose = client.get_pose2d("pose")
client.put_pose2d("pose", pose)
```

Both clients convert at the boundary, so poses arrive as WPILib's own `Pose2d`
and `Pose3d`. The Java client binds WPILib 2027 (`org.wpilib.*`); the Python
client depends on `robotpy-wpimath`.

## Benchmarks

One-way latency, 96 byte payload, 500 Hz, publisher and subscriber as separate
processes on one host, every subject in one run, 3000 samples each with 500
warmup discarded. Fastest first.

|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|
|---|---|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|38.53|26.90|45.44|49.53|53.22|60.70|280.32|1554.43|0.00|
|tarwyn v5.0.0|100.90|66.97|359.59|1069.83|1801.03|3406.29|4838.35|7987.33|1.32|
|ntcore v2026.2.2|2029.50|25.90|4019.09|4029.13|4034.44|4050.41|4700.34|5704.60|0.00|

16 byte results, what each subject is, and how to rerun are in
[bench/BENCHMARK.md](bench/BENCHMARK.md).

## Requirements

The server and every client share one native library, so the platform rules are
the same for all of them. The server and clients use tungstenite (pure-Rust) for
transport, so no libzmq is needed.

| | Needs |
|---|---|
| Server | 64-bit Linux, macOS or Windows |
| Rust client | Rust 1.85+ (edition 2024) |
| Java client | **JDK 25+**, and `--enable-native-access` |
| Python client | Python 3.11+ |

**Platforms.** The Rust server supports `linux-x86_64`, `linux-aarch64`,
`windows-x86_64`, `windows-aarch64`, and `macos-aarch64`. The Java jar carries
`linux-x86_64`, `linux-aarch64`, `windows-x86_64`, and `macos-aarch64`, and
unpacks the right one at runtime. Linux needs glibc 2.35+.

**Not supported:** the roboRIO, musl distributions, anything 32-bit, JDK 24 and
older.

**Building from source** needs nothing beyond Rust and a JDK. The Java client
uses the Foreign Function & Memory API, so there is no JNI shim to compile and
no C++ toolchain. The two UniFFI generators are workspace crates under `tools/`,
so Gradle builds them from source at the version this repo pins.

**Ports.** WebSocket 5810 (values + control; endpoint `/nt/<client name>`, the name
chosen by the client), UDP 5809 (telemetry). Both sit in the
5800-5810 range FIRST reserves for team use, which is the only range an FRC
field's FMS leaves open between the robot and the driver station. The two
live ports are configurable through `TarwynServer::with_ports_and_telemetry`
(the 3rd and 4th arguments); the PUB/SUB and PUSH/PULL slots are kept for
source compatibility but unused.

**Listening address.** Both planes listen on every interface, since the clients
are the driver station and the coprocessors rather than anything on the robot
controller itself. Neither plane authenticates its callers, so on a shared
network pass `--bind 127.0.0.1` to keep the WebSocket plane local, or reach it
through `TarwynServer::try_with_bind`.

## Tools
Make sure you have Rust, Python and Java installed. You do not need `protoc`:
the protobuf definitions are compiled by [`protox`](https://crates.io/crates/protox),
a pure-Rust compiler, so a clean `cargo build` needs no external toolchain.

Commit hooks run through [pre-commit](https://pre-commit.com):

```sh
pip install pre-commit && pre-commit install
```

They cover formatting and clippy. Whether the committed clients still match
`bindings/src/lib.rs`, the tests and the Gradle build stay in CI.

Regenerate the clients after changing the bindings:

```sh
./gradlew uniffiGenerate generateWrapper pythonWheel
```

## Example

`TarwynClient::new()` connects to localhost. For another machine, such as a
coprocessor or the robot controller, pass its address:

```rs
let client = TarwynClient::connect("10.4.88.2");
```

`with_config` takes an `TarwynConfig` to override the ports or the request
timeout. Connecting never blocks. The client's reader thread keeps retrying in
the background, so you can build a client before the server exists.

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
Do not name a channel `TARWYN_INTERNAL`, or start one with that prefix. The
server uses it for its own traffic, and yours may collide with it.

## Roadmap
- [x] Unit Testing
- [x] Custom Logging
- [x] Server Logger Interface
- [x] Further Benchmarking

## Credits

Credits to [TARWYN](https://github.com/Team488/tarwyn)
by [Team488](https://github.com/Team488) for the
original project and implementation
