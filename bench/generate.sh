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
SUBJECTS="${SUBJECTS:-}"
REPS="${REPS:-3}"
export BENCH_RATE_HZ="$RATE"

PYNTCORE="${PYNTCORE:-$(awk -F'"' '/pyntcore==/ { print $2 }' "$ROOT/bindings/pyproject.toml" 2>/dev/null)}"
[ -n "$PYNTCORE" ] || PYNTCORE="pyntcore"
BENCH_NTCORE_VERSION="${PYNTCORE#*==}"
[ "$BENCH_NTCORE_VERSION" = "$PYNTCORE" ] && BENCH_NTCORE_VERSION="unpinned"

has() { [ -z "$SUBJECTS" ] && return 0; case " $SUBJECTS " in *" $1 "*) return 0;; *) return 1;; esac; }

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
capture() {
  local file=$1 case_name=$2 implementation=$3
  awk -F'\t' -v OFS='\t' -v label="$case_name $implementation" \
    '/^ROW/ {
      version = ""
      if (split($2, parts, " ") > 1 && parts[2] ~ /^v/) version = substr(parts[2], 2)
      $2 = label
      if (version != "") $(NF+1) = version
      print
    }' "$file" >> "$CAPTURE_TO" 2>/dev/null
}

BENCH_ENV="$ROOT/build/bench-env.sh"
if [ ! -f "$BENCH_ENV" ]; then
  "$ROOT/gradlew" -q benchEnv >&2 || true
fi
[ -f "$BENCH_ENV" ] && . "$BENCH_ENV"
export BENCH_WPILIB_VERSION BENCH_TARWYN_VERSION
JAVA_OK=0
[ -n "${BENCH_CP:-}" ] && JAVA_OK=1

run_ntcore() {
  local tag=ntcore
  local samples="$SAMPLES"
  local warmup="$WARMUP"
  local case_name=$1 pay=$2 port=$((48820 + pay % 100)) out="$ROWS/${tag}_${pay}_r${REP:-1}.out"
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
  wait $sub; capture "$out" "$case_name" "ntcore"; stop_server
}

run_ntcore_server() {
  local tag=ntcore
  local case_name=$1 pay=$2 port=$((48820 + pay % 100)) out="$ROWS/${case_name}_${tag}_${pay}_r${REP:-1}.out"
  nohup $PIN_SERVER env PYTHONPATH="$ROOT/bench/python" \
    uv run --quiet --with "$PYNTCORE" python "$ROOT/bench/python/ntcore_subject.py" \
    server --port $port > "$ROWS/${case_name}_${tag}_server_${pay}_r${REP:-1}.log" 2>&1 & SERVER_PID=$!
  bench_wait_port t $port || { stop_server; return 1; }
  timeout "$LIMIT" $PIN_SUB "$B" run --case "$case_name" --impl "$tag" \
    --role subscriber --host "127.0.0.1:$port" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$! waited=0
  while ! grep -q "waiting for" "$out" 2>/dev/null && [ $waited -lt 30 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  sleep 5
  timeout "$LIMIT" $PIN_PUB "$B" run --case "$case_name" --impl "$tag" \
    --role publisher --host "127.0.0.1:$port" --rate "$RATE" --payload "$pay" --count "$COUNT" \
    > "$ROWS/${case_name}_${tag}_pub_${pay}_r${REP:-1}.log" 2>&1
  wait $sub; capture "$out" "$case_name" "$tag"; stop_server
}

run_tarwyn_java() {
  local case_name=$1 pay=$2 out="$ROWS/xtj_${pay}_r${REP:-1}.out"
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
  wait $sub; capture "$out" "$case_name" "tarwyn"; stop_server
}

run_case() {
  local case_name=$1 implementation=$2 pay=$3 mode=$4
  case "$implementation" in
    ntcore)
      if [ "$case_name" = "publish" ]; then
        run_ntcore_server "$case_name" "$pay"
      else
        run_ntcore "$case_name" "$pay"
      fi
      return
      ;;
    tarwyn)
      [ "$JAVA_OK" = "1" ] || return 0
      run_tarwyn_java "$case_name" "$pay"
      return
      ;;
  esac
  local out="$ROWS/${case_name}_${implementation}_${pay}_r${REP:-1}.out"
  case "$mode" in
    round-trip)
      nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
      bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
      timeout "$LIMIT" $PIN_SUB "$B" run --case "$case_name" --impl "$implementation" \
        --role caller --rate "$RATE" --payload "$pay" --samples "$SAMPLES" > "$out" 2>&1
      capture "$out" "$case_name" "$implementation"
      stop_server
      ;;
    delivery)
      if [ "$implementation" = "tarwyn-rust" ]; then
        nohup $PIN_SERVER "$SERVER" >/dev/null 2>&1 & SERVER_PID=$!
        bench_own_port "$SERVER_PID" t 5810 || { stop_server; return 1; }
      fi
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
      capture "$out" "$case_name" "$implementation"
      stop_server
      ;;
  esac
}

FAILED=0
FIRST_PAYLOAD="${PAYLOADS%% *}"

if [ "${ONLY_REPORT:-0}" != "1" ]; then
bench_noise_check
: > "$ROWS/all.tsv"
for rep in $(seq 1 "$REPS"); do
export REP="$rep"
for pay in $PAYLOADS; do
  while IFS=$'\t' read -r case_name group mode impls; do
    [ "$mode" = "round-trip" ] && [ "$pay" != "$FIRST_PAYLOAD" ] && continue
    for implementation in ${impls//,/ }; do
      has "$case_name" || continue
      case "$implementation" in
        tarwyn) [ "$JAVA_OK" = "1" ] || continue ;;
      esac
      bench_settle
      echo "rep $rep payload ${pay}B: $case_name/$implementation" >&2
      attempt "$case_name/$implementation" run_case "$case_name" "$implementation" "$pay" "$mode" ||
        FAILED=1
    done
  done < <("$B" list-cases)
done
done
fi

mkdir -p "$ROOT/target/bench"
if ! "$B" report --rows "$ROWS/all.tsv" --json "$ROOT/target/bench/results.json" \
  --markdown "$ROOT/bench/RESULTS.md" --rate "$RATE" --samples "$SAMPLES" \
  --warmup "$WARMUP" --reps "$REPS"; then
  echo "bench report failed" >&2
  exit 1
fi
[ "$FAILED" = "1" ] && { echo "one or more cases failed every retry" >&2; exit 1; }
echo "updated $ROOT/bench/RESULTS.md" >&2
