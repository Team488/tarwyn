#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PAYLOAD="${PAYLOAD:-96}"
RATE="${RATE:-2000}"
SAMPLES="${SAMPLES:-3000}"
COUNT="${COUNT:-8000}"
REPS="${REPS:-3}"

B="$ROOT/target/release/benches"

settle() {
  for pid in $(pgrep -x tarwyn_server) $(pgrep -x benches); do kill -9 "$pid" 2>/dev/null; done
  sleep 1
}

measure() {
  settle
  nohup "$ROOT/target/release/tarwyn_server" >/dev/null 2>&1 &
  local server=$!
  local tries=100
  while [ $tries -gt 0 ] && ! ss -ltn 2>/dev/null | grep -q ":5557"; do sleep 0.1; tries=$((tries-1)); done
  local out; out="$(mktemp)"
  timeout 90 "$B" subscriber --subject tarwyn --payload "$PAYLOAD" --samples "$SAMPLES" > "$out" 2>&1 &
  local sub=$!
  timeout 90 "$B" publisher --subject tarwyn --payload "$PAYLOAD" --rate "$RATE" --count "$COUNT" >/dev/null 2>&1
  wait $sub
  kill -9 "$server" 2>/dev/null
  awk -F'\t' '/^ROW/ {print $4}' "$out"
  rm -f "$out"
}

for rep in $(seq 1 "$REPS"); do
  for variant in "$@"; do
    case "$variant" in
      A) cargo build --release -p tarwyn_server --no-default-features -q 2>/dev/null ;;
      B) cargo build --release -p tarwyn_server -q 2>/dev/null ;;
    esac
    printf "rep%d %s median=%s\n" "$rep" "$variant" "$(measure)"
  done
done
