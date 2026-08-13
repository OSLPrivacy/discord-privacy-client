#!/usr/bin/env bash
# TASK 6206 — starvation ladder.
#
# A check that would still pass with a manifest row, a router row, a role, an
# endpoint, the authorized controls, the hostile requests, a frozen target, a
# keep record, the independent raw read of the deployed service's own state or
# the captured client bytes missing is not a check. Each rung below removes
# exactly one of those from the run and requires the check to exit 1 naming what
# went missing. Nothing here relaxes the deployed service.
#
# The remaining two dimensions — a missing production guard mutant and a missing
# restored run — are accounted for by scripts/task-6206-mutants.sh.
#
#   scripts/task-6206-starvation.sh
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"

# label | env assignment | phrase the failure must contain
RUNGS=(
"manifest-row|TASK6206_STARVE_MANIFEST_ROW=message.edit|unclassified deployed route: the deployed router exposes message.edit but the installed client's route manifest does not classify it"
"router-row|TASK6206_STARVE_ROUTER_ROW=channel.create|the installed client's route manifest classifies channel.create but the deployed router does not expose it"
"role|TASK6206_STARVE_ROLE=limited|no request was sent from the limited role"
"endpoint|TASK6206_STARVE_OP=message.delete|no authorized control exercised the message.delete endpoint"
"authorized-control|TASK6206_STARVE_AUTHORIZED=1|no authorized control ran"
"hostile-request|TASK6206_STARVE_HOSTILE=1|no hostile request ran"
"frozen-target|TASK6206_STARVE_TARGET=thread:thread-6206-new-limited-92c0|an authorized control changed thread:thread-6206-new-limited-92c0, which is not a declared frozen target"
"keep-record|TASK6206_STARVE_KEEP=message:message-6206-open-ben-keep-52b0|durable record message:message-6206-open-ben-keep-52b0 is neither a declared frozen target nor a declared keep record"
"independent-raw-read|TASK6206_STARVE_OBSERVE=1|independent raw read is missing"
"observed-bytes|TASK6206_STARVE_BYTES=1|captured no response bytes from the deployed service"
)

failed=0
for rung in "${RUNGS[@]}"; do
  IFS='|' read -r name assignment phrase <<<"$rung"
  echo "=== TASK6206STARVE $name ($assignment) ==="
  out="$(env "$assignment" bash scripts/task-6206-check.sh "starve-$name" 2>&1)"
  code=$?
  echo "$out" | grep -E '^TASK6206 (SUMMARY|RESULT|CHECK)' | head -3
  echo "$out" | grep "^TASK6206 FAIL" | grep -F "$phrase" | head -1
  if [ "$code" -ne 1 ]; then
    echo "TASK6206STARVE FAIL $name: the check exited $code, expected 1"
    failed=1
  elif ! echo "$out" | grep "^TASK6206 FAIL" | grep -qF "$phrase"; then
    echo "TASK6206STARVE FAIL $name: the check exited 1 without naming it"
    echo "$out" | grep "^TASK6206 FAIL" | head -4
    failed=1
  else
    echo "TASK6206STARVE $name exit=1 named=\"$phrase\""
  fi
done

echo "TASK6206STARVE LADDER rungs=${#RUNGS[@]}"
if [ "$failed" -ne 0 ]; then
  echo "TASK6206STARVE RESULT fail"
  exit 1
fi
echo "TASK6206STARVE RESULT pass"
exit 0
