#!/usr/bin/env bash
# Deliberately hostile, throwaway relay/cache copies for TASK 4882.
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_dir=/mnt/d/osl-lane-targets/l
scratch_dir=$(mktemp -d /tmp/task4882b.XXXXXX)
trap 'rm -rf "$scratch_dir"' EXIT

run_one() {
  local size=$1
  local mode=$2
  local log_file="$scratch_dir/${size}-${mode}.log"
  set +e
  TASK4882_MUTATION="$mode" TASK4882_MUTATION_SIZE="$size" CARGO_TARGET_DIR="$target_dir" \
    cargo test --manifest-path "$repo_dir/apps/osl-hub/Cargo.toml" --no-default-features --features core \
      --test task_4882_enclave_backward_secrecy -- --test-threads=1 --nocapture >"$log_file" 2>&1
  local status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    echo "TASK4882B FAIL size=$size mutation=$mode unexpectedly green" >&2
    exit 1
  fi
  rg -q "MUTATION size=$size" "$log_file" || { cat "$log_file" >&2; exit 1; }
  if [[ "$mode" == "a2-cache" || "$mode" == "l-cache" ]]; then
    rg -q "principal=.*epoch=$((size + 3)).*path=key-cache" "$log_file" || { cat "$log_file" >&2; exit 1; }
  else
    rg -q "sink=.*recovered_content_ids=postjoin-text-00,postjoin-attachment-00" "$log_file" || { cat "$log_file" >&2; exit 1; }
  fi
  echo "TASK4882B RED size=$size mutation=$mode exit=$status"
}

for size in 500 553; do
  run_one "$size" neutral-cache
  run_one "$size" operator-backup
  run_one "$size" derived-material
  run_one "$size" a2-cache
  run_one "$size" l-cache
done
echo "TASK4882B PASS throwaway_copies=10 discarded=10 sizes=500,553 raw_sinks=2 derived_sinks=1 removed_caches=2"
