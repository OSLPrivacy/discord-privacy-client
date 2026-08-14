#!/usr/bin/env bash
# TASK 6120 — starvation ladder.
#
# A check that would still pass with a role, an endpoint, a frozen identifier,
# the authorized controls, the hostile requests, the observed bytes or the
# independent service-side read missing is not a check. Each rung below removes
# exactly one of those from the run and requires the check to exit 1 naming what
# went missing. Nothing here relaxes the deployed service.
#
#   scripts/task-6120-starvation.sh
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"

# label | env assignment | phrase the failure must contain
RUNGS=(
"role|TASK6120_STARVE_ROLE=limited|no request was sent from the limited role"
"endpoint|TASK6120_STARVE_OP=blob.fetch|no authorized control exercised the blob.fetch endpoint"
"frozen-identifier|TASK6120_STARVE_ID=thread-6120-child-30c0|no authorized control returned the frozen identifier thread-6120-child-30c0"
"authorized-control|TASK6120_STARVE_AUTHORIZED=1|no authorized control ran"
"hostile-request|TASK6120_STARVE_HOSTILE=1|no hostile request ran"
"observed-bytes|TASK6120_STARVE_BYTES=1|captured no response bytes from the deployed service"
"service-state|TASK6120_STARVE_OBSERVE=1|independent service-side observation is missing"
)

failed=0
for rung in "${RUNGS[@]}"; do
  IFS='|' read -r name assignment phrase <<<"$rung"
  echo "=== TASK6120STARVE $name ($assignment) ==="
  out="$(env "$assignment" bash scripts/task-6120-check.sh "starve-$name" 2>&1)"
  code=$?
  echo "$out" | grep -E '^TASK6120 (SUMMARY|RESULT|CHECK)' | head -4
  echo "$out" | grep "^TASK6120 FAIL" | grep -F "$phrase" | head -2
  if [ "$code" -ne 1 ]; then
    echo "TASK6120STARVE FAIL $name: the check exited $code, expected 1"
    failed=1
  elif ! echo "$out" | grep "^TASK6120 FAIL" | grep -qF "$phrase"; then
    echo "TASK6120STARVE FAIL $name: the check exited 1 without naming it"
    failed=1
  else
    echo "TASK6120STARVE $name exit=1 named=\"$phrase\""
  fi
done

echo "TASK6120STARVE LADDER rungs=${#RUNGS[@]}"
if [ "$failed" -ne 0 ]; then
  echo "TASK6120STARVE RESULT fail"
  exit 1
fi
echo "TASK6120STARVE RESULT pass"
exit 0
