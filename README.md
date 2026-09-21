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

## Latency

Most of a value's latency is thread wakeups, not server work. Two flags trade
CPU for those wakeups:

- `--predict <MICROS>` (default 200): the reader wakes just before the next
  periodic message is due and spins briefly. 96 → 69 µs at 500 Hz for 7% of a
  core; `0` disables it.
- `--busy-poll <MICROS>` (default 0): the reader spins for that long after
  every message. 89 → 49 µs at 500 Hz, at the cost of a whole core.

The client has the same two settings, `Config::predict` and
`Config::busy_poll`. Details and measurements: [bench/BENCHMARK.md](bench/BENCHMARK.md).

## Requirements

| | Needs |
|---|---|
| Server | 64-bit Linux (glibc 2.35+), macOS or Windows |
| Rust client | Rust 1.88+ |
| Java client | JDK 25+ with `--enable-native-access` |
| Python client | Python 3.11–3.14 |
| C++ client | C++23 and the WPILib 2027 `wpimath` headers |

Builds ship for `linux-x86_64`, `linux-aarch64`, `windows-x86_64` and
`macos-aarch64`. Not supported: the roboRIO, musl, 32-bit, JDK 24 and older.

Both planes listen on every interface with no authentication; pass
`--bind 127.0.0.1` to keep the server local.

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

`Client::new()` targets localhost. Connecting never blocks and retries in the
background.

## Logging

Published values can be mirrored to a [wpilog](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
file, which AdvantageScope, Elastic and the DataLogTool open directly:

```rs
client.log_to("/home/lvuser/match.wpilog")?;
let path = client.log_to_drive("match.wpilog")?;  // first writable USB mount
```

Writes never block a publish; `log_dropped()` counts overflow and
`logging_healthy()` reports whether the writer still succeeds.

## Notices

Channel names starting with `TARWYN_INTERNAL` are reserved for the server.
