# Benchmark results

One-way latency, publisher and subscriber as separate processes on one host,
every subject measured back to back in a single run. Regenerate with
`bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).

Benchmark ran at 500 Hz with 500 warmup samples discarded and
3000 samples recorded per subject.

## 16 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust|26.56|16.33|31.18|36.45|44.48|2094.08|0.00|
|tarwyn|141.38|72.66|448.87|1123.30|1859.13|5256.53|1.12|
|nt4|2040.78|24.45|4030.04|4040.74|4051.69|7173.64|0.00|

## 96 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust|27.04|16.85|32.49|39.04|51.58|3162.11|0.00|
|tarwyn|131.31|76.12|168.19|488.24|1321.35|7521.00|1.61|
|nt4|2035.93|23.22|4026.26|4035.63|4041.45|6854.36|0.00|
