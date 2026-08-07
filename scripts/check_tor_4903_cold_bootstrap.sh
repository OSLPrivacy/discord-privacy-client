#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

log="$(mktemp)"
set +e
CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i cargo test --locked -p transport --test task_4903_cold_bootstrap -- --nocapture 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

if [[ "$status" -eq 0 ]]; then
  rm -f "$log"
  exit 0
fi

if grep -q "TOR-4903-RED" "$log"; then
  rm -f "$log"
  exit 1
fi

rm -f "$log"
exit "$status"
