# Benchmark Results

Latency is one way, in microseconds. [BENCHMARK.md](BENCHMARK.md) explains how it is measured and how to read the tables, and `results.json` beside this file has every percentile. Regenerate with `bench sweep`.

|  |  |
|---|---|
|date|2026-09-22|
|machine|linux 7.2.6-arch2-1, AMD Ryzen 5 5600X 6-Core Processor, 12 logical cpus|
|state|governor powersave, boost on, load 0.75 at start|
|pinning|publisher on cpu 1, subscriber on cpu 2, server on cpus 3,4,5|
|commit|cad5996-dirty|
|measured|50 and 500 Hz, 3000 samples after 500 warmup, 3 reps|
|implementations|ntcore=2027.0.0a7, tarwyn=0.1.1|

## Clients

### 50 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|55.3|51.9 to 58.9|238.8|0.8%|
|subscribe, 3 subscribers|ntcore|168.7|166.5 to 189.3|588.3|0.2%|
|publish|tarwyn|55.7|45.0 to 56.6|262.9|0.8%|
|publish|ntcore|144.4|142.3 to 145.3|591.9|0.1%|
|subscribe|tarwyn|43.2|42.0 to 44.2|255.7|0.7%|
|subscribe|ntcore|142.8|136.3 to 147.1|779.8|0.1%|

- `subscribe, 3 subscribers`: tarwyn 3.0x faster than ntcore on the median and 2.5x on the p99.
- `publish`: tarwyn 2.6x faster than ntcore on the median and 2.3x on the p99.
- `subscribe`: tarwyn 3.3x faster than ntcore on the median and 3.0x on the p99.

### 50 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|55.7|52.2 to 58.4|228.2|0.8%|
|subscribe, 3 subscribers|ntcore|170.9|150.8 to 172.8|602.1|0.2%|
|publish|tarwyn|57.0|56.5 to 57.7|343.6|0.8%|
|publish|ntcore|144.8|142.6 to 148.1|824.8|0.1%|
|subscribe|tarwyn|44.1|43.1 to 48.2|211.5|0.7%|
|subscribe|ntcore|141.6|136.2 to 148.1|726.0|0.1%|

- `subscribe, 3 subscribers`: tarwyn 3.1x faster than ntcore on the median and 2.6x on the p99.
- `publish`: tarwyn 2.5x faster than ntcore on the median and 2.4x on the p99.
- `subscribe`: tarwyn 3.2x faster than ntcore on the median and 3.4x on the p99.

### 50 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|68.3|56.0 to 95.4|310.0|0.8%|
|subscribe, 3 subscribers|ntcore|167.7|160.0 to 202.8|733.7|0.2%|
|publish|tarwyn|49.6|49.3 to 49.7|223.0|0.8%|
|publish|ntcore|135.3|134.5 to 140.5|326.4|0.1%|
|subscribe|tarwyn|34.2|34.0 to 34.5|215.2|0.7%|
|subscribe|ntcore|141.3|139.9 to 142.6|480.5|0.1%|

- `subscribe, 3 subscribers`: tarwyn 2.5x faster than ntcore on the median and 2.4x on the p99.
- `publish`: tarwyn 2.7x faster than ntcore on the median and 1.5x on the p99.
- `subscribe`: tarwyn 4.1x faster than ntcore on the median and 2.2x on the p99.

### 500 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|33.3|32.2 to 33.8|53.7|7.3%|
|subscribe, 3 subscribers|ntcore|129.8|127.7 to 131.2|307.7|1.3%|
|publish|tarwyn|24.5|24.0 to 25.1|48.3|6.9%|
|publish|ntcore|103.4|97.6 to 107.2|365.3|0.8%|
|subscribe|tarwyn|21.2|20.6 to 21.2|39.4|6.9%|
|subscribe|ntcore|103.4|98.6 to 103.5|319.2|0.8%|

- `subscribe, 3 subscribers`: tarwyn 3.9x faster than ntcore on the median and 5.7x on the p99.
- `publish`: tarwyn 4.2x faster than ntcore on the median and 7.6x on the p99.
- `subscribe`: tarwyn 4.9x faster than ntcore on the median and 8.1x on the p99.

### 500 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|33.1|32.1 to 34.1|53.0|7.5%|
|subscribe, 3 subscribers|ntcore|129.2|128.2 to 129.3|301.6|1.1%|
|publish|tarwyn|25.7|25.6 to 25.9|46.0|6.9%|
|publish|ntcore|104.5|103.6 to 105.0|645.6|0.8%|
|subscribe|tarwyn|21.1|20.8 to 22.4|41.2|7.1%|
|subscribe|ntcore|104.2|99.7 to 104.9|411.6|0.8%|

- `subscribe, 3 subscribers`: tarwyn 3.9x faster than ntcore on the median and 5.7x on the p99.
- `publish`: tarwyn 4.1x faster than ntcore on the median and 14.0x on the p99.
- `subscribe`: tarwyn 4.9x faster than ntcore on the median and 10.0x on the p99.

### 500 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|subscribe, 3 subscribers|tarwyn|36.8|33.0 to 38.4|58.2|7.6%|
|subscribe, 3 subscribers|ntcore|129.3|126.6 to 131.7|328.7|1.3%|
|publish|tarwyn|21.4|20.3 to 22.7|36.5|7.1%|
|publish|ntcore|100.4|100.3 to 103.9|308.5|0.8%|
|subscribe|tarwyn|19.4|18.8 to 21.0|37.7|7.2%|
|subscribe|ntcore|103.2|102.1 to 104.4|352.0|0.8%|

- `subscribe, 3 subscribers`: tarwyn 3.5x faster than ntcore on the median and 5.6x on the p99.
- `publish`: tarwyn 4.7x faster than ntcore on the median and 8.4x on the p99.
- `subscribe`: tarwyn 5.3x faster than ntcore on the median and 9.3x on the p99.

## Servers

### 50 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|53.1|46.1 to 59.8|243.2|0.7%|
|publish|ntcore|74.5|61.2 to 79.5|267.0|0.2%|

- `publish`: tarwyn 1.4x faster than ntcore on the median and 1.1x on the p99.

### 50 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|60.4|49.1 to 60.7|237.4|0.7%|
|publish|ntcore|81.7|80.5 to 90.9|340.5|0.2%|

- `publish`: tarwyn 1.4x faster than ntcore on the median and 1.4x on the p99.

### 50 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|48.7|42.8 to 49.7|225.7|0.7%|
|publish|ntcore|72.8|65.7 to 89.5|471.6|0.2%|

- `publish`: tarwyn 1.5x faster than ntcore on the median and 2.1x on the p99.

### 500 Hz, 16 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|25.7|23.3 to 25.7|44.0|6.9%|
|publish|ntcore|33.5|33.0 to 34.7|223.2|0.8%|

- `publish`: tarwyn 1.3x faster than ntcore on the median and 5.1x on the p99.

### 500 Hz, 96 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|25.5|25.4 to 26.2|53.7|6.8%|
|publish|ntcore|34.3|34.1 to 36.2|72.7|0.8%|

- `publish`: tarwyn 1.3x faster than ntcore on the median and 1.4x on the p99.

### 500 Hz, 1024 B

|Operation|Implementation|Median|Range|P99|Server CPU|
|---|---|---|---|---|---|
|publish|tarwyn|23.2|22.5 to 24.8|49.4|7.1%|
|publish|ntcore|35.5|33.8 to 36.2|65.1|0.8%|

- `publish`: tarwyn 1.5x faster than ntcore on the median and 1.3x on the p99.

Not measured: more than 3 subscribers, rates other than 50 and 500 Hz, payloads other than 16 B, 96 B and 1024 B, traffic that crosses a network, memory.

<!-- bench sim -->
## Robot load: team488-2026

The profile was captured from Team488/TeamXbot2026@55fa6f0 in simulation, in autonomous. Run 2026-10-17 on AMD Ryzen 5 5600X 6-Core Processor, 12 logical cpus, commit 95101f9-dirty, with 5 s of warmup and 10 s measured per step and 1 reps. Implementations: tarwyn=0.1.1, ntcore-2026=2026.2.2, ntcore-2027=2027.0.0a7. [BENCHMARK.md](BENCHMARK.md#bench-sim) explains the columns and verdicts.

### Headroom

|Implementation|Budget|Passed up to|Stopped by|
|---|---|---|---|
|ntcore-2026|none|2x|4x: broke: camera p99 4.7 ms over 4.2 ms|
|ntcore-2027|none|4x|8x: broke: camera p99 2.3 ms over 2.1 ms|
|tarwyn|none|8x|16x: broke: camera p99 1.1 ms over 1.0 ms|
|ntcore-2026|25pct|2x|4x: broke: camera p99 5.8 ms over 4.2 ms|
|ntcore-2027|25pct|-|1x: broke: camera p99 53.0 ms over 16.7 ms|
|tarwyn|25pct|4x|8x: broke: camera p99 4.2 ms over 2.1 ms|

### Rate, budget none

|Rate|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|1x|tarwyn|100.2 µs / 1.1 ms|226.4 µs / 389.9 µs|-|0%|-|timeout|16.2%|18.3 MB|pass (1 of 1)|
|1x|ntcore-2026|134.1 µs / 1.7 ms|18.7 ms / 20.3 ms|-|0%|-|101.9 / 102.0 ms|2.8%|121.7 MB|pass (1 of 1)|
|1x|ntcore-2027|129.7 µs / 3.0 ms|205.2 µs / 366.3 µs|-|0%|-|11.0 / 18.8 ms|3.2%|95.8 MB|pass (1 of 1)|
|2x|tarwyn|92.9 µs / 978.4 µs|211.6 µs / 441.6 µs|697.9 µs / 873.0 µs|0%|-|timeout|29.9%|18.5 MB|pass (1 of 1)|
|2x|ntcore-2026|123.9 µs / 957.5 µs|2.7 ms / 20.3 ms|20.1 ms / 20.3 ms|0%|-|102.0 / 102.1 ms|4.4%|126.4 MB|pass (1 of 1)|
|2x|ntcore-2027|122.1 µs / 1.7 ms|186.4 µs / 3.6 ms|3.4 ms / 3.6 ms|0%|-|10.4 / 18.1 ms|4.8%|96.2 MB|pass (1 of 1)|
|4x|tarwyn|89.6 µs / 985.6 µs|210.3 µs / 530.9 µs|361.0 µs / 555.0 µs|0%|-|timeout|51.9%|18.7 MB|pass (1 of 1)|
|4x|ntcore-2026|997.0 µs / 4.7 ms|10.1 ms / 20.1 ms|10.0 ms / 18.5 ms|0%|-|101.3 / 101.5 ms|6.8%|126.2 MB|broke: camera p99 4.7 ms over 4.2 ms (1 of 1)|
|4x|ntcore-2027|116.0 µs / 2.2 ms|174.6 µs / 1.6 ms|1.3 ms / 1.4 ms|0%|-|10.6 / 18.4 ms|8.0%|96.0 MB|pass (1 of 1)|
|8x|tarwyn|87.1 µs / 1.1 ms|209.4 µs / 546.3 µs|432.9 µs / 537.1 µs|0%|-|timeout|90.2%|18.6 MB|pass (1 of 1)|
|8x|ntcore-2027|105.8 µs / 2.3 ms|161.8 µs / 1.6 ms|1.4 ms / 7.6 ms|0%|-|10.4 / 20.0 ms|13.1%|96.6 MB|broke: camera p99 2.3 ms over 2.1 ms (1 of 1)|
|16x|tarwyn|87.3 µs / 1.1 ms|199.2 µs / 551.4 µs|631.8 µs / 1.4 ms|0%|-|timeout|126.0%|18.9 MB|broke: camera p99 1.1 ms over 1.0 ms (1 of 1)|

### Rate, budget 25pct

|Rate|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|1x|tarwyn|107.7 µs / 1.9 ms|355.6 µs / 1.3 ms|-|0%|-|timeout|14.9%|18.5 MB|pass (1 of 1)|
|1x|ntcore-2026|139.7 µs / 2.7 ms|13.1 ms / 20.1 ms|-|0%|-|102.4 / 102.6 ms|2.9%|93.7 MB|pass (1 of 1)|
|1x|ntcore-2027|142.4 µs / 53.0 ms|217.2 µs / 54.9 ms|-|0%|-|30.4 / 60.8 ms|3.4%|87.8 MB|broke: camera p99 53.0 ms over 16.7 ms (1 of 1)|
|2x|tarwyn|122.1 µs / 3.3 ms|420.1 µs / 3.6 ms|-|0%|-|timeout|24.2%|18.6 MB|pass (1 of 1)|
|2x|ntcore-2026|124.2 µs / 1.1 ms|4.9 ms / 20.3 ms|22.2 ms / 22.3 ms|0%|-|102.2 / 102.3 ms|4.5%|93.8 MB|pass (1 of 1)|
|4x|tarwyn|232.3 µs / 3.8 ms|560.6 µs / 4.1 ms|3.7 ms / 3.8 ms|0%|-|timeout|24.9%|18.5 MB|pass (1 of 1)|
|4x|ntcore-2026|1.4 ms / 5.8 ms|10.2 ms / 20.6 ms|14.8 ms / 14.9 ms|0%|-|101.9 / 102.1 ms|8.0%|94.3 MB|broke: camera p99 5.8 ms over 4.2 ms (1 of 1)|
|8x|tarwyn|724.5 µs / 4.2 ms|1.0 ms / 5.2 ms|3.8 ms / 4.0 ms|0%|-|timeout|24.9%|18.6 MB|broke: camera p99 4.2 ms over 2.1 ms (1 of 1)|

### Clients

|Extra dashboards|Implementation|Camera p50 / p99|Struct p50 / p99|Slow p50 / p99|Loss|Stale p99|Connect announce / values|CPU|RSS|Verdict|
|---|---|---|---|---|---|---|---|---|---|---|
|+0|tarwyn|96.7 µs / 1.2 ms|224.2 µs / 454.4 µs|1.3 ms / 2.9 ms|0%|-|timeout|16.6%|18.5 MB|pass (1 of 1)|
|+0|ntcore-2026|129.6 µs / 1.1 ms|19.3 ms / 20.4 ms|-|0%|-|101.8 / 102.0 ms|2.6%|126.0 MB|pass (1 of 1)|
|+0|ntcore-2027|123.7 µs / 2.0 ms|208.8 µs / 270.3 µs|-|0%|-|10.8 / 18.7 ms|3.2%|96.3 MB|pass (1 of 1)|
|+4|tarwyn|94.9 µs / 574.0 µs|322.1 µs / 3.1 ms|5.1 ms / 5.3 ms|0%|-|timeout|19.2%|21.1 MB|pass (1 of 1)|
|+4|ntcore-2026|130.6 µs / 1.2 ms|3.3 ms / 20.7 ms|-|0%|-|101.6 / 101.8 ms|2.9%|123.7 MB|pass (1 of 1)|
|+4|ntcore-2027|121.4 µs / 2.1 ms|398.3 µs / 871.9 µs|-|0%|-|10.3 / 17.9 ms|5.3%|96.5 MB|pass (1 of 1)|
|+16|tarwyn|98.6 µs / 1.8 ms|821.8 µs / 3.1 ms|-|0%|-|timeout|22.1%|29.5 MB|pass (1 of 1)|
|+16|ntcore-2026|127.5 µs / 1.5 ms|6.7 ms / 20.9 ms|-|0%|-|101.7 / 101.9 ms|3.7%|132.8 MB|pass (1 of 1)|
|+16|ntcore-2027|135.3 µs / 4.1 ms|1.1 ms / 5.6 ms|-|0%|-|11.6 / 19.8 ms|12.0%|99.1 MB|pass (1 of 1)|

