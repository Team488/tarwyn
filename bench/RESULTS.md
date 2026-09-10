# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).

## Settings

|  |  |
|---|---|
|measured|500 Hz, 3000 samples, 500 warmup, 3 reps|
|implementations|ntcore=2027.0.0a6.post4, reference, tarwyn-rust=0.1.0, tarwyn=5.0.0|

## Best effort, datagram

Not comparable with the tables above: nothing here is retransmitted, ordered or acknowledged, so read the loss column alongside the latency. `udp_floor` has no server in it at all and is the floor, not a subject.


### 16 B

|Operation|reference|tarwyn-rust|
|---|---|---|
|telemetry_publish|-|22.02 us (p99 210.30 us, loss 0.00%)|
|udp_floor|9.33 us (p99 25.39 us, loss 0.00%)|-|

### 96 B

|Operation|reference|tarwyn-rust|
|---|---|---|
|telemetry_publish|-|22.11 us (p99 95.74 us, loss 0.00%)|
|udp_floor|9.70 us (p99 20.48 us, loss 0.00%)|-|

## Client libraries

What a robot's own code gets. Every row publishes through its project's own client library, which for `ntcore` and `tarwyn` is the only way to speak their protocols at all.


### 16 B

|Operation|ntcore|tarwyn|tarwyn-rust|
|---|---|---|---|
|publish|53.21 us (p99 693.02 us, loss 0.00%)|107.55 us (p99 2941.73 us, loss 1.37%)|35.36 us (p99 114.81 us, loss 0.00%)|

### 96 B

|Operation|ntcore|tarwyn|tarwyn-rust|
|---|---|---|---|
|publish|53.44 us (p99 418.23 us, loss 0.00%)|107.94 us (p99 2974.41 us, loss 1.74%)|35.33 us (p99 114.50 us, loss 0.00%)|

## Round trip

Operations that block the caller until the server answers. The figure is the call's own wall time in the calling process, paced at the same rate as everything else so the two halves of this file stay comparable.


### 16 B

|Operation|tarwyn-rust|
|---|---|
|compare_and_set|37.60 us (p99 807.42 us, loss 0.00%)|
|delete|41.05 us (p99 97.73 us, loss 0.00%)|
|get|35.23 us (p99 231.68 us, loss 0.00%)|
|ping|33.60 us (p99 108.16 us, loss 0.00%)|
|tables|38.27 us (p99 508.42 us, loss 0.00%)|

## Servers

One client, two servers. Every row was driven by the same raw NT4 publisher and subscriber from this repo, so the only thing that differs is which server answered. `tarwyn` cannot appear here: its ZeroMQ protocol has no client but its own.


### 16 B

|Operation|ntcore|tarwyn-rust|
|---|---|---|
|publish|40.09 us (p99 98.81 us, loss 0.00%)|34.85 us (p99 127.10 us, loss 0.00%)|

### 96 B

|Operation|ntcore|tarwyn-rust|
|---|---|---|
|publish|38.53 us (p99 75.07 us, loss 0.00%)|36.03 us (p99 81.02 us, loss 0.00%)|

## Detail

Every percentile the run recorded, for readers who want more than the median.

|Section|Operation|Implementation|Payload|P0|Median|P80|P90|P95|P99|P99.9|P100|Loss (%)|Samples|
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|Best effort, datagram|telemetry_publish|tarwyn-rust|16 B|14.39|22.02|27.93|30.22|33.79|210.30|1688.58|3088.38|0.00|3000|
|Best effort, datagram|telemetry_publish|tarwyn-rust|96 B|15.58|22.11|27.47|29.95|31.70|95.74|2164.74|2834.43|0.00|3000|
|Best effort, datagram|udp_floor|reference|16 B|6.88|9.33|12.00|13.16|14.53|25.39|2012.16|2816.00|0.00|3000|
|Best effort, datagram|udp_floor|reference|96 B|7.04|9.70|11.62|12.46|13.73|20.48|1997.82|2081.79|0.00|3000|
|Client libraries|publish|ntcore|16 B|35.17|53.21|64.69|71.27|75.69|693.02|2191.49|4204.11|0.00|3000|
|Client libraries|publish|tarwyn|16 B|71.07|107.55|485.49|1138.85|1804.19|2941.73|5376.19|6926.59|1.37|3000|
|Client libraries|publish|tarwyn-rust|16 B|20.69|35.36|43.81|47.49|50.85|114.81|2404.35|4747.26|0.00|3000|
|Client libraries|publish|ntcore|96 B|34.23|53.44|65.69|71.25|78.79|418.23|2116.24|4987.97|0.00|3000|
|Client libraries|publish|tarwyn|96 B|68.84|107.94|427.28|1045.83|1808.23|2974.41|5118.50|9122.14|1.74|3000|
|Client libraries|publish|tarwyn-rust|96 B|20.72|35.33|42.49|46.37|51.10|114.50|2172.93|2856.96|0.00|3000|
|Round trip|compare_and_set|tarwyn-rust|16 B|23.20|37.60|43.30|48.67|51.17|807.42|2144.26|2850.82|0.00|3000|
|Round trip|delete|tarwyn-rust|16 B|26.42|41.05|47.45|49.25|52.19|97.73|1098.75|2854.91|0.00|3000|
|Round trip|get|tarwyn-rust|16 B|22.56|35.23|40.96|44.22|47.36|231.68|1555.45|3123.20|0.00|3000|
|Round trip|ping|tarwyn-rust|16 B|21.07|33.60|39.42|43.17|46.24|108.16|2031.62|2965.50|0.00|3000|
|Round trip|tables|tarwyn-rust|16 B|23.81|38.27|43.74|48.00|54.30|508.42|1977.34|2854.91|0.00|3000|
|Servers|publish|ntcore|16 B|24.96|40.09|50.14|54.40|57.85|98.81|1531.90|2252.80|0.00|3000|
|Servers|publish|tarwyn-rust|16 B|20.13|34.85|42.78|47.42|50.34|127.10|2031.62|3377.15|0.00|3000|
|Servers|publish|ntcore|96 B|24.56|38.53|47.90|52.13|55.39|75.07|2637.82|2865.15|0.00|3000|
|Servers|publish|tarwyn-rust|96 B|19.97|36.03|44.29|48.38|51.07|81.02|2042.88|2330.62|0.00|3000|

## Run-to-run spread

How far the median moved between runs of the same row. A difference smaller than the spread here is noise, not a result.

|Section|Operation|Implementation|Payload|Runs|Lowest median|Highest median|Spread (%)|
|---|---|---|---|---|---|---|---|
|Best effort, datagram|telemetry_publish|tarwyn-rust|16 B|3|21.97|23.95|9.0|
|Best effort, datagram|telemetry_publish|tarwyn-rust|96 B|3|22.02|22.45|1.9|
|Best effort, datagram|udp_floor|reference|16 B|3|9.11|10.65|16.5|
|Best effort, datagram|udp_floor|reference|96 B|3|8.89|10.30|14.5|
|Client libraries|publish|ntcore|16 B|3|52.45|62.61|19.1|
|Client libraries|publish|tarwyn|16 B|3|105.72|119.00|12.3|
|Client libraries|publish|tarwyn-rust|16 B|3|35.26|41.44|17.5|
|Client libraries|publish|ntcore|96 B|3|52.18|54.35|4.1|
|Client libraries|publish|tarwyn|96 B|3|102.64|111.22|7.9|
|Client libraries|publish|tarwyn-rust|96 B|3|34.21|35.33|3.2|
|Round trip|compare_and_set|tarwyn-rust|16 B|3|35.52|39.97|11.8|
|Round trip|delete|tarwyn-rust|16 B|3|38.98|42.34|8.2|
|Round trip|get|tarwyn-rust|16 B|3|33.44|41.02|21.5|
|Round trip|ping|tarwyn-rust|16 B|3|31.66|37.09|16.2|
|Round trip|tables|tarwyn-rust|16 B|3|31.77|38.49|17.6|
|Servers|publish|ntcore|16 B|3|38.49|41.98|8.7|
|Servers|publish|tarwyn-rust|16 B|3|34.85|34.88|0.1|
|Servers|publish|ntcore|96 B|3|38.46|41.18|7.1|
|Servers|publish|tarwyn-rust|96 B|3|34.72|39.39|13.0|

## Coordinated omission check

Corrected figures refill the samples a stall swallowed, assuming the send schedule. A corrected figure far above the raw one means the run hit stalls the raw percentiles cannot show. Harnesses that report no correction are left out.

|Section|Operation|Implementation|Payload|Median|Corrected median|P99|Corrected P99|Achieved (Hz)|
|---|---|---|---|---|---|---|---|---|
|Best effort, datagram|telemetry_publish|tarwyn-rust|16 B|22.02|22.02|210.30|210.30|500.0|
|Best effort, datagram|telemetry_publish|tarwyn-rust|96 B|22.11|22.11|95.74|95.74|500.0|
|Best effort, datagram|udp_floor|reference|16 B|9.33|9.33|25.39|25.39|500.0|
|Best effort, datagram|udp_floor|reference|96 B|9.70|9.70|20.48|20.48|500.0|
|Client libraries|publish|tarwyn-rust|16 B|35.36|35.36|114.81|114.81|500.0|
|Client libraries|publish|tarwyn-rust|96 B|35.33|35.33|114.50|114.50|500.0|
|Round trip|compare_and_set|tarwyn-rust|16 B|37.60|37.60|807.42|807.42|500.0|
|Round trip|delete|tarwyn-rust|16 B|41.05|41.05|97.73|97.73|500.0|
|Round trip|get|tarwyn-rust|16 B|35.23|35.23|231.68|231.68|500.0|
|Round trip|ping|tarwyn-rust|16 B|33.60|33.60|108.16|108.16|500.0|
|Round trip|tables|tarwyn-rust|16 B|38.27|38.27|508.42|508.42|500.0|
|Servers|publish|ntcore|16 B|40.09|40.09|98.81|98.81|500.0|
|Servers|publish|tarwyn-rust|16 B|34.85|34.85|127.10|127.10|500.0|
|Servers|publish|ntcore|96 B|38.53|38.53|75.07|75.07|500.0|
|Servers|publish|tarwyn-rust|96 B|36.03|36.03|81.02|81.02|500.0|
