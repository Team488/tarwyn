#!/usr/bin/env bash
set -uo pipefail

if [ $# -lt 2 ]; then
  cat >&2 <<'USAGE'
usage: bench/compare-builds.sh <server-a> <server-b>

Measures two tarwyn_server builds against each other, alternating between them
so that drift on a noisy machine lands on both rather than on one. Prints every
measurement and the median of each build's runs.

Build the two servers however you like, then pass their paths:

    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/before
    # ... make a change ...
    cargo build --release -p tarwyn_server && cp target/release/tarwyn_server /tmp/after
    REPS=5 bench/compare-builds.sh /tmp/before /tmp/after

A single pair of runs cannot tell a real change from noise. Use at least three
reps and treat anything under a few percent as unproven.
USAGE
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PAYLOAD="${PAYLOAD:-96}"
RATE="${RATE:-500}"
SAMPLES="${SAMPLES:-3000}"
COUNT="${COUNT:-12000}"
REPS="${REPS:-3}"

BENCH="$ROOT/target/release/bench"
[ -x "$BENCH" ] || { echo "build the harness first: cargo build --release -p bench" >&2; exit 1; }

for server in "$@"; do
  [ -x "$server" ] || { echo "not an executable server: $server" >&2; exit 1; }
done

settle() {
  for pid in $(pgrep -x tarwyn_server) $(pgrep -x bench); do
    kill -9 "$pid" 2>/dev/null
  done
  sleep 1
}

measure() {
  local server=$1
  settle
  nohup "$server" >/dev/null 2>&1 &
  local pid=$!
  local tries=100
  while [ $tries -gt 0 ] && ! ss -ltn 2>/dev/null | grep -q ":5557"; do
    sleep 0.1
    tries=$((tries - 1))
  done
  local out
  out="$(mktemp)"
  timeout 90 "$BENCH" subscriber --subject tarwyn --payload "$PAYLOAD" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout 90 "$BENCH" publisher --subject tarwyn --payload "$PAYLOAD" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub
  kill -9 "$pid" 2>/dev/null
  awk -F'\t' '/^ROW/ {print $4}' "$out"
  rm -f "$out"
}

RESULTS="$(mktemp)"
for rep in $(seq 1 "$REPS"); do
  for server in "$@"; do
    median="$(measure "$server")"
    printf "rep%-3d %-40s median=%s\n" "$rep" "$(basename "$server")" "${median:-none}"
    [ -n "$median" ] && printf "%s\t%s\n" "$server" "$median" >> "$RESULTS"
  done
done

echo
for server in "$@"; do
  awk -F'\t' -v s="$server" -v name="$(basename "$server")" '
    $1 == s { values[n++] = $2 }
    END {
      if (n == 0) { printf "  %-40s no measurements\n", name; exit }
      asort(values)
      printf "  %-40s median of %d runs = %.2f us\n", name, n, values[int((n + 1) / 2)]
    }' "$RESULTS" 2>/dev/null ||
  awk -F'\t' -v s="$server" -v name="$(basename "$server")" '
    $1 == s { sum += $2; n++ }
    END {
      if (n == 0) { printf "  %-40s no measurements\n", name }
      else { printf "  %-40s mean of %d runs = %.2f us\n", name, n, sum / n }
    }' "$RESULTS"
done
rm -f "$RESULTS"
