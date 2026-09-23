# Third-party software

tarwyn is MIT licensed ([LICENSE](LICENSE)). `cargo deny check` verifies the
Rust dependency tree against [deny.toml](deny.toml).

| | License | Role |
|---|---|---|
| [WPILib](https://github.com/wpilibsuite/allwpilib) | BSD-3-Clause | a run-time dependency, never copied in: the jar uses `wpimath-java`, the wheel `robotpy-wpimath`, the C++ header `wpimath` |
| [cbindgen](https://github.com/mozilla/cbindgen) | MPL-2.0 | build time only, renders `tarwyn.h` |
| [maturin](https://github.com/PyO3/maturin) | MIT/Apache-2.0 | build time only, builds the wheel |
