#!/usr/bin/env bash
# TASK 6844 — prove the check can fail.
#
# Runs scripts/task-6844-check.sh once per starved ingredient and requires each
# run to exit 1. A check that cannot go red is decoration, and this is the file
# that says whether it can.
#
# Exit 0 when every starvation went red, 1 otherwise.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK="$REPO_ROOT/scripts/task-6844-check.sh"

ITEMS=(
  real-capture
  binding
  signature
  dedupe
  offline-delivery
  restart
  unsupported-path-disclosure
  adversarial-event
  simulated-event
  absolute-claim
)

FAILURES=0
for item in "${ITEMS[@]}"; do
  OUTPUT="$(bash "$CHECK" --starve "$item" 2>&1)"
  CODE=$?
  REASON="$(printf '%s' "$OUTPUT" | grep -m1 '^6844 FAIL' | sed 's/^6844 FAIL //')"
  if [ "$CODE" -eq 1 ]; then
    echo "6844 starve=$item exit=1 RED reason=\"${REASON:-phase exited non-zero}\""
  else
    echo "6844 starve=$item exit=$CODE NOT-RED  <-- the check survived without this"
    FAILURES=$((FAILURES + 1))
  fi
done

if [ "$FAILURES" -ne 0 ]; then
  echo "6844 starvation sweep: $FAILURES item(s) did not turn the check red"
  exit 1
fi
echo "6844 starvation sweep: ${#ITEMS[@]}/${#ITEMS[@]} items turn the check red"
exit 0
