# Third-party software

Tarwyn-Rust is MIT licensed; see [LICENSE](LICENSE). It ships and builds on the
work below, which carries its own terms.

## Distributed inside the native library, the jar and the wheel

### ZeroMQ (libzmq) — MPL-2.0

ZeroMQ is compiled from source and linked into `libtarwyn_ffi`, so every
artifact that carries the native library also carries libzmq: the release
archives, the jar, and the Python wheel.

The Mozilla Public License 2.0 is file-level copyleft. It places no conditions
on the rest of this project, but recipients of a binary containing libzmq are
entitled to the source of the MPL-covered files.

- Source: <https://github.com/zeromq/libzmq>
- Licence: <https://github.com/zeromq/libzmq/blob/master/LICENSE>

### Rust crates

The dependency tree is permissively licensed — `zmq` and `zmq-sys` are
MIT/Apache-2.0, `pyo3` is MIT OR Apache-2.0, `prost` is Apache-2.0. Nothing in
it imposes conditions beyond attribution.

## Present in the repository, not distributed

### Boost.UT — BSL-1.0

The C++ test suite builds against the Boost.UT single header, fetched at build
time by the `fetchBoostUt` Gradle task and pinned by tag and SHA-256 in
`gradle.properties`. It is test-only and appears in no released artifact, and no
copy is committed here.

- Copyright (c) 2019-2021 Kris Jusiak
- Licence: <https://www.boost.org/LICENSE_1_0.txt>

## Upstream

This project implements the API and wire format of
[TARWYN](https://github.com/Team488/tarwyn) by Jun Jie Huang, which is MIT
licensed. No TARWYN source is included here.
