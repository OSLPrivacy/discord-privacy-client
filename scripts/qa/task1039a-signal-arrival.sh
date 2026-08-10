#!/usr/bin/env bash
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
export CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/b

cargo test \
  --manifest-path "$repo_root/apps/osl-hub/task_1039_signal_send/Cargo.toml" \
  -p task-1039-signal-send \
  --test task_1039a_signal_arrival \
  -- --test-threads=1 --nocapture
status=$?

if [[ $status -ne 0 ]]; then
  exit 1
fi
