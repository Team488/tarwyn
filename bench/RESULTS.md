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
