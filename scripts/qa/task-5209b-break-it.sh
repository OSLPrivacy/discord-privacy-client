#!/usr/bin/env bash
set -euo pipefail

mutants=(unregistered-surface hard-coded-item swap-confirm-cancel generic-collision self-derived-oracle)
ran=0
for mutant in "${mutants[@]}"; do
  if [[ "${OSL_5209B_SKIP_MUTANT:-}" == "$mutant" ]]; then
    continue
  fi
  set +e
  output="$(node scripts/qa/task-5209-screen-catalogue.mjs --mutant "$mutant" 2>&1)"
  status=$?
  set -e
  if [[ $status -ne 1 ]]; then
    echo "5209b mutant=$mutant expected exit 1, got $status" >&2
    exit 1
  fi
  for field in file= runtime_path= control= expected= actual= meaning= action=; do
    if [[ "$output" != *"$field"* ]]; then
      echo "5209b mutant=$mutant omitted $field" >&2
      exit 1
    fi
  done
  echo "TASK5209B mutant=$mutant exit=1 ${output%%$'\n'*}"
  ran=$((ran + 1))
done
if [[ $ran -ne ${#mutants[@]} ]]; then
  echo "5209b absent mutant=${OSL_5209B_SKIP_MUTANT:-unknown}: expected ${#mutants[@]} attacks, ran $ran" >&2
  exit 1
fi
node scripts/qa/task-5209-screen-catalogue.mjs
echo "TASK5209B control=green mutants=$ran red_exit=1 restored=green"
