# Third-party software

Tarwyn-Rust is MIT licensed; see [LICENSE](LICENSE). It ships and builds on the
work below, which carries its own terms.

## Distributed inside the native libraries and the jar

### ZeroMQ (libzmq) — MPL-2.0

ZeroMQ is compiled from source and linked into `libtarwyn_bindings`, so every
artifact that carries the native library also carries libzmq: the release
archives and the jar.

The Mozilla Public License 2.0 is file-level copyleft. It places no conditions
on the rest of this project, but recipients of a binary containing libzmq are
entitled to the source of the MPL-covered files.

- Source: <https://github.com/zeromq/libzmq>
- Licence: <https://github.com/zeromq/libzmq/blob/master/LICENSE>

### Rust crates

The dependency tree is permissively licensed — `zmq` and `zmq-sys` are
MIT/Apache-2.0, `prost` is Apache-2.0, `boltffi` is MIT. Nothing in it imposes
conditions beyond attribution.

## Used at build time, not distributed

### BoltFFI — MIT

The Java and Python clients are generated from `bindings/src/lib.rs` by the
`boltffi` tool, which also emits the JNI and CPython glue compiled into the
shipped natives. The generated sources are committed here and carry no separate
terms; the generator itself is not distributed.

- Source: <https://github.com/boltffi/boltffi>

## Upstream

This project implements the API and wire format of
[TARWYN](https://github.com/Team488/tarwyn) by Jun Jie Huang, which is MIT
licensed. No TARWYN source is included here.
