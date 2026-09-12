# Benchmark Results

Regenerate with `bench sweep`; see [BENCHMARK.md](BENCHMARK.md).

## Testbed

The conditions of this run. They change with the machine, so figures from two testbeds say nothing about each other; rerun the benchmark on yours rather than reading these.

|  |  |
|---|---|
|machine|linux 7.2.4-arch1-2, AMD Ryzen 5 5600X 6-Core Processor|
|commit|fba9520|
|measured|500 Hz, 3000 samples, 500 warmup, 3 reps|
|implementations|ntcore=2027.0.0a6.post4, tarwyn=0.1.0|

Cells are medians in microseconds, with the lowest and highest run in brackets, then the p99 and the loss. A row whose two best run-to-run ranges overlap is marked `within noise` and did not measure a difference.

## Clients

What a robot's own code gets, and the comparison that decides anything: every row publishes through its project's own client library.


### 16 B

|Operation|ntcore|tarwyn|Verdict|
|---|---|---|---|
|publish|113.22 (111.55–115.65 over 3) p99 815.62, loss 0.00%|30.22 (27.97–34.98 over 3) p99 130.50, loss 0.00%|tarwyn, 3.7x|

### 96 B

|Operation|ntcore|tarwyn|Verdict|
|---|---|---|---|
|publish|115.33 (110.85–120.38 over 3) p99 748.03, loss 0.00%|28.34 (28.16–30.83 over 3) p99 51.52, loss 0.00%|tarwyn, 4.1x|

## Servers

Each server with its client library taken out of the path. `tarwyn` and `ntcore` are driven by the same raw NT4 publisher from this repo, so the row isolates what each server costs on its own.


### 16 B

|Operation|ntcore|tarwyn|Verdict|
|---|---|---|---|
|publish|36.29 (34.72–36.90 over 3) p99 117.69, loss 0.00%|31.42 (29.47–34.21 over 3) p99 103.68, loss 0.00%|tarwyn, 1.2x|

### 96 B

|Operation|ntcore|tarwyn|Verdict|
|---|---|---|---|
|publish|38.62 (34.69–38.66 over 3) p99 188.67, loss 0.00%|31.77 (29.68–35.58 over 3) p99 66.24, loss 0.00%|within noise (tarwyn vs ntcore)|

## Detail

Every percentile the run recorded.

|Section|Operation|Implementation|Version|Payload|P0|Median|P80|P90|P95|P99|P99.9|P100|Loss (%)|Samples|Achieved (Hz)|
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|Clients|publish|ntcore|2027.0.0a6.post4|16 B|62.21|113.22|122.56|128.06|142.08|815.62|1728.51|3246.08|0.00|3000|500.0|
|Clients|publish|tarwyn|0.1.0|16 B|17.97|30.22|37.95|42.98|50.94|130.50|1183.74|2344.96|0.00|3000|500.0|
|Clients|publish|ntcore|2027.0.0a6.post4|96 B|58.56|115.33|124.80|130.05|141.95|748.03|1589.25|3151.87|0.00|3000|500.0|
|Clients|publish|tarwyn|0.1.0|96 B|17.78|28.34|35.55|39.77|44.26|51.52|598.01|2533.38|0.00|3000|500.0|
|Servers|publish|ntcore|2027.0.0a6.post4|16 B|21.98|36.29|42.24|46.49|50.66|117.69|1398.78|2085.89|0.00|3000|500.0|
|Servers|publish|tarwyn|0.1.0|16 B|17.26|31.42|36.67|39.77|42.85|103.68|1261.57|2818.05|0.00|3000|500.0|
|Servers|publish|ntcore|2027.0.0a6.post4|96 B|22.21|38.62|44.83|49.89|56.90|188.67|2007.04|2293.76|0.00|3000|500.0|
|Servers|publish|tarwyn|0.1.0|96 B|18.14|31.77|37.79|41.92|45.95|66.24|967.17|2054.14|0.00|3000|500.0|

## Run-to-Run Spread

How far the median moved between runs of the same row.

|Section|Operation|Implementation|Payload|Runs|Lowest median|Highest median|Spread (%)|
|---|---|---|---|---|---|---|---|
|Clients|publish|ntcore|16 B|3|111.55|115.65|3.6|
|Clients|publish|tarwyn|16 B|3|27.97|34.98|23.2|
|Clients|publish|ntcore|96 B|3|110.85|120.38|8.3|
|Clients|publish|tarwyn|96 B|3|28.16|30.83|9.4|
|Servers|publish|ntcore|16 B|3|34.72|36.90|6.0|
|Servers|publish|tarwyn|16 B|3|29.47|34.21|15.1|
|Servers|publish|ntcore|96 B|3|34.69|38.66|10.3|
|Servers|publish|tarwyn|96 B|3|29.68|35.58|18.6|
