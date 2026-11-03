#!/usr/bin/env bash
# CI diagnosis: start the selftest build with several display layouts and
# print a screenshot of each (base64) plus the share of magenta pixels.
set -u
BIN="$1"
for exp in soft-direct soft-shell direct full chromewin; do
  export NIGHTFALL_PROFILE_DIR="$RUNNER_TEMP/nf-exp-$exp"
  export NIGHTFALL_UI_EXPERIMENT="$exp"
  "$BIN" > "$RUNNER_TEMP/exp-$exp.out" 2>&1 &
  PID=$!
  sleep 15
  screencapture -x "$RUNNER_TEMP/exp-$exp.png"
  sips -Z 640 "$RUNNER_TEMP/exp-$exp.png" > /dev/null
  python3 ci/probe_color.py "$exp" "$RUNNER_TEMP/exp-$exp.png"
  echo "NFSHOT-BEGIN-$exp"; base64 -i "$RUNNER_TEMP/exp-$exp.png" | tr -d '\n'; echo; echo "NFSHOT-END-$exp"
  kill $PID 2>/dev/null; sleep 2; pkill -f selftest-dist || true; pkill -f "nightfall Helper" || true; sleep 2
done
exit 0
