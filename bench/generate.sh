#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROWS="${ROWS:-$ROOT/target/bench-rows}"
mkdir -p "$ROWS"

RATE="${RATE:-500}"
SAMPLES="${SAMPLES:-3000}"
WARMUP="${WARMUP:-500}"
export BENCH_WARMUP="$WARMUP"
COUNT="${COUNT:-12000}"
PAYLOADS="${PAYLOADS:-16 96}"
SUBJECTS="${SUBJECTS:-tarwyn-rust tarwyn nt4}"

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

B="$ROOT/target/release/bench"
SERVER="$ROOT/target/release/tarwyn_server"

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
  for pid in $(pgrep -x java) $(pgrep -x tarwyn_server) $(pgrep -x bench); do
    kill -9 "$pid" 2>/dev/null
  done
  sleep 1
}

BENCH_ENV="$ROOT/build/bench-env.sh"
if [ ! -f "$BENCH_ENV" ]; then
  "$ROOT/gradlew" -q benchEnv >&2 || true
fi
[ -f "$BENCH_ENV" ] && . "$BENCH_ENV"
JAVA_OK=0
[ -n "${BENCH_CP:-}" ] && JAVA_OK=1

run_rust_udp() {
  local pay=$1 port=48810 out="$ROWS/udp_$pay.out"
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject udp --addr "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject udp --addr "127.0.0.1:$port" --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
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



run_nt4() {
  local pay=$1 port=$((48820 + pay % 100)) out="$ROWS/nt4_$pay.out"
  timeout "$LIMIT" $PIN_SUB env LD_PRELOAD="$BENCH_NATIVES/libwpiutiljni.so" java --enable-native-access=ALL-UNNAMED -Djava.library.path="$BENCH_NATIVES" -cp "$BENCH_CP" \
    tarwyn.Main subscriber --subject nt4 --port $port --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  if wait_port t $port; then
    timeout "$LIMIT" $PIN_PUB env LD_PRELOAD="$BENCH_NATIVES/libwpiutiljni.so" java --enable-native-access=ALL-UNNAMED -Djava.library.path="$BENCH_NATIVES" -cp "$BENCH_CP" \
      tarwyn.Main publisher --subject nt4 --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
    wait $sub; capture "$out"
  else
    kill -9 $sub 2>/dev/null
  fi
}

run_tarwyn_java() {
  local pay=$1 out="$ROWS/xtj_$pay.out"
  nohup $PIN_SERVER java -cp "$BENCH_TARWYN_JAR" org.team488.JServer.Main >/dev/null 2>&1 & SERVER_PID=$!
  wait_port t 48800 || { stop_server; return 1; }
  sleep "${TARWYN_WARMUP:-8}"
  timeout "$LIMIT" $PIN_SUB java -cp "$BENCH_CP" tarwyn.Main subscriber --subject tarwyn-java --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  local waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  sleep 2
  timeout "$LIMIT" $PIN_PUB java -cp "$BENCH_CP" tarwyn.Main publisher --subject tarwyn-java --payload "$pay" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub; capture "$out"; stop_server
}

if [ "${ONLY_REPORT:-0}" != "1" ]; then
: > "$ROWS/all.tsv"
for pay in $PAYLOADS; do
  has udp-floor    && { settle; echo "payload ${pay}B: udp-floor" >&2;    run_rust_udp "$pay"; }
  has tarwyn-zmq  && { settle; echo "payload ${pay}B: tarwyn-zmq" >&2;  run_rust_tarwyn "$pay"; }
  has tarwyn-rust && { settle; echo "payload ${pay}B: tarwyn-rust" >&2; run_rust_tarwyn_udp "$pay"; }
  if [ "$JAVA_OK" = "1" ]; then
    has nt4     && { settle; echo "payload ${pay}B: nt4" >&2;     run_nt4 "$pay"; }
    has tarwyn && { settle; echo "payload ${pay}B: tarwyn" >&2; run_tarwyn_java "$pay"; }
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



RESULTS="$ROOT/bench/RESULTS.md"
{
  echo "# Benchmark results"
  echo
  echo "Regenerate with \`bench/generate.sh\`; see [BENCHMARK.md](BENCHMARK.md)."
  echo "${RATE} Hz, ${WARMUP} warmup samples discarded, ${SAMPLES} recorded per subject."
  for pay in $PAYLOADS; do
    echo
    echo "## ${pay} byte payload"
    echo
    table_for "$pay"
  done
} > "$RESULTS"
echo "updated $RESULTS" >&2
