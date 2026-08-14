#!/usr/bin/env bash
set -euo pipefail

manifest="tools/task-6214-durable-send/Cargo.toml"
test_target="task_6214"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT

mutants=(
  drop_row
  replay_after_cancel
  false_cancel
  no_revalidation
  off_by_one
  accept_before_reservation
  stale_check_write
  success_early
  leave_pending
)
tests=(
  encrypted_pending_outage_cancel_and_both_race_orders_have_durable_authority
  encrypted_pending_outage_cancel_and_both_race_orders_have_durable_authority
  encrypted_pending_outage_cancel_and_both_race_orders_have_durable_authority
  encrypted_pending_outage_cancel_and_both_race_orders_have_durable_authority
  linearizable_reservations_hold_all_exact_item_byte_and_disk_edges
  fresh_marked_sends_survive_all_fifteen_real_kill_points_exactly_once
  linearizable_reservations_hold_all_exact_item_byte_and_disk_edges
  fresh_marked_sends_survive_all_fifteen_real_kill_points_exactly_once
  fresh_marked_sends_survive_all_fifteen_real_kill_points_exactly_once
)

red=0
for index in "${!mutants[@]}"; do
  mutant="${mutants[$index]}"
  test_name="${tests[$index]}"
  log="$scratch/$mutant.log"
  set +e
  OSL_6214_MUTANT="$mutant" CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/c \
    CARGO_BUILD_JOBS=1 RUSTC_WRAPPER= cargo test -p task-6214-durable-send \
    --manifest-path "$manifest" --test "$test_target" "$test_name" -- \
    --exact --test-threads=1 --nocapture >"$log" 2>&1
  status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    echo "TASK6214_MUTANT_ERROR mutant=$mutant absent_attack test=$test_name exit=0"
    exit 1
  fi
  diagnostic="$(grep -E 'send_id=|edge=|authority_generation=|reservation=|measured peak|provider effect' "$log" | tail -n 1 | tr '\n' ' ')"
  if [[ -z "$diagnostic" ]]; then
    echo "TASK6214_MUTANT_ERROR mutant=$mutant exit=$status missing_named_diagnostic"
    exit 1
  fi
  red=$((red + 1))
  echo "TASK6214_MUTANT mutant=$mutant exit=$status test=$test_name diagnostic=$diagnostic"
done

echo "TASK6214_MUTANTS total=${#mutants[@]} red=$red every_candidate_discarded=true"
