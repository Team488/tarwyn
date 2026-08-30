# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).
500 Hz, 3000 samples per subject with 500 warmup discarded.
Measured on 12 cores, Linux 7.1.11-arch1-1.

## 16 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|28.80|16.25|37.50|110.53|326.65|2650.11|0.00|
|tarwyn v5.0.0|148.04|81.66|600.08|1304.64|1897.22|10791.52|1.41|
|ntcore v2025.3.2|3083.09|41.02|5080.41|5087.71|5094.00|15286.16|0.00|

## 96 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.1.0|29.23|16.48|37.02|117.25|383.23|2721.79|0.00|
|tarwyn v5.0.0|147.36|84.04|623.01|1183.40|1903.00|7034.69|1.67|
|ntcore v2025.3.2|2043.17|28.77|4030.02|4042.55|4049.07|5783.51|0.00|
