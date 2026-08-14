#!/usr/bin/env bash
set -u

cd /home/liamw/osl-exec-h
export CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/h

mode="${1:-default}"
case "$mode" in
  default) force_direct=0 ;;
  --force-direct) force_direct=1 ;;
  *)
    echo "usage: $0 [--force-direct]" >&2
    exit 2
    ;;
esac

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

env_args=(-u OSL_ARTI_PROXY_PATH -u OSL_ARTI_PROXY -u OSL_ARTI_PROXY_ARGS)
extra_env=()
if [[ "$force_direct" == "1" ]]; then
  extra_env=(TOR_4900_FORCE_DIRECT=1)
fi

set +e
env "${env_args[@]}" "${extra_env[@]}" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
  cargo test -p ipc --locked \
    --test task_4900_default_tor_answer_send \
    -- --nocapture --test-threads=1 | tee "$log"
cargo_status=${PIPESTATUS[0]}
set -e

if [[ "$force_direct" == "1" ]]; then
  if [[ "$cargo_status" -ne 0 ]]; then
    exit "$cargo_status"
  fi
  if ! grep -q 'TOR-4900-INSTRUMENT .*messages_arrived=1' "$log"; then
    echo "TOR-4900-HARNESS-ERROR missing forced-Direct instrumentation" >&2
    exit 1
  fi
  exit 0
fi

if [[ "$cargo_status" -ne 0 ]]; then
  if grep -q 'TOR-4900-RED .*messages_arrived=0' "$log"; then
    exit 1
  fi
  echo "TOR-4900-HARNESS-ERROR cargo failed before producing the red proof" >&2
  exit 1
fi

exit 0
