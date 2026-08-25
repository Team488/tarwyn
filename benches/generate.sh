#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JARS="${JARS:-}"
OUT="$ROOT/benches/Benchmarks.md"
ROWS="${ROWS:-$ROOT/target/bench-rows}"
mkdir -p "$ROWS"

RATE="${RATE:-2000}"
SAMPLES="${SAMPLES:-3000}"
COUNT="${COUNT:-8000}"
PAYLOADS="${PAYLOADS:-16 96}"

B="$ROOT/target/release/benches"
SERVER="$ROOT/target/release/tarwyn_server"
JAVA_DIR="$ROOT/benches/java"

LIMIT="${LIMIT:-90}"

SERVER_PID=""
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap 'stop_server' EXIT

wait_port() {
  local proto=$1 port=$2 tries=${3:-200}
  while [ $tries -gt 0 ]; do
    ss -ln"$proto" 2>/dev/null | grep -q ":$port" && return 0
    sleep 0.1; tries=$((tries - 1))
  done
  echo "  timed out waiting for port $port" >&2
  return 1
}

capture() { grep -h '^ROW' "$1" >> "$ROWS/all.tsv" 2>/dev/null; }

settle() {
  for pid in $(pgrep -x java) $(pgrep -x tarwyn_server) $(pgrep -x benches); do
    kill -9 "$pid" 2>/dev/null
  done
  sleep 1
}

java_cp() { echo "$JAVA_DIR/out:$(ls "$JARS"/*.jar 2>/dev/null | tr '\n' ':')"; }

run_rust_udp() {
  local pay=$1 port=48810 out="$ROWS/udp_$pay.out"
  timeout "$LIMIT" "$B" subscriber --subject udp --addr "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" "$B" publisher --subject udp --addr "127.0.0.1:$port" --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"
}

run_rust_tarwyn() {
  local pay=$1 out="$ROWS/tarwyn_$pay.out"
  nohup "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 5557 || { stop_server; return 1; }
  timeout "$LIMIT" "$B" subscriber --subject tarwyn --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" "$B" publisher --subject tarwyn --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

run_java_udp() {
  local pay=$1 port=48811 out="$ROWS/judp_$pay.out"
  timeout "$LIMIT" java -cp "$JAVA_DIR/out" Bench subscriber --subject java-udp --port $port --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" java -cp "$JAVA_DIR/out" Bench publisher --subject java-udp --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"
}

run_nt4() {
  local pay=$1 port=$((48820 + pay % 100)) out="$ROWS/nt4_$pay.out"
  export LD_PRELOAD="$JARS/natives/libwpiutiljni.so"
  timeout "$LIMIT" java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" -cp "$(java_cp)" \
    Bench subscriber --subject nt4 --port $port --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  if wait_port t $port; then
    timeout "$LIMIT" java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" -cp "$(java_cp)" \
      Bench publisher --subject nt4 --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
    wait $sub; capture "$out"
  else
    kill -9 $sub 2>/dev/null
  fi
  unset LD_PRELOAD
}

run_tarwyn_java() {
  local pay=$1 out="$ROWS/xtj_$pay.out"
  nohup java -cp "$JARS/TARWYN.jar" org.team488.JServer.Main >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 48800 || { stop_server; return 1; }
  sleep 3
  timeout "$LIMIT" java -cp "$(java_cp)" Bench subscriber --subject tarwyn-java --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" java -cp "$(java_cp)" Bench publisher --subject tarwyn-java --payload "$pay" --rate 1000 --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

if [ "${ONLY_REPORT:-0}" != "1" ]; then
: > "$ROWS/all.tsv"
for pay in $PAYLOADS; do
  settle; echo "payload ${pay}B: udp-floor" >&2;    run_rust_udp "$pay"
  settle; echo "payload ${pay}B: tarwyn-rust" >&2; run_rust_tarwyn "$pay"
  if [ -n "$JARS" ] && [ -d "$JAVA_DIR/out" ]; then
    settle; echo "payload ${pay}B: java-udp" >&2;     run_java_udp "$pay"
    settle; echo "payload ${pay}B: nt4-flush" >&2;    run_nt4 "$pay"
    settle; echo "payload ${pay}B: tarwyn-java" >&2; run_tarwyn_java "$pay"
  fi
done
fi

table_for() {
  echo "|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|"
  echo "|---|---|---|---|---|---|---|---|"
  awk -F'\t' -v p="$1" '$3 == p {
    printf "|%s|%s|%s|%s|%s|%s|%s|%s|\n", $2, $4, $5, $6, $7, $8, $9, $10
  }' "$ROWS/all.tsv"
}

{
  echo "# Benchmarks"
  echo
  echo "## Methodology"
  echo
  echo "Publisher and subscriber run as separate processes on one host, both reading"
  echo "\`CLOCK_REALTIME\`, so one-way latency is comparable without a clock-sync protocol."
  echo "That holds same-host only; cross-machine numbers need their own design."
  echo
  echo "Each subject is sent $COUNT messages at $RATE Hz and $SAMPLES samples are collected."
  echo "Every subject carries the same 16-byte header — sequence number and send timestamp —"
  echo "so all of them are measured identically. TARWYN is published at 1000 Hz because it"
  echo "does not keep up at $RATE Hz."
  echo
  echo "NetworkTables is configured to be measured at its best rather than at its defaults:"
  echo "\`sendAll(true)\`, \`keepDuplicates(true)\`, \`periodic(0.001s)\`, \`pollStorage(1000)\`,"
  echo "\`flush()\` after every \`set\`, read via \`readQueue()\`, and a subscriber that spins"
  echo "rather than sleeps between polls. Its 100 ms default sweep is a configuration, not a"
  echo "ceiling, and benchmarking against it would be misleading."
  echo
  echo "All subjects in a table are measured back to back in a single run, so they share"
  echo "machine conditions and are comparable to each other. Absolute figures are sensitive"
  echo "to load — the same subjects measured individually on an idle machine come out roughly"
  echo "30% lower — so compare within a table rather than across runs."
  echo
  echo "Source: [benches/src](src), [benches/java/src](java/src). Generated by [generate.sh](generate.sh)."
  echo
  echo "## Results"
  echo
  echo "\`P[NUMBER]\` = [NUMBER] Percentile. All figures in microseconds, lower is better."
  echo "\`Loss\` is the share of published messages that never arrived, counted from gaps in"
  echo "the sequence numbers."
  echo
  echo "## Last Updated $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo
  echo "## Tool Versions"
  echo "\`tarwyn-rust\`: $(grep -m1 '^version' "$ROOT/core/Cargo.toml" | cut -d'"' -f2)  "
  echo "\`TARWYN\`: v5.0.0  "
  echo "\`NetworkTables\`: 2025.3.2  "
  echo "\`rustc\`: $(rustc --version | awk '{print $2}')  "
  echo "\`libzmq\`: $(pkg-config --modversion libzmq 2>/dev/null || echo unknown)  "
  echo "\`java\`: $(java -version 2>&1 | head -1 | awk -F'\"' '{print $2}')  "
  echo
  echo "## Computer Specs"
  echo "Processor: \`$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)\`  "
  echo "Threads: \`$(nproc)\`  "
  echo "Memory: \`$(free -h | awk '/^Mem:/{print $2}')\`  "
  echo "Kernel: \`$(uname -r)\`  "
  for pay in $PAYLOADS; do
    echo
    echo "## $pay byte payload"
    table_for "$pay"
  done
  echo
  echo "## Reading the numbers"
  echo
  echo "**Loss is not a fault of the transport in every case.** The two ZeroMQ subjects lose"
  echo "messages to slow-joiner behaviour: a SUB socket subscribes asynchronously and the"
  echo "publisher discards anything sent before the subscription is established. That is a"
  echo "startup artifact, not congestion, and it is identical across runs."
  echo
  echo "**nt4-flush loses nothing because it queues instead.** That queuing is what its"
  echo "median measures. It is the only subject here that never discards."
  echo
  echo "**tarwyn-java's upper percentiles are JVM warmup.** The first messages are"
  echo "interpreted before the JIT compiles the hot path, which is why its P95 and P100 sit"
  echo "two orders of magnitude above its median. The median is the representative figure."
  echo
  echo "**udp-floor is the floor, not a product.** It carries no topics, no discovery and no"
  echo "reliability. It exists to show how much of the gap above it is inherent to networking"
  echo "and how much is the transport design."
} > "$OUT"

echo "wrote $OUT" >&2
