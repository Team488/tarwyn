# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).
500 Hz, 3000 samples per subject with 500 warmup discarded.

Rows marked (cold) discard no warmup and record only 200
samples, so they show what a freshly started process delivers at boot.
The smaller sample count moves a median on its own, so only differences
much larger than that are worth reading.

## 16 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|26.59|16.98|34.91|52.19|103.36|1732.61|0.00|
|tarwyn v5.0.0|147.54|83.60|815.90|1434.74|1973.22|8710.50|1.67|
|ntcore v2025.3.2|2038.99|23.76|4027.49|4039.06|4047.44|7852.04|0.00|
|ntcore v2025.3.2 (cold)|2044.99|35.28|4034.86|4044.70|4054.91|6165.37|0.00|

## 96 byte payload

|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|
|---|---|---|---|---|---|---|---|
|tarwyn-rust v0.0.3|23.92|15.78|28.32|33.79|42.59|2553.86|0.00|
|tarwyn-rust v0.0.3 (cold)|25.01|17.04|29.88|32.61|38.44|151.22|0.00|
|tarwyn v5.0.0|130.11|77.30|534.57|1258.31|1856.46|6950.88|1.38|
|tarwyn v5.0.0 (cold)|1462.52|219.63|4430.81|6415.84|22709.07|29597.06|79.53|
|ntcore v2025.3.2|2032.75|19.85|4022.91|4032.32|4037.37|5956.58|0.00|
|ntcore v2025.3.2 (cold)|2041.86|30.13|4028.69|4043.89|4060.60|5121.15|0.00|
