# Benchmark Results

Latency is one way, in microseconds. [BENCHMARK.md](BENCHMARK.md) explains how it is measured and how to read the tables, and `results.json` beside this file has every percentile. Regenerate with `bench sweep`.

|  |  |
|---|---|
|date|2026-10-17|
|machine|linux 7.2.10-arch1-1, AMD Ryzen 5 5600X 6-Core Processor, 12 logical cpus|
|state|governor powersave, boost on, load 0.77 at start|
|pinning|publisher on cpu 1, subscriber on cpu 2, server on cpus 3,4,5|
|commit|f6a8cc7-dirty|
|measured|50 and 500 Hz, 3000 samples after 500 warmup, 3 reps|
|implementations|ntcore-2026=2026.2.2, ntcore=2027.0.0a7, tarwyn=0.1.1|

## Clients

### 50 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|75.4|74.0 to 95.6|292.1|0.2%|
|subscribe, 3 subscribers|ntcore|173.1|146.4 to 197.5|1177.6|0.3%|
|subscribe, 3 subscribers|ntcore-2026|3207.2|2236.4 to 3297.3|5361.7|0.3%|
|publish|tarwyn|62.8|59.2 to 79.9|244.2|0.1%|
|publish|ntcore|154.1|132.9 to 221.2|634.4|0.2%|
|publish|ntcore-2026|1249.3|1187.8 to 2283.5|5255.2|0.3%|
|subscribe|tarwyn|68.5|67.9 to 81.2|285.4|0.1%|
|subscribe|ntcore|159.6|136.2 to 216.6|1101.8|0.2%|
|subscribe|ntcore-2026|2277.4|152.3 to 2291.7|5238.8|0.3%|

- `subscribe, 3 subscribers`: tarwyn 2.3x faster than ntcore on the median and 4.0x on the p99.
- `publish`: tarwyn 2.5x faster than ntcore on the median and 2.6x on the p99.
- `subscribe`: tarwyn 2.3x faster than ntcore on the median and 3.9x on the p99.

### 50 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|71.3|70.6 to 98.6|393.5|0.1%|
|subscribe, 3 subscribers|ntcore|155.3|148.2 to 240.8|1381.4|0.2%|
|subscribe, 3 subscribers|ntcore-2026|3246.1|3215.4 to 3299.3|5365.8|0.4%|
|publish|tarwyn|67.3|56.2 to 81.9|247.4|0.1%|
|publish|ntcore|154.0|138.5 to 173.3|1085.4|0.2%|
|publish|ntcore-2026|1210.4|1206.3 to 2248.7|5292.0|0.2%|
|subscribe|tarwyn|75.0|58.3 to 82.9|480.5|0.1%|
|subscribe|ntcore|144.5|137.8 to 222.5|787.5|0.2%|
|subscribe|ntcore-2026|2215.9|137.8 to 2299.9|5201.9|0.3%|

- `subscribe, 3 subscribers`: tarwyn 2.2x faster than ntcore on the median and 3.5x on the p99.
- `publish`: tarwyn 2.3x faster than ntcore on the median and 4.4x on the p99.
- `subscribe`: tarwyn 1.9x faster than ntcore on the median and 1.6x on the p99.

### 50 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|72.5|68.8 to 97.8|494.3|0.2%|
|subscribe, 3 subscribers|ntcore|153.8|150.8 to 237.3|1153.0|0.2%|
|subscribe, 3 subscribers|ntcore-2026|2306.1|2209.8 to 3209.2|5296.1|0.3%|
|publish|tarwyn|63.7|57.4 to 76.4|501.8|0.1%|
|publish|ntcore|181.6|132.5 to 212.7|1144.8|0.2%|
|publish|ntcore-2026|2283.5|1249.3 to 3192.8|5304.3|0.3%|
|subscribe|tarwyn|59.1|51.6 to 76.0|276.7|0.1%|
|subscribe|ntcore|134.5|132.6 to 216.1|808.5|0.2%|
|subscribe|ntcore-2026|2189.3|2189.3 to 3280.9|5210.1|0.2%|

- `subscribe, 3 subscribers`: tarwyn 2.1x faster than ntcore on the median and 2.3x on the p99.
- `publish`: tarwyn 2.9x faster than ntcore on the median and 2.3x on the p99.
- `subscribe`: tarwyn 2.3x faster than ntcore on the median and 2.9x on the p99.

### 500 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|49.8|46.6 to 58.2|80.9|0.9%|
|subscribe, 3 subscribers|ntcore|130.7|130.4 to 151.6|716.8|1.4%|
|subscribe, 3 subscribers|ntcore-2026|5238.8|5201.9 to 6205.4|8245.2|0.8%|
|publish|tarwyn|32.9|30.2 to 43.5|55.4|0.5%|
|publish|ntcore|112.8|97.5 to 137.0|199.8|0.8%|
|publish|ntcore-2026|4276.2|4177.9 to 6189.1|8396.8|0.7%|
|subscribe|tarwyn|35.8|34.0 to 45.6|58.7|0.5%|
|subscribe|ntcore|111.2|109.0 to 136.2|694.8|1.0%|
|subscribe|ntcore-2026|4202.5|2119.7 to 5189.6|7393.3|0.6%|

- `subscribe, 3 subscribers`: tarwyn 2.6x faster than ntcore on the median and 8.9x on the p99.
- `publish`: tarwyn 3.4x faster than ntcore on the median and 3.6x on the p99.
- `subscribe`: tarwyn 3.1x faster than ntcore on the median and 11.8x on the p99.

### 500 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|48.6|44.5 to 59.2|78.1|0.9%|
|subscribe, 3 subscribers|ntcore|133.5|128.5 to 154.2|325.1|1.4%|
|subscribe, 3 subscribers|ntcore-2026|4292.6|3199.0 to 5201.9|8220.7|0.7%|
|publish|tarwyn|34.7|32.0 to 43.8|55.5|0.7%|
|publish|ntcore|110.3|106.1 to 135.9|650.8|0.8%|
|publish|ntcore-2026|5185.5|2119.7 to 5206.0|8208.4|0.6%|
|subscribe|tarwyn|36.4|34.0 to 45.9|62.3|0.7%|
|subscribe|ntcore|112.2|109.5 to 139.5|741.4|1.0%|
|subscribe|ntcore-2026|4169.7|3176.4 to 4251.6|6193.1|0.6%|

- `subscribe, 3 subscribers`: tarwyn 2.7x faster than ntcore on the median and 4.2x on the p99.
- `publish`: tarwyn 3.2x faster than ntcore on the median and 11.7x on the p99.
- `subscribe`: tarwyn 3.1x faster than ntcore on the median and 11.9x on the p99.

### 500 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|48.4|45.8 to 57.7|73.2|0.9%|
|subscribe, 3 subscribers|ntcore|130.5|129.9 to 160.8|214.4|1.4%|
|subscribe, 3 subscribers|ntcore-2026|5226.5|2187.3 to 5255.2|9265.1|0.7%|
|publish|tarwyn|29.1|27.9 to 37.7|52.9|0.7%|
|publish|ntcore|107.2|105.9 to 133.5|348.9|0.8%|
|publish|ntcore-2026|3186.7|3184.6 to 4231.2|5218.3|0.7%|
|subscribe|tarwyn|32.3|31.6 to 42.8|56.1|0.7%|
|subscribe|ntcore|109.2|106.9 to 133.1|441.6|0.8%|
|subscribe|ntcore-2026|4239.4|2127.9 to 6189.1|8384.5|0.4%|

- `subscribe, 3 subscribers`: tarwyn 2.7x faster than ntcore on the median and 2.9x on the p99.
- `publish`: tarwyn 3.7x faster than ntcore on the median and 6.6x on the p99.
- `subscribe`: tarwyn 3.4x faster than ntcore on the median and 7.9x on the p99.

## Servers

### 50 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|ntcore|73.2|71.6 to 98.8|518.6|0.2%|
|publish|tarwyn|78.8|68.4 to 80.4|969.2|0.1%|
|publish|ntcore-2026|2357.2|2144.3 to 3125.2|5181.4|0.3%|

- `publish`: ntcore and tarwyn within noise on the median, ntcore 1.9x on the p99.
- `publish`/ntcore-2026: 2 attempts reported nothing and were retried.

### 50 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|67.1|64.0 to 79.4|258.4|0.1%|
|publish|ntcore|75.7|71.7 to 97.6|391.2|0.2%|
|publish|ntcore-2026|3106.8|2379.8 to 3156.0|4452.4|0.3%|

- `publish`: tarwyn and ntcore within noise on the median, tarwyn 1.5x on the p99.

### 50 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|57.3|50.4 to 74.3|635.9|0.1%|
|publish|ntcore|77.0|71.5 to 97.3|654.3|0.2%|
|publish|ntcore-2026|3106.8|2140.2 to 3123.2|4718.6|0.3%|

- `publish`: tarwyn and ntcore within noise on the median, tarwyn 1.0x on the p99.
- `publish`/ntcore-2026: 1 attempt reported nothing and was retried.

### 500 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|36.3|33.9 to 45.6|62.0|0.7%|
|publish|ntcore|37.1|34.5 to 51.6|65.7|0.9%|
|publish|ntcore-2026|2052.1|2037.8 to 2056.2|4231.2|0.7%|

- `publish`: tarwyn and ntcore within noise on the median, tarwyn 1.1x on the p99.
- `publish`/ntcore-2026: 2 attempts reported nothing and were retried.

### 500 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|35.8|35.2 to 44.7|57.5|0.7%|
|publish|ntcore|37.0|35.9 to 50.9|63.6|0.9%|
|publish|ntcore-2026|2048.0|2037.8 to 2058.2|4186.1|0.8%|

- `publish`: tarwyn and ntcore within noise on the median, tarwyn 1.1x on the p99.
- `publish`/ntcore-2026: 1 attempt reported nothing and was retried.

### 500 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|33.6|31.7 to 42.4|56.2|0.5%|
|publish|ntcore|36.8|36.5 to 50.9|58.9|0.8%|
|publish|ntcore-2026|2049.0|2042.9 to 3090.4|4231.2|0.8%|

- `publish`: tarwyn and ntcore within noise on the median, tarwyn 1.0x on the p99.

Not measured: more than 3 subscribers, rates other than 50 and 500 Hz, payloads other than 16 B, 96 B and 1024 B, traffic that crosses a network, memory.

<!-- bench sim -->
## Robot load: team488-2026

The profile was captured from Team488/TeamXbot2026@55fa6f0 in simulation, in autonomous, with the robot loop scaled from 25 Hz to 50 Hz. Run 2026-10-17 on AMD Ryzen 5 5600X 6-Core Processor, 12 logical cpus, commit f6a8cc7, with 5 s of warmup and 30 s measured per step and 3 reps. Implementations: tarwyn=0.1.1, ntcore-2026=2026.2.2, ntcore-2027=2027.0.0a7. [BENCHMARK.md](BENCHMARK.md#bench-sim) explains the columns and verdicts.

### Headroom

|Implementation|Budget|Passed up to|Stopped by|
|---|---|---|---|
|ntcore-2026|none|2x|4x: broke: camera p99 5.4 ms over 4.2 ms|
|ntcore-2027|none|8x|16x: broke: camera p99 1.6 ms over 1.0 ms|
|tarwyn|none|8x|16x: broke: camera p99 1.4 ms over 1.0 ms|
|ntcore-2026|1core|2x|4x: broke: camera p99 6.1 ms over 4.2 ms|
|ntcore-2027|1core|4x|8x: broke: camera p99 2.6 ms over 2.1 ms|
|tarwyn|1core|4x|8x: broke: camera p99 2.4 ms over 2.1 ms|
|ntcore-2026|25pct|2x|4x: broke: camera p99 6.1 ms over 4.2 ms|
|ntcore-2027|25pct|-|1x: broke: camera p99 17.8 ms over 16.7 ms|
|tarwyn|25pct|2x|4x: broke: camera p99 7.7 ms over 4.2 ms|

### Rate, budget none

|Rate|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|1x|tarwyn|111.7 µs / 1.3 ms|193.8 µs / 342.5 µs|560.1 µs / 716.3 µs|0%|-|19.4 / 19.4 ms|4.4%|28.6 MB|pass (3 of 3)|
|1x|ntcore-2026|159.4 µs / 1.4 ms|13.8 ms / 20.0 ms|16.5 ms / 20.1 ms|0%|-|102.2 / 102.4 ms|4.6%|122.3 MB|pass (3 of 3)|
|1x|ntcore-2027|151.4 µs / 1.4 ms|226.1 µs / 466.2 µs|1.6 ms / 4.4 ms|0%|-|11.1 / 19.4 ms|4.4%|96.2 MB|pass (3 of 3)|
|2x|tarwyn|105.2 µs / 1.4 ms|188.5 µs / 535.0 µs|386.3 µs / 519.4 µs|0%|-|19.8 / 19.8 ms|7.8%|28.8 MB|pass (3 of 3)|
|2x|ntcore-2026|144.5 µs / 1.5 ms|10.2 ms / 20.0 ms|10.2 ms / 20.2 ms|0%|-|102.0 / 102.2 ms|7.6%|114.9 MB|pass (3 of 3)|
|2x|ntcore-2027|140.9 µs / 1.6 ms|209.7 µs / 847.9 µs|1.2 ms / 1.4 ms|0%|-|11.1 / 19.4 ms|7.4%|96.6 MB|pass (3 of 3)|
|4x|tarwyn|98.9 µs / 1.4 ms|177.3 µs / 451.6 µs|374.0 µs / 1.3 ms|0%|-|19.4 / 19.4 ms|13.5%|27.6 MB|pass (3 of 3)|
|4x|ntcore-2026|1.0 ms / 5.2 ms|10.2 ms / 19.9 ms|9.7 ms / 20.2 ms|0%|-|101.4 / 101.6 ms|12.1%|112.4 MB|broke: camera p99 5.4 ms over 4.2 ms (3 of 3)|
|4x|ntcore-2027|127.9 µs / 1.6 ms|184.6 µs / 620.0 µs|1.4 ms / 2.0 ms|0%|-|10.8 / 20.5 ms|12.4%|96.8 MB|pass (3 of 3)|
|8x|tarwyn|95.2 µs / 1.4 ms|167.4 µs / 471.8 µs|537.6 µs / 1.3 ms|0%|-|18.8 / 18.8 ms|24.5%|28.2 MB|pass (3 of 3)|
|8x|ntcore-2027|118.8 µs / 1.6 ms|167.8 µs / 707.1 µs|1.4 ms / 2.4 ms|0%|-|11.1 / 20.0 ms|21.3%|98.3 MB|pass (3 of 3)|
|16x|tarwyn|91.9 µs / 1.5 ms|159.2 µs / 739.8 µs|489.5 µs / 3.3 ms|0%|-|20.4 / 20.4 ms|44.3%|29.3 MB|broke: camera p99 1.4 ms over 1.0 ms (3 of 3)|
|16x|ntcore-2027|109.9 µs / 1.6 ms|158.6 µs / 955.4 µs|1.3 ms / 2.3 ms|0%|-|11.5 / 19.2 ms|36.3%|100.5 MB|broke: camera p99 1.6 ms over 1.0 ms (3 of 3)|

### Rate, budget 1core

|Rate|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|1x|tarwyn|106.6 µs / 1.8 ms|346.1 µs / 1.6 ms|1.4 ms / 2.4 ms|0%|-|21.2 / 21.2 ms|4.6%|27.8 MB|pass (3 of 3)|
|1x|ntcore-2026|165.8 µs / 2.0 ms|7.2 ms / 20.3 ms|11.3 ms / 17.2 ms|0%|-|102.1 / 102.3 ms|4.8%|94.5 MB|pass (3 of 3)|
|1x|ntcore-2027|160.7 µs / 2.0 ms|223.6 µs / 1.6 ms|1.8 ms / 4.2 ms|0%|-|10.9 / 19.8 ms|4.6%|88.1 MB|pass (3 of 3)|
|2x|tarwyn|100.2 µs / 2.2 ms|327.7 µs / 1.9 ms|1.6 ms / 2.0 ms|0%|-|21.4 / 21.4 ms|8.1%|28.7 MB|pass (3 of 3)|
|2x|ntcore-2026|152.4 µs / 1.9 ms|10.2 ms / 20.3 ms|7.8 ms / 22.2 ms|0%|-|101.6 / 101.8 ms|8.2%|94.5 MB|pass (3 of 3)|
|2x|ntcore-2027|149.3 µs / 2.0 ms|214.5 µs / 1.8 ms|1.5 ms / 2.2 ms|0%|-|10.8 / 19.4 ms|7.9%|88.5 MB|pass (3 of 3)|
|4x|tarwyn|98.9 µs / 2.3 ms|322.1 µs / 2.3 ms|1.4 ms / 2.2 ms|0%|-|23.3 / 23.3 ms|14.5%|27.9 MB|pass (3 of 3)|
|4x|ntcore-2026|1.4 ms / 6.1 ms|10.2 ms / 20.3 ms|11.4 ms / 23.2 ms|0%|-|101.8 / 102.0 ms|13.2%|95.5 MB|broke: camera p99 6.1 ms over 4.2 ms (3 of 3)|
|4x|ntcore-2027|140.6 µs / 2.4 ms|194.8 µs / 2.1 ms|1.6 ms / 2.4 ms|0%|-|10.4 / 19.1 ms|13.5%|88.8 MB|pass (3 of 3)|
|8x|tarwyn|96.1 µs / 2.3 ms|282.9 µs / 2.2 ms|1.9 ms / 3.7 ms|0%|-|25.8 / 25.8 ms|23.5%|27.4 MB|broke: camera p99 2.4 ms over 2.1 ms (3 of 3)|
|8x|ntcore-2027|132.2 µs / 2.6 ms|181.1 µs / 2.1 ms|1.5 ms / 2.3 ms|0%|-|11.2 / 19.7 ms|22.5%|90.2 MB|broke: camera p99 2.6 ms over 2.1 ms (3 of 3)|

### Rate, budget 25pct

|Rate|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|1x|tarwyn|120.1 µs / 3.2 ms|297.0 µs / 3.1 ms|1.7 ms / 1.7 ms|0%|-|71.4 / 71.4 ms|4.1%|27.5 MB|pass (3 of 3)|
|1x|ntcore-2026|175.5 µs / 2.0 ms|12.0 ms / 20.6 ms|24.3 ms / 25.3 ms|0%|-|102.5 / 102.7 ms|4.8%|94.1 MB|pass (3 of 3)|
|1x|ntcore-2027|175.4 µs / 19.2 ms|239.9 µs / 13.2 ms|1.9 ms / 13.8 ms|0%|-|31.5 / 59.5 ms|4.6%|88.2 MB|broke: camera p99 17.8 ms over 16.7 ms (3 of 3)|
|2x|tarwyn|115.5 µs / 2.9 ms|287.7 µs / 4.2 ms|3.0 ms / 3.1 ms|0%|-|79.0 / 79.0 ms|7.2%|27.7 MB|pass (3 of 3)|
|2x|ntcore-2026|153.0 µs / 2.2 ms|10.3 ms / 20.2 ms|17.8 ms / 25.6 ms|0%|-|102.2 / 102.4 ms|8.1%|94.2 MB|pass (3 of 3)|
|4x|tarwyn|106.4 µs / 7.7 ms|273.1 µs / 1847.6 ms|2.3 ms / 4.0 ms|0%|-|100.0 / 100.0 ms|12.5%|27.3 MB|broke: camera p99 7.7 ms over 4.2 ms (3 of 3)|
|4x|ntcore-2026|1.2 ms / 5.9 ms|10.2 ms / 20.3 ms|15.3 ms / 25.1 ms|0%|-|102.1 / 102.3 ms|12.8%|93.0 MB|broke: camera p99 6.1 ms over 4.2 ms (3 of 3)|

### Clients

|Extra dashboards|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|+0|tarwyn|111.1 µs / 1.8 ms|196.1 µs / 477.7 µs|574.0 µs / 760.3 µs|0%|-|19.7 / 19.7 ms|4.4%|27.4 MB|pass (3 of 3)|
|+0|ntcore-2026|159.2 µs / 1.8 ms|12.3 ms / 19.7 ms|15.1 ms / 20.0 ms|0%|-|102.3 / 102.5 ms|4.6%|93.9 MB|pass (3 of 3)|
|+0|ntcore-2027|155.5 µs / 2.1 ms|229.6 µs / 1.1 ms|1.7 ms / 4.3 ms|0%|-|11.1 / 19.9 ms|4.5%|86.5 MB|pass (3 of 3)|
|+2|tarwyn|112.0 µs / 1.6 ms|225.8 µs / 1.0 ms|1.1 ms / 3.0 ms|0%|-|22.2 / 22.2 ms|6.0%|29.4 MB|pass (3 of 3)|
|+2|ntcore-2026|162.1 µs / 1.8 ms|17.0 ms / 20.2 ms|18.1 ms / 19.7 ms|0%|-|102.0 / 102.2 ms|4.8%|96.8 MB|pass (3 of 3)|
|+2|ntcore-2027|156.0 µs / 2.0 ms|326.9 µs / 1.3 ms|1.3 ms / 4.6 ms|0%|-|11.9 / 20.3 ms|7.2%|89.0 MB|pass (2 of 3)|
|+4|tarwyn|113.0 µs / 1.8 ms|310.3 µs / 1.3 ms|1.9 ms / 2.2 ms|0%|-|24.7 / 24.7 ms|7.5%|32.9 MB|pass (3 of 3)|
|+4|ntcore-2026|161.4 µs / 1.8 ms|6.4 ms / 20.4 ms|20.1 ms / 20.4 ms|0%|-|102.1 / 102.3 ms|5.1%|95.5 MB|pass (3 of 3)|
|+4|ntcore-2027|162.4 µs / 2.1 ms|441.1 µs / 1.7 ms|2.8 ms / 5.0 ms|0%|-|11.3 / 19.7 ms|9.5%|87.6 MB|pass (3 of 3)|
|+8|tarwyn|114.4 µs / 2.1 ms|483.8 µs / 1.6 ms|3.7 ms / 4.1 ms|0%|-|22.6 / 22.6 ms|10.5%|37.0 MB|pass (3 of 3)|
|+8|ntcore-2026|163.5 µs / 2.0 ms|11.2 ms / 20.4 ms|20.1 ms / 20.6 ms|0%|-|102.3 / 102.4 ms|5.5%|121.9 MB|pass (3 of 3)|
|+8|ntcore-2027|166.4 µs / 2.3 ms|673.8 µs / 2.7 ms|4.7 ms / 5.5 ms|0%|-|11.3 / 19.7 ms|13.9%|98.0 MB|pass (3 of 3)|
|+16|tarwyn|116.3 µs / 2.4 ms|814.1 µs / 3.6 ms|6.7 ms / 7.0 ms|0%|-|28.9 / 28.9 ms|16.2%|43.9 MB|pass (2 of 3)|
|+16|ntcore-2026|163.9 µs / 1.9 ms|9.6 ms / 20.5 ms|11.4 ms / 20.7 ms|0%|-|102.2 / 102.4 ms|6.3%|128.9 MB|pass (3 of 3)|
|+16|ntcore-2027|180.2 µs / 4.1 ms|1.1 ms / 4.2 ms|7.5 ms / 8.7 ms|0%|-|12.3 / 21.0 ms|22.5%|100.6 MB|broke: robot exited (2 of 3)|

