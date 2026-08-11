#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
break_it="${script_dir}/task_5205b_break_it.sh"
attacks=(literal-fallback duplicate-key malformed-interpolation permissive-bypass security-value-swap destructive-confirm-cancel-collision missing-unknown-role-swap payment-security-severity-disposition-swap self-derived-oracle)

# Build and prove the full control once.
"${break_it}"

for attack in "${attacks[@]}"; do
  set +e
  output="$(OSL_5205B_NO_BUILD=1 OSL_5205B_SKIP_MUTANT="${attack}" "${break_it}" 2>&1)"
  status=$?
  set -e
  if [[ ${status} -ne 1 || "${output}" != *"absent_attack=${attack}"* ]]; then
    echo "5205b starvation failed attack=${attack} exit=${status}: ${output}" >&2
    exit 1
  fi
  echo "TASK5205B_STARVATION absent_attack=${attack} exit=${status}"
done

OSL_5205B_NO_BUILD=1 "${break_it}"
echo "TASK5205B_STARVATION control=green absent_attacks_named=9 restored=green"
