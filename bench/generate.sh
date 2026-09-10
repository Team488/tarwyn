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
SUBJECTS="${SUBJECTS:-publish telemetry_publish udp_floor get compare_and_set delete tables ping}"
REPS="${REPS:-3}"
export BENCH_RATE_HZ="$RATE"

PYNTCORE="${PYNTCORE:-$(awk -F'"' '/pyntcore==/ { print $2 }' "$ROOT/bindings/pyproject.toml" 2>/dev/null)}"
[ -n "$PYNTCORE" ] || PYNTCORE="pyntcore"
BENCH_NTCORE_VERSION="${PYNTCORE#*==}"
[ "$BENCH_NTCORE_VERSION" = "$PYNTCORE" ] && BENCH_NTCORE_VERSION="unpinned"

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
  bench_own_port "$SERVER_PID" u 5809 || { stop_server; return 1; }
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

run_ntcore_server() {
  local pay=$1 port=$((48850 + pay % 100)) out="$ROWS/ntsrv_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER env PYTHONPATH="$ROOT/bench/python" \
    uv run --quiet --with "$PYNTCORE" python "$ROOT/bench/python/ntcore_subject.py" \
    server --port $port > "$ROWS/ntsrv_server_${pay}_r${REP:-1}.log" 2>&1 & SERVER_PID=$!
  bench_wait_port t $port || { stop_server; return 1; }
  BENCH_LABEL="ntcore server v${BENCH_NTCORE_VERSION:-unknown}" \
    timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject nt4 --host "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$! waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  sleep 3
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject nt4 --host "127.0.0.1:$port" --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/ntsrv_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_client() {
  local pay=$1 out="$ROWS/client_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
  BENCH_LABEL="tarwyn-rust v$(cargo pkgid -p tarwyn_client 2>/dev/null | sed 's/.*[#@]//' | sed 's/.*://')" \
    timeout "$LIMIT" $PIN_SUB "$B" subscriber --subject nt4 --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout "$LIMIT" $PIN_PUB "$B" publisher --subject client --payload "$pay" --rate "$RATE" --count "$COUNT" > "$ROWS/client_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out"; stop_server
}

run_rust_nt4() {
  local pay=$1 out="$ROWS/nt4_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
  bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
  BENCH_LABEL="tarwyn-rust server v$(cargo pkgid -p tarwyn_server 2>/dev/null | sed 's/.*[#@]//' | sed 's/.*://')" \
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

run_case() {
  local case_name=$1 implementation=$2 pay=$3 mode=$4
  case "$implementation" in
    ntcore)
      run_ntcore "$pay"
      return
      ;;
    ntcore-server)
      run_ntcore_server "$pay"
      return
      ;;
    tarwyn)
      [ "$JAVA_OK" = "1" ] || return 0
      run_tarwyn_java "$pay"
      return
      ;;
  esac
  local out="$ROWS/${case_name}_${implementation}_${pay}_r${REP:-1}.out"
  case "$mode" in
    round-trip)
      nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
      bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
      timeout "$LIMIT" $PIN_SUB "$B" run --case "$case_name" --impl "$implementation" \
        --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1
      capture "$out"
      stop_server
      ;;
    delivery)
      case "$implementation" in
        tarwyn-rust | tarwyn-rust-client)
          nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
          bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
          ;;
      esac
      timeout "$LIMIT" $PIN_SUB "$B" run --case "$case_name" --impl "$implementation" \
        --role subscriber --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
      local sub=$! waited=0
      while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
        sleep 1
        waited=$((waited + 1))
      done
      timeout "$LIMIT" $PIN_PUB "$B" run --case "$case_name" --impl "$implementation" \
        --role publisher --payload "$pay" --rate "$RATE" --count "$COUNT" \
        > "$ROWS/${case_name}_${implementation}_pub_${pay}_r${REP:-1}.log" 2>&1
      wait $sub
      capture "$out"
      stop_server
      ;;
  esac
}

if [ "${ONLY_REPORT:-0}" != "1" ]; then
bench_noise_check
: > "$ROWS/all.tsv"
for rep in $(seq 1 "$REPS"); do
export REP="$rep"
for pay in $PAYLOADS; do
  while IFS=$'\t' read -r case_name group mode impls; do
    for implementation in ${impls//,/ }; do
      has "$case_name" || continue
      case "$implementation" in
        tarwyn) [ "$JAVA_OK" = "1" ] || continue ;;
      esac
      bench_settle
      echo "rep $rep payload ${pay}B: $case_name/$implementation" >&2
      attempt "$case_name/$implementation" run_case "$case_name" "$implementation" "$pay" "$mode"
    done
  done < <("$B" list-cases)
done
done
fi

mkdir -p "$ROOT/target/bench"
"$B" report --rows "$ROWS/all.tsv" --json "$ROOT/target/bench/results.json" \
  --markdown "$ROOT/bench/RESULTS.md" --rate "$RATE" --samples "$SAMPLES" \
  --warmup "$WARMUP" --reps "$REPS"
echo "updated $ROOT/bench/RESULTS.md" >&2
