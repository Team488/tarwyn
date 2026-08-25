#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JARS="${JARS:-}"
OUT="$ROOT/benches/Benchmarks.md"
ROWS="${ROWS:-$ROOT/target/bench-rows}"
mkdir -p "$ROWS"

RATE="${RATE:-500}"
SAMPLES="${SAMPLES:-3000}"
WARMUP="${WARMUP:-500}"
export BENCH_WARMUP="$WARMUP"
COUNT="${COUNT:-12000}"
PAYLOADS="${PAYLOADS:-16 96}"
SUBJECTS="${SUBJECTS:-tarwyn-rust nt4 tarwyn-java}"

has() { case " $SUBJECTS " in *" $1 "*) return 0;; *) return 1;; esac; }

PIN="${PIN:-1}"
if [ "$PIN" = "1" ] && command -v taskset >/dev/null 2>&1 && [ "$(nproc)" -ge 6 ]; then
  PIN_SERVER="taskset -c 2"
  PIN_PUB="taskset -c 4"
  PIN_SUB="taskset -c 6"
else
  PIN_SERVER=""
  PIN_PUB=""
  PIN_SUB=""
fi

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
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject udp --addr "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject udp --addr "127.0.0.1:$port" --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"
}

run_zmq_direct() {
  local pay=$1 out="$ROWS/zmqd_$pay.out"
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject zmq-direct --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject zmq-direct --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"
}

run_rust_tarwyn_udp() {
  local pay=$1 out="$ROWS/xtudp_$pay.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 5557 || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject tarwyn-udp --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject tarwyn-udp --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

run_rust_tarwyn() {
  local pay=$1 out="$ROWS/tarwyn_$pay.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 5557 || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject tarwyn --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject tarwyn --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

run_java_udp() {
  local pay=$1 port=48811 out="$ROWS/judp_$pay.out"
  timeout "$LIMIT" $PIN_SUB java -cp "$JAVA_DIR/out" Bench subscriber --subject java-udp --port $port --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" $PIN_PUB java -cp "$JAVA_DIR/out" Bench publisher --subject java-udp --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"
}

run_nt4() {
  local pay=$1 port=$((48820 + pay % 100)) out="$ROWS/nt4_$pay.out"
  export LD_PRELOAD="$JARS/natives/libwpiutiljni.so"
  timeout "$LIMIT" $PIN_SUB java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" -cp "$(java_cp)" \
    Bench subscriber --subject nt4 --port $port --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  if wait_port t $port; then
    timeout "$LIMIT" $PIN_PUB java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" -cp "$(java_cp)" \
      Bench publisher --subject nt4 --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
    wait $sub; capture "$out"
  else
    kill -9 $sub 2>/dev/null
  fi
  unset LD_PRELOAD
}

run_tarwyn_java() {
  local pay=$1 out="$ROWS/xtj_$pay.out"
  nohup $PIN_SERVER java -cp "$JARS/TARWYN.jar" org.team488.JServer.Main >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 48800 || { stop_server; return 1; }
  sleep 3
  timeout "$LIMIT" $PIN_SUB java -cp "$(java_cp)" Bench subscriber --subject tarwyn-java --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB java -cp "$(java_cp)" Bench publisher --subject tarwyn-java --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

if [ "${ONLY_REPORT:-0}" != "1" ]; then
: > "$ROWS/all.tsv"
for pay in $PAYLOADS; do
  has udp-floor    && { settle; echo "payload ${pay}B: udp-floor" >&2;    run_rust_udp "$pay"; }
  has zmq-direct   && { settle; echo "payload ${pay}B: zmq-direct" >&2;   run_zmq_direct "$pay"; }
  has tarwyn-zmq  && { settle; echo "payload ${pay}B: tarwyn-zmq" >&2;  run_rust_tarwyn "$pay"; }
  has tarwyn-rust && { settle; echo "payload ${pay}B: tarwyn-rust" >&2; run_rust_tarwyn_udp "$pay"; }
  if [ -n "$JARS" ] && [ -d "$JAVA_DIR/out" ]; then
    has java-udp     && { settle; echo "payload ${pay}B: java-udp" >&2;     run_java_udp "$pay"; }
    has nt4          && { settle; echo "payload ${pay}B: nt4" >&2;          run_nt4 "$pay"; }
    has tarwyn-java && { settle; echo "payload ${pay}B: tarwyn-java" >&2; run_tarwyn_java "$pay"; }
  fi
done
fi

table_for() {
  echo "|Subject (us)|Median|P0|P80|P90|P95|P100|Loss (%)|"
  echo "|---|---|---|---|---|---|---|---|"
  awk -F'\t' -v p="$1" '$3 == p {
    printf "%s\t|%s|%s|%s|%s|%s|%s|%s|%s|\n", $4, $2, $4, $5, $6, $7, $8, $9, $10
  }' "$ROWS/all.tsv" | sort -g -k1,1 | cut -f2-
}

{
  echo "# Benchmarks"
  echo
  echo "Publisher and subscriber as separate processes on one host, both reading"
  echo "\`CLOCK_REALTIME\`. Same-host only; cross-machine numbers need clock sync."
  echo
  echo "Every subject runs at the same rate with the same 16-byte header, pinned to separate"
  echo "cores. The first $WARMUP received messages are discarded so figures are steady state,"
  echo "not JIT warmup. A subject that cannot keep up shows it under Loss rather than being"
  echo "given an easier run."
  echo
  echo "NetworkTables is configured at its best, not its 100 ms default: \`sendAll(true)\`,"
  echo "\`keepDuplicates(true)\`, \`periodic(0.001s)\`, \`pollStorage(1000)\`, \`flush()\` per"
  echo "set, read via \`readQueue()\`, subscriber spinning rather than sleeping."
  echo
  echo "$RATE Hz is below saturation. Pushed to 2000 Hz every subject queues and repeated runs"
  echo "vary by more than 2x, which measures the queue rather than the transport."
  echo
  echo "\`tarwyn-rust\` is measured on its UDP telemetry plane, its fastest supported path."
  echo "Run \`SUBJECTS=\"tarwyn-zmq udp-floor zmq-direct java-udp ...\"\` to measure the"
  echo "ZeroMQ path or decompose the gap."
  echo
  echo "## Results"
  echo
  echo "Microseconds, lower is better, fastest first. \`Loss\` is published messages that"
  echo "never arrived, from gaps in the sequence numbers."
  for pay in $PAYLOADS; do
    echo
    echo "### $pay byte payload, $RATE Hz"
    table_for "$pay"
  done
  echo
  echo "## Environment"
  echo
  echo "\`$(date -u '+%Y-%m-%d %H:%M:%S UTC')\`  "
  echo "tarwyn-rust \`$(grep -m1 '^version' "$ROOT/core/Cargo.toml" | cut -d'"' -f2)\` · TARWYN \`v5.0.0\` · NetworkTables \`2025.3.2\`  "
  echo "rustc \`$(rustc --version | awk '{print $2}')\` · java \`$(java -version 2>&1 | head -1 | awk -F'\"' '{print $2}')\` · libzmq \`$(pkg-config --modversion libzmq 2>/dev/null || echo unknown)\`  "
  echo "\`$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)\` · $(nproc) threads · $(free -h | awk '/^Mem:/{print $2}') · kernel \`$(uname -r)\`  "
  echo
  echo "## Reproduce"
  echo
  echo "\`\`\`sh"
  echo "cargo build --release --workspace"
  echo "JARS=/path/to/jars benches/generate.sh     # see java/README.md for the jars"
  echo "\`\`\`"
} > "$OUT"

README="$ROOT/README.md"
if [ -f "$README" ] && grep -q "BENCHMARK TABLE START" "$README"; then
  MAIN_PAYLOAD="${MAIN_PAYLOAD:-96}"
  {
    echo "<!-- BENCHMARK TABLE START -->"
    echo "<!-- generated by benches/generate.sh; edits here are overwritten -->"
    echo
    echo "**${MAIN_PAYLOAD} byte payload**, publisher at ${RATE} Hz."
    echo
    table_for "$MAIN_PAYLOAD"
    echo "<!-- BENCHMARK TABLE END -->"
  } > "$ROWS/table.md"

  awk -v table="$ROWS/table.md" '
    /BENCHMARK TABLE START/ { while ((getline line < table) > 0) print line; skip = 1; next }
    /BENCHMARK TABLE END/   { skip = 0; next }
    !skip { print }
  ' "$README" > "$ROWS/README.md" && mv "$ROWS/README.md" "$README"
  echo "updated $README" >&2
fi

echo "wrote $OUT" >&2
