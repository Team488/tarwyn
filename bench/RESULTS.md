# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).
500 Hz, 500 warmup samples discarded, 3000 recorded per subject.

## 16 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|26.59|16.98|34.91|52.19|103.36|1732.61|0.00|
|tarwyn v5.0.0|147.54|83.60|815.90|1434.74|1973.22|8710.50|1.67|
|ntcore v2025.3.2|2038.99|23.76|4027.49|4039.06|4047.44|7852.04|0.00|

Cold, no warmup discarded — the first 200 messages a
freshly started process sees. Recorded over 200 samples
against 3000 warm, so only large differences here are meaningful;
sample count alone moves the median and both tails.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|ntcore v2025.3.2|2044.99|35.28|4034.86|4044.70|4054.91|6165.37|0.00|

## 96 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|23.92|15.78|28.32|33.79|42.59|2553.86|0.00|
|tarwyn v5.0.0|130.11|77.30|534.57|1258.31|1856.46|6950.88|1.38|
|ntcore v2025.3.2|2032.75|19.85|4022.91|4032.32|4037.37|5956.58|0.00|

Cold, no warmup discarded — the first 200 messages a
freshly started process sees. Recorded over 200 samples
against 3000 warm, so only large differences here are meaningful;
sample count alone moves the median and both tails.

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|25.01|17.04|29.88|32.61|38.44|151.22|0.00|
|tarwyn v5.0.0|1462.52|219.63|4430.81|6415.84|22709.07|29597.06|79.53|
|ntcore v2025.3.2|2041.86|30.13|4028.69|4043.89|4060.60|5121.15|0.00|
