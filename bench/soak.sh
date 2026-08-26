#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-$ROOT/target/soak}"
mkdir -p "$OUT"

DURATION="${DURATION:-3600}"
WINDOW="${WINDOW:-60}"
RATE="${RATE:-500}"
PAYLOAD="${PAYLOAD:-96}"

B="$ROOT/target/release/bench"
SERVER="$ROOT/target/release/tarwyn_server"

for binary in "$B" "$SERVER"; do
  if [ ! -x "$binary" ]; then
    echo "missing $binary, run: cargo build --release --workspace" >&2
    exit 1
  fi
done

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

SERVER_PID=""
PUB_PID=""
SUB_PID=""
RSS_PID=""
cleanup() {
  for pid in "$PUB_PID" "$SUB_PID" "$SERVER_PID"; do
    [ -n "$pid" ] && kill -9 "$pid" 2>/dev/null
  done
  wait 2>/dev/null
}
set +m
trap cleanup EXIT

WINDOWS="$OUT/windows.tsv"
RSS="$OUT/rss.tsv"
: > "$WINDOWS"
: > "$RSS"

echo "soaking the telemetry plane for ${DURATION}s at ${RATE} Hz, ${PAYLOAD} B payload" >&2
echo "reporting every ${WINDOW}s to $WINDOWS" >&2

$PIN_SERVER "$SERVER" > "$OUT/server.log" 2>&1 &
SERVER_PID=$!
sleep 2

$PIN_SUB "$B" subscriber --subject tarwyn-udp --payload "$PAYLOAD" \
  --window-secs "$WINDOW" --duration-secs "$DURATION" > "$OUT/subscriber.log" 2>&1 &
SUB_PID=$!
sleep 1

COUNT=$(( RATE * (DURATION + 30) ))
$PIN_PUB "$B" publisher --subject tarwyn-udp --payload "$PAYLOAD" \
  --rate "$RATE" --count "$COUNT" > "$OUT/publisher.log" 2>&1 &
PUB_PID=$!

(
  while kill -0 "$SUB_PID" 2>/dev/null; do
    printf '%s\t%s\n' "$(date +%s)" "$(awk '/VmRSS/{print $2}' "/proc/$SERVER_PID/status" 2>/dev/null)" >> "$RSS"
    sleep 5
  done
) &
RSS_PID=$!

wait "$SUB_PID"
grep '^WINDOW' "$OUT/subscriber.log" > "$WINDOWS"
cleanup

if [ ! -s "$WINDOWS" ]; then
  echo "no windows recorded, see $OUT/subscriber.log" >&2
  exit 1
fi

echo
echo "|Window|Received|Median (us)|P95 (us)|Max (us)|Lost|"
echo "|---|---|---|---|---|---|"
awk -F'\t' '{ printf "|%s|%s|%s|%s|%s|%s|\n", $2, $3, $4, $5, $6, $7 }' "$WINDOWS"

echo
awk -F'\t' '
  { received[NR] = $3; median[NR] = $4; p95[NR] = $5; lost += $7 }
  END {
    for (i = 1; i <= NR; i++) {
      if (received[i] == 0) silent++
    }
    if (silent > 0) {
      printf "FAIL: %d of %d windows received nothing, so the stream stopped\n", silent, NR
      exit
    }
    if (NR < 4) { print "too few windows to judge drift"; exit }
    quarter = int(NR / 4); if (quarter < 1) quarter = 1
    for (i = 1; i <= quarter; i++) { first_median += median[i]; first_p95 += p95[i] }
    for (i = NR - quarter + 1; i <= NR; i++) { last_median += median[i]; last_p95 += p95[i] }
    first_median /= quarter; last_median /= quarter
    first_p95 /= quarter; last_p95 /= quarter

    printf "first %d windows: median %.2f us, p95 %.2f us\n", quarter, first_median, first_p95
    printf "last  %d windows: median %.2f us, p95 %.2f us\n", quarter, last_median, last_p95
    printf "drift: median %+.1f%%, p95 %+.1f%%, %d lost overall\n",
      100 * (last_median - first_median) / first_median,
      100 * (last_p95 - first_p95) / first_p95, lost

    if (last_median > first_median * 1.25 || last_p95 > first_p95 * 1.25)
      print "FAIL: latency grew with time, which is what a queue looks like"
    else
      print "PASS: latency did not grow with time"
  }' "$WINDOWS"

if [ -s "$RSS" ]; then
  echo
  awk -F'\t' 'NR == 1 { first = $2 } { last = $2 } END {
    printf "server RSS: %d kB to %d kB (%+.1f%%)\n", first, last, 100 * (last - first) / first
  }' "$RSS"
fi
