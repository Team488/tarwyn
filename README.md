# tarwyn

[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)

A key/value server for FRC robots, written in Rust. It speaks NetworkTables 4.1,
so AdvantageScope and other NT4 tools connect to it directly, and it ships Rust,
Java, Python and C++ clients.

```sh
cargo run -p tarwyn_server
```

Values and control share one WebSocket on 5810; telemetry is UDP 5809, fire and
forget. Reads return `None` when the server does not answer within the request
timeout. The full API is the rustdoc: `cargo doc --workspace --open`.

Latency against WPILib's ntcore is in [bench/RESULTS.md](bench/RESULTS.md);
[bench/BENCHMARK.md](bench/BENCHMARK.md) says how to rerun it.

## Requirements

| | Needs |
|---|---|
| Server | 64-bit Linux (glibc 2.35+), macOS or Windows |
| Rust client | Rust 1.88+ |
| Java client | JDK 25+ with `--enable-native-access` |
| Python client | Python 3.11–3.14 |
| C++ client | C++23 and the WPILib 2027 `wpimath` headers |

Builds ship for `linux-x86_64`, `linux-aarch64`, `windows-x86_64` and
`macos-aarch64`; Windows on ARM runs the x86_64 build under its emulation
layer. Not supported: the roboRIO, musl, 32-bit, JDK 24 and older.

Both ports sit in the 5800–5810 range FIRST leaves open on a field. Both planes
listen on every interface and authenticate nobody; pass `--bind 127.0.0.1` to
keep the server local.

## Clients

Every client is a thin layer over `tarwyn_client::ffi`:

- **C++ and Java** call the C ABI in `bindings/c` (`libtarwyn`, header
  `bindings/c/include/tarwyn.h`). C++ adds the header-only `tarwyn.hpp` in
  `bindings/cpp`; Java uses the Foreign Function & Memory API.
- **Python** is a PyO3 module in `bindings/python`, packaged by `maturin`.

All three take and return WPILib's own geometry types.

```sh
cargo build -p tarwyn-c --release                 # libtarwyn and tarwyn.h
(cd bindings/java && ./gradlew build)             # the jar
(cd bindings/python && uv run --group dev pytest) # the wheel and its tests
cmake -S bindings/cpp -B build/cpp -DWPILIB_INCLUDE_DIR=<wpimath headers>
cmake --build build/cpp && ctest --test-dir build/cpp
```

`cargo xtask package <target> <platform>` zips a release build; `cargo xtask
version` prints the version.

## Example

```rs
use tarwyn_client::Client;

let client = Client::connect("10.4.88.2");
client.subscribe("test", |data| println!("{data:?}"));
client.start();
client.send_bool("test", true);
```

`Client::new()` targets localhost. Connecting never blocks; the client keeps
retrying in the background, so it can exist before the server does.

## Logging

Published values can be mirrored to a [WPILOG](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
file, which AdvantageScope, Elastic and the DataLogTool open directly:

```rs
client.log_to("/home/lvuser/match.wpilog")?;
let path = client.log_to_drive("match.wpilog")?;  // first writable USB mount
```

Writes go through a bounded queue and never block a publish; overflow is
dropped and counted by `log_dropped()`, and `logging_healthy()` reports whether
the writer still succeeds.

## Notices

Channel names starting with `TARWYN_INTERNAL` are reserved for the server.
