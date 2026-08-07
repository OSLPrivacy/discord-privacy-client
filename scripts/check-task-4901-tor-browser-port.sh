#!/usr/bin/env bash
set -u

if [[ "${CARGO_TARGET_DIR:-}" != "/mnt/d/osl-lane-targets/n" ]]; then
  echo "TASK 4901 refused: CARGO_TARGET_DIR must be /mnt/d/osl-lane-targets/n" >&2
  exit 2
fi

set +e
output=$(cargo test -p transport --test task_4901_tor_browser_port --locked -- --ignored --nocapture 2>&1)
status=$?
set -e

printf '%s\n' "$output"

if grep -q 'TOR-4901-RED' <<<"$output"; then
  exit 1
fi

exit "$status"
