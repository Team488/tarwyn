# Third-party software

tarwyn is MIT licensed; see [LICENSE](LICENSE). It ships and builds on the
work below, which carries its own terms.

## Distributed inside the native libraries, the jar and the wheel

### Rust crates

The dependency tree is permissively licensed: `tungstenite`, `prost` and
`pyo3` are MIT/Apache-2.0. Nothing in it imposes conditions beyond
attribution. `cargo tree` at the workspace root lists every crate a build
links in.

### WPILib — BSD-3-Clause

The Java, Python and C++ clients take and return WPILib's geometry types, so
the jar depends on `wpimath-java`, the wheel on `robotpy-wpimath`, and the C++
header includes `wpimath`'s headers. None of WPILib is copied into this
repository.

- Source: <https://github.com/wpilibsuite/allwpilib>
- Licence: <https://github.com/wpilibsuite/allwpilib/blob/main/LICENSE.md>

## Used at build time, not distributed

### cbindgen — MPL-2.0

`cbindgen` renders `bindings/c/src/lib.rs` as `bindings/c/include/tarwyn.h`
on every build of the C ABI. The header it writes carries this project's terms;
the generator itself is not distributed.

- Source: <https://github.com/mozilla/cbindgen>

### maturin — MIT/Apache-2.0

`maturin` compiles the PyO3 module and packages it with the Python sources into
the wheel.

- Source: <https://github.com/PyO3/maturin>
