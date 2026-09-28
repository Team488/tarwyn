# tarwyn

[![CI](https://github.com/Team488/tarwyn/actions/workflows/ci.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/ci.yml) [![Release](https://github.com/Team488/tarwyn/actions/workflows/release.yml/badge.svg)](https://github.com/Team488/tarwyn/actions/workflows/release.yml)

tarwyn is a NetworkTables 4.1 server for FRC robots, written in Rust.
AdvantageScope and other NT4 tools connect to it unchanged, and it comes with
clients for Rust, Java, Python and C++.

```sh
cargo run -p tarwyn_server
```

```rs
use tarwyn_client::Client;

let client = Client::connect("10.4.88.2");
client.subscribe("test", |data| println!("{data:?}"));
client.start();
client.send_bool("test", true);
```

Values and control go over a WebSocket on port 5810, and telemetry over UDP
5809. There is no authentication, so use `--bind 127.0.0.1` to keep the server
local. `cargo doc --workspace --open` builds the API docs.

## Latency

Time from a publish to the subscriber's callback on one machine, with default
settings:

| Rate | tarwyn | ntcore |
|---|---|---|
| 500 Hz | 21 µs | 104 µs |
| 50 Hz | 43 µs | 142 µs |

The full results are in [bench/RESULTS.md](bench/RESULTS.md), and
[bench/BENCHMARK.md](bench/BENCHMARK.md) explains how they were taken.

Most of that time is thread wakeups. Two flags spend CPU to avoid them, and
the client has the same settings as `Config::predict` and `Config::busy_poll`.
Windows ignores both.

| Flag | Default | Does | Costs |
|---|---|---|---|
| `--predict <MICROS>` | 200 | wakes the reader just before the next periodic message | ~7% of a core at 500 Hz |
| `--busy-poll <MICROS>` | 0 | spins after every message | a whole core |

## Requirements

| | Needs |
|---|---|
| Server | 64-bit Linux (glibc 2.35+), macOS or Windows |
| Rust client | Rust 1.88+ |
| Java client | JDK 25+ with `--enable-native-access` |
| Python client | Python 3.11 to 3.14 |
| C++ client | C++23 and the WPILib 2027 `wpimath` headers |

Releases are built for `linux-x86_64`, `linux-aarch64`, `windows-x86_64` and
`darwin-arm64`. The roboRIO, musl and 32-bit targets are not supported.

## Clients

The C++ header (`bindings/cpp/tarwyn.hpp`) and the Java library both wrap the
C ABI in `bindings/c`, and Python uses a PyO3 module in `bindings/python`.

```sh
cargo build -p tarwyn-c --release                 # libtarwyn and tarwyn.h
(cd bindings/java && ./gradlew build)             # the jar
(cd bindings/python && uv run --group dev pytest) # the wheel and its tests
cmake -S bindings/cpp -B build/cpp -DWPILIB_INCLUDE_DIR=<wpimath headers>
cmake --build build/cpp && ctest --test-dir build/cpp
cargo xtask package <target> <platform>           # a release zip
```

## Logging

The client can mirror published values to a [wpilog](https://github.com/wpilibsuite/allwpilib/blob/main/wpiutil/doc/datalog.adoc)
file, which AdvantageScope, Elastic and the DataLogTool can open:

```rs
client.log_to("/home/lvuser/match.wpilog")?;
let path = client.log_to_drive("match.wpilog")?;  // first writable USB mount
```

Writing the log never blocks a publish. `log_dropped()` counts records that
did not fit in the queue, and `logging_healthy()` reports whether writes still
succeed.

Channel names that start with `TARWYN_INTERNAL` are reserved for the server.
