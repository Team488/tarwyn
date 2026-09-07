#!/usr/bin/env bash
set -uo pipefail
set +m

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "$ROOT/bench/common.sh"

ROWS="${ROWS:-$ROOT/target/bench-rows}"
mkdir -p "$ROWS"

RATE="${RATE:-500}"
SAMPLES="${SAMPLES:-3000}"
WARMUP="${WARMUP:-500}"
export BENCH_WARMUP="$WARMUP"
COUNT="${COUNT:-12000}"
PAYLOADS="${PAYLOADS:-16 96}"
SUBJECTS="${SUBJECTS:-tarwyn-rust tarwyn ntcore}"
REPS="${REPS:-3}"
export BENCH_RATE_HZ="$RATE"

PYNTCORE="${PYNTCORE:-$(awk -F'"' '/pyntcore==/ { print $2 }' "$ROOT/bindings/pyproject.toml" 2>/dev/null)}"
[ -n "$PYNTCORE" ] || PYNTCORE="pyntcore"

has() { case " $SUBJECTS " in *" $1 "*) return 0;; *) return 1;; esac; }

attempt() {
  local label=$1
  shift
  local before after try
  for try in 1 2; do
    before="$(wc -l < "$CAPTURE_TO")"
    bench_settle
    "$@"
    after="$(wc -l < "$CAPTURE_TO")"
    [ "$after" -gt "$before" ] && return 0
    echo "  $label reported nothing, retrying" >&2
  done
  return 1
}

bench_pin_cpus

B="$ROOT/target/release/bench"
SERVER="$ROOT/target/release/tarwyn_server"

LIMIT="${LIMIT:-90}"

SERVER_PID=""
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap 'stop_server' EXIT

CAPTURE_TO="$ROWS/all.tsv"
capture() { grep -h '^ROW' "$1" >> "$CAPTURE_TO" 2>/dev/null; }

BENCH_ENV="$ROOT/build/bench-env.sh"
if [ ! -f "$BENCH_ENV" ]; then
  "$ROOT/gradlew" -q benchEnv >&2 || true
fi
[ -f "$BENCH_ENV" ] && . "$BENCH_ENV"
export BENCH_WPILIB_VERSION BENCH_TARWYN_VERSION
JAVA_OK=0
[ -n "${BENCH_CP:-}" ] && JAVA_OK=1

run_rust_udp() {
  local pay=$1 port=48810 out="$ROWS/udp_${pay}_r${REP:-1}.out"
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject udp --addr "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  bench_wait_port u $port || { kill -9 $sub 2>/dev/null; return 1; }
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject udp --addr "127.0.0.1:$port" --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/udp_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"
}

run_telemetry() {
  local pay=$1 out="$ROWS/telemetry_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  bench_wait_port u 5809 || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject telemetry --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  local waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 15 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject telemetry --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/telemetry_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_client() {
  local pay=$1 out="$ROWS/client_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  bench_wait_port t 5810 || { stop_server; return 1; }
  BENCH_LABEL="tarwyn-rust client v$(cargo pkgid -p tarwyn_client 2>/dev/null | sed 's/.*[#@]//' | sed 's/.*://')" \
    timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject nt4 --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject client --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/client_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_rust_nt4() {
  local pay=$1 out="$ROWS/nt4_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  bench_wait_port t 5810 || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject nt4 --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject nt4 --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/nt4_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_ntcore() {
  local tag=ntcore
  local samples="$SAMPLES"
  local warmup="$WARMUP"
  local pay=$1 port=$((48820 + pay % 100)) out="$ROWS/${tag}_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER env PYTHONPATH="$ROOT/bench/python" \
    uv run --quiet --with "$PYNTCORE" python "$ROOT/bench/python/ntcore_subject.py" \
    server --port $port > "$ROWS/${tag}_server_${pay}_r${REP:-1}.log" 2>&1 & SERVER_PID=$!
  bench_wait_port t $port || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB env PYTHONPATH="$ROOT/bench/python" \
    uv run --quiet --with "$PYNTCORE" python "$ROOT/bench/python/ntcore_subject.py" \
    subscriber --port $port --payload "$pay" --samples "$samples" --warmup "$warmup" > "$out" 2>&1 &
  local sub=$! waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  sleep 5
  timeout "$LIMIT" $PIN_PUB env PYTHONPATH="$ROOT/bench/python" \
    uv run --quiet --with "$PYNTCORE" python "$ROOT/bench/python/ntcore_subject.py" \
    publisher --port $port --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/${tag}_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_tarwyn_java() {
  local pay=$1 out="$ROWS/xtj_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER java -cp "$BENCH_TARWYN_JAR" org.team488.JServer.Main > "$ROWS/xtj_server_${pay}_r${REP:-1}.log" 2>&1 & SERVER_PID=$!
  bench_wait_port t 48800 || { stop_server; return 1; }
  sleep "${TARWYN_WARMUP:-8}"
  timeout "$LIMIT" $PIN_SUB java -cp "$BENCH_CP" tarwyn.Main subscriber --subject tarwyn-java --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  local waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  while grep -q "Connection delayed for socket: SUBSCRIBE" "$out" 2>/dev/null &&
        ! grep -q "Client socket connected: SUBSCRIBE" "$out" 2>/dev/null &&
        [ $waited -lt 45 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  sleep 5
  timeout "$LIMIT" $PIN_PUB java -cp "$BENCH_CP" tarwyn.Main publisher --subject tarwyn-java --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/xtj_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

if [ "${ONLY_REPORT:-0}" != "1" ]; then
bench_noise_check
: > "$ROWS/all.tsv"
for rep in $(seq 1 "$REPS"); do
export REP="$rep"
for pay in $PAYLOADS; do
  has udp-floor    && { echo "rep $rep payload ${pay}B: udp-floor" >&2;    attempt udp-floor    run_rust_udp "$pay"; }
  has telemetry    && { echo "rep $rep payload ${pay}B: telemetry" >&2;    attempt telemetry    run_telemetry "$pay"; }
  has tarwyn-rust && { echo "rep $rep payload ${pay}B: tarwyn-rust" >&2; attempt tarwyn-rust run_rust_nt4 "$pay"; }
  has client       && { echo "rep $rep payload ${pay}B: client" >&2;       attempt client       run_client "$pay"; }
  has ntcore       && { echo "rep $rep payload ${pay}B: ntcore" >&2;       attempt ntcore       run_ntcore "$pay"; }
  if [ "$JAVA_OK" = "1" ]; then
    has tarwyn    && { echo "rep $rep payload ${pay}B: tarwyn" >&2;      attempt tarwyn      run_tarwyn_java "$pay"; }
  fi
done
done
fi

MEDIANS="$ROWS/medians.tsv"
SPREAD="$ROWS/spread.tsv"

short_rows() {
  awk -F'\t' -v want="$SAMPLES" 'NF >= 13 && $13 + 0 < want * 0.9' "$ROWS/all.tsv" 2>/dev/null | wc -l
}

select_medians() {
  : > "$SPREAD"
  awk -F'\t' -v want="$SAMPLES" 'NF < 13 || $13 + 0 >= want * 0.9' "$ROWS/all.tsv" 2>/dev/null |
  sort -t$'\t' -k2,2 -k3,3n -k4,4g |
  awk -F'\t' -v spread="$SPREAD" '
    function flush(   lo, hi) {
      if (!n) return
      print rows[int((n + 1) / 2)]
      split(rows[1], lo, "\t")
      split(rows[n], hi, "\t")
      printf "%s\t%s\t%d\t%s\t%s\t%.1f\n", lo[2], lo[3], n, lo[4], hi[4],
        (lo[4] + 0 > 0 ? 100 * (hi[4] - lo[4]) / lo[4] : 0) >> spread
      n = 0
    }
    { key = $2 SUBSEP $3; if (key != prev) flush(); prev = key; rows[++n] = $0 }
    END { flush() }' > "$MEDIANS"
}

table_for() {
  local rows
  rows="$(awk -F'\t' -v p="$1" -v class="$2" '
    $3 == p {
      if ($2 ~ /telemetry|udp-floor/) { row = "besteffort" }
      else if ($2 ~ /client/) { row = "client" }
      else { row = "reliable" }
      if (row != class) next
      printf "%s\t|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|\n", $4, $2, $4, $5, $6, $7, $8, $9, $10, $11, $12
    }' "$MEDIANS" 2>/dev/null | sort -g -k1,1 | cut -f2-)"
  [ -n "$rows" ] || return 1
  echo "|Subject (us)|Median|P0|P80|P90|P95|P99|P99.9|P100|Loss (%)|"
  echo "|---|---|---|---|---|---|---|---|---|---|"
  printf '%s\n' "$rows"
}

co_table() {
  echo "|Subject|Payload (B)|Median|Corrected median|P99|Corrected P99|Achieved (Hz)|"
  echo "|---|---|---|---|---|---|---|"
  awk -F'\t' 'NF >= 16 {
    printf "|%s|%s|%s|%s|%s|%s|%s|\n", $2, $3, $4, $14, $9, $15, $16
  }' "$MEDIANS" 2>/dev/null | sort -t'|' -k3,3n
}

spread_table() {
  echo "|Subject|Payload (B)|Runs|Lowest median|Highest median|Spread (%)|"
  echo "|---|---|---|---|---|---|"
  sort -t$'\t' -k2,2n -k1,1 "$SPREAD" 2>/dev/null |
    awk -F'\t' '{ printf "|%s|%s|%s|%s|%s|%s|\n", $1, $2, $3, $4, $5, $6 }'
}

select_medians
DROPPED="$(short_rows)"
[ "$DROPPED" -gt 0 ] && echo "dropped $DROPPED run(s) that ended short of $SAMPLES samples" >&2
awk -F'\t' -v reps="$REPS" '$3 + 0 < reps {
  printf "%s at %s B reported %s of %s runs; the rest never got far enough to report\n", $1, $2, $3, reps
}' "$SPREAD" >&2

RESULTS="$ROOT/bench/RESULTS.md"
{
  echo "# Benchmark results"
  echo
  echo "Regenerate with \`bench/generate.sh\`; see [BENCHMARK.md](BENCHMARK.md)."
  echo "${RATE} Hz, ${SAMPLES} samples per subject with ${WARMUP} warmup discarded."
  if [ "$REPS" -gt 1 ]; then
    echo "Every subject ran ${REPS} times, subjects interleaved; each row is that"
    echo "subject's median run, picked by its median column."
  fi
  for pay in $PAYLOADS; do
    echo
    echo "## ${pay} byte payload"
    echo
    echo "### Reliable, server-relayed"
    echo
    echo "Same service contract on every row: a TCP stream through a server, values"
    echo "delivered in order, every client tuned for latency."
    echo
    table_for "$pay" reliable || echo "(none run)"
    if table_for "$pay" client > /dev/null; then
      echo
      echo "### Client libraries"
      echo
      echo "The same server and the same subscriber as the table above, published"
      echo "through a client library rather than onto a socket. The difference"
      echo "between a row here and \`tarwyn-rust\` there is what the library costs"
      echo "the code using it, which is the number a robot actually lives with."
      echo
      table_for "$pay" client
    fi
    if table_for "$pay" besteffort > /dev/null; then
      echo
      echo "### Best effort, datagram"
      echo
      echo "Not comparable with the table above: nothing here is retransmitted, ordered"
      echo "or acknowledged, so read the loss column alongside the latency."
      echo "\`udp-floor\` has no server in it at all and is the floor, not a subject."
      echo
      table_for "$pay" besteffort
    fi
  done
  if awk -F'\t' 'NF >= 16 { found = 1 } END { exit !found }' "$MEDIANS" 2>/dev/null; then
    echo
    echo "## Coordinated omission check"
    echo
    echo "Corrected columns refill the samples a stall swallowed, assuming the"
    echo "${RATE} Hz send schedule. A corrected figure far above the raw one means the"
    echo "run hit stalls the raw percentiles cannot show. Subjects whose harness does"
    echo "not report this are left out."
    echo
    co_table
  fi
  if [ "$REPS" -gt 1 ] && [ -s "$SPREAD" ]; then
    echo
    echo "## Run-to-run spread"
    echo
    echo "How far the median moved across runs of the same subject. A change smaller"
    echo "than the spread here is noise, not a result."
    echo
    spread_table
  fi
} > "$RESULTS"
echo "updated $RESULTS" >&2
