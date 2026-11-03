#!/bin/sh
# Measure total RSS and CPU of a browser's processes on macOS.
# Usage: ./measure_macos.sh NIGHTFALL 300
NAME="$1"; SECONDS_TOTAL="${2:-300}"; INTERVAL=5
OUT="ram_cpu_${NAME}_$(date +%Y%m%d_%H%M%S).csv"
echo "time,processes,rss_mb,cpu_percent" > "$OUT"
t=0
while [ "$t" -le "$SECONDS_TOTAL" ]; do
  ps -axo rss=,%cpu=,command= | grep -i "$NAME" | grep -v grep | grep -v measure_macos \
    | awk -v t="$t" '{rss+=$1; cpu+=$2; n++} END {printf "%d,%d,%.1f,%.2f\n", t, n, rss/1024, cpu}' >> "$OUT"
  sleep "$INTERVAL"; t=$((t + INTERVAL))
done
echo "Résultats : $OUT"
