#!/usr/bin/env bash
set -euo pipefail

task_tmp="$(mktemp -d "${TMPDIR:-/tmp}/osl-5209b-run-XXXXXX")"
trap 'rm -rf -- "$task_tmp"' EXIT
export OSL_5209B_TMP_ROOT="$task_tmp"

mutants=(
  routed-hard-coded-sentence
  modal-hard-coded-button
  hard-coded-hover-title-reason
  unregistered-reachable-screen
  swap-confirm-cancel
  generic-destructive-harmless-label
  cross-wired-security-reason
)
if [[ -n "${OSL_5209B_SKIP_MUTANT:-}" ]]; then
  found=0
  for mutant in "${mutants[@]}"; do
    [[ "$mutant" == "$OSL_5209B_SKIP_MUTANT" ]] && found=1
  done
  if [[ $found -eq 1 ]]; then
    echo "5209b absent mutant=$OSL_5209B_SKIP_MUTANT: expected ${#mutants[@]} attacks, ran $((${#mutants[@]} - 1))" >&2
    exit 1
  fi
  echo "5209b unknown starvation attack=$OSL_5209B_SKIP_MUTANT" >&2
  exit 1
fi
ran=0
for mutant in "${mutants[@]}"; do
  set +e
  output="$(node scripts/qa/task-5209-screen-catalogue.mjs --mutant "$mutant" 2>&1)"
  status=$?
  set -e
  if [[ $status -ne 1 ]]; then
    echo "5209b mutant=$mutant expected exit 1, got $status" >&2
    exit 1
  fi
  for field in file= surface= runtime_path= control= expected_copy= actual_copy= meaning_class= invoked_action= expected= actual= meaning= action= poisoned_oracle=regenerated disposal=1 build_discarded=1 temp_remaining=0; do
    if [[ "$output" != *"$field"* ]]; then
      echo "5209b mutant=$mutant omitted $field" >&2
      exit 1
    fi
  done
  diagnostic="$(awk '/^5209b file=/{print; exit}' <<<"$output")"
  if [[ -z "$diagnostic" ]]; then
    echo "5209b mutant=$mutant omitted first structured diagnostic" >&2
    exit 1
  fi
  echo "TASK5209B mutant=$mutant exit=1 $diagnostic"
  ran=$((ran + 1))
done
if [[ $ran -ne ${#mutants[@]} ]]; then
  echo "5209b absent mutant=${OSL_5209B_SKIP_MUTANT:-unknown}: expected ${#mutants[@]} attacks, ran $ran" >&2
  exit 1
fi
green="$(node scripts/qa/task-5209-screen-catalogue.mjs)"
if [[ "$green" != *"items=1626 invoked_actions=1626 literals=0 missing_keys=0 generic_collisions=0 swapped_controls=0 cross_wired_mappings=0"* ]]; then
  echo "5209b green gate omitted exact zero/action counts" >&2
  exit 1
fi
if find "$task_tmp" -mindepth 1 -print -quit | grep -q .; then
  echo "5209b temporary mutant directories remain" >&2
  exit 1
fi
echo "$green"
echo "TASK5209B control=green mutants=$ran red_exit=1 restored=green disposals=$ran builds=$ran discarded_builds=$ran temp_remaining=0"
