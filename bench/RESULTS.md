# Benchmark results

Regenerate with `bench/generate.sh`; see [BENCHMARK.md](BENCHMARK.md).

## Run conditions

|  |  |
|---|---|
|machine|AMD Ryzen 5 5600X 6-Core Processor, kernel 7.2.4-arch1-2|
|scaling|powersave, boost on|
|load average|0.97 at start|
|measured|500 Hz, 3000 samples, 500 warmup, 3 reps|
|implementations|ntcore, reference, tarwyn, tarwyn-rust, tarwyn-rust-client|

## best-effort


### 16 B

|Operation|reference|
|---|---|
|udp_floor|9.16 us (p99 19.41 us, loss 0.00%)|

### 96 B

|Operation|reference|
|---|---|
|udp_floor|10.06 us (p99 26.88 us, loss 0.00%)|

## delivery


### 16 B

|Operation|ntcore|tarwyn|tarwyn-rust|tarwyn-rust-client|
|---|---|---|---|---|
|publish|52.75 us (p99 823.67 us, loss 0.00%)|102.67 us (p99 3058.72 us, loss 1.27%)|36.06 us (p99 227.97 us, loss 0.00%)|34.59 us (p99 125.63 us, loss 0.00%)|

### 96 B

|Operation|ntcore|tarwyn|tarwyn-rust|tarwyn-rust-client|
|---|---|---|---|---|
|publish|52.67 us (p99 643.27 us, loss 0.00%)|110.23 us (p99 3051.04 us, loss 1.17%)|34.62 us (p99 90.43 us, loss 0.00%)|37.38 us (p99 105.53 us, loss 0.00%)|

## round-trip


### 16 B

|Operation|tarwyn-rust-client|
|---|---|
|get|35.42 us (p99 112.00 us, loss 0.00%)|
|ping|33.44 us (p99 69.06 us, loss 0.00%)|
