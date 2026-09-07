#!/usr/bin/env bash

bench_pin_cpus() {
  PIN_SERVER=""
  PIN_PUB=""
  PIN_SUB=""
  [ "${PIN:-1}" = "1" ] || return 0
  command -v taskset >/dev/null 2>&1 || return 0
  local server pub sub
  read -r server pub sub <<EOF
$(lscpu -p=CPU,CORE 2>/dev/null | awk -F, '!/^#/ && $2 != 0 && !seen[$2]++ { print $1 }' | head -3 | tr '\n' ' ')
EOF
  [ -n "$sub" ] || return 0
  PIN_SERVER="taskset -c $server"
  PIN_PUB="taskset -c $pub"
  PIN_SUB="taskset -c $sub"
}

bench_settle() {
  local pid
  for pid in $(pgrep -x java) $(pgrep -x tarwyn_server) $(pgrep -x bench) $(pgrep -f ntcore_subject.py); do
    kill -9 "$pid" 2>/dev/null
  done
  sleep 1
}

bench_wait_port() {
  local proto=$1 port=$2 tries=${3:-200}
  while [ "$tries" -gt 0 ]; do
    ss -ln"$proto" 2>/dev/null | grep -q ":$port" && return 0
    sleep 0.1
    tries=$((tries - 1))
  done
  echo "  timed out waiting for port $port" >&2
  return 1
}

bench_noise_check() {
  local governors load boost driver epp
  governors="$(cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null | sort -u | tr '\n' ' ')"
  driver="$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver 2>/dev/null)"
  epp="$(cat /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference 2>/dev/null | sort -u | tr '\n' ' ')"
  case "${governors% }" in
    "" | performance) ;;
    powersave)
      case "$driver" in
        *-epp | intel_pstate)
          case "${epp% }" in
            performance) ;;
            *) echo "note: hardware picks the clock (${driver}, epp '${epp% }'); 'performance' pins it and tightens spread" >&2 ;;
          esac
          ;;
        *) echo "note: cpu governor is 'powersave' on ${driver:-unknown}, so clocks ramp; expect run-to-run spread" >&2 ;;
      esac
      ;;
    *) echo "note: cpu governor is '${governors% }', not performance; expect run-to-run spread" >&2 ;;
  esac
  boost="$(cat /sys/devices/system/cpu/cpufreq/boost 2>/dev/null)"
  [ "$boost" = "1" ] &&
    echo "note: turbo/boost is on, so clocks drift with temperature across a long run" >&2
  load="$(awk '{ print $1 }' /proc/loadavg 2>/dev/null)"
  awk -v l="${load:-0}" 'BEGIN {
    if (l > 1.0) printf "note: load average is %.2f, the machine is not quiet\n", l
  }' >&2
  return 0
}
