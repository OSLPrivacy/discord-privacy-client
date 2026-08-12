#!/usr/bin/env bash
# TASK 6845 — prove the mutation proof can fail.
#
# The proof in scripts/task-6845-mutation-proof.sh only means something if it
# notices when one of its own ingredients is missing. Three things can go
# missing:
#
#   * a mutant — the copy is left exactly as shipped, so the 6844 check passes
#     and the mutation was never demonstrated to be caught;
#   * the supported capture — the restored run is denied its real PrintScreen,
#     so "the real flow still passes" was never shown;
#   * the restoration — the restored run is made against a copy that still
#     carries a mutation, so nothing was ever put back.
#
# Each starvation must make the proof exit 1. The starved runs are narrowed to
# the part they starve (`--only`), because the full proof re-runs a real
# desktop capture per mutation and a sweep of full runs would take longer than
# the check it is defending.
#
# Exit 0 when every starvation was caught, 1 otherwise.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROOF="$REPO_ROOT/scripts/task-6845-mutation-proof.sh"
MUTANTS="$REPO_ROOT/scripts/task-6845-mutants.py"

FAILURES=0
TOTAL=0

check_starvation() {
  local item="$1" scope="$2"
  local output code reason
  TOTAL=$((TOTAL + 1))
  output="$(bash "$PROOF" --starve "$item" --only "$scope" 2>&1 </dev/null)"
  code=$?
  reason="$(printf '%s' "$output" | grep -m1 '^6845 FAIL' | sed 's/^6845 FAIL //')"
  if [ "$code" -eq 1 ] && [ -n "$reason" ]; then
    echo "6845 starve=$item exit=1 RED reason=\"$reason\""
  else
    echo "6845 starve=$item exit=$code NOT-RED  <-- the proof survived without this"
    FAILURES=$((FAILURES + 1))
  fi
}

# Read the list before running anything: the proof spawns Windows processes
# that inherit this loop's stdin and would otherwise consume the rest of it.
mapfile -t LINES < <(python3 "$MUTANTS" list)
for line in "${LINES[@]}"; do
  [ -z "$line" ] && continue
  IFS='|' read -r name _category _file _defect <<<"$line"
  check_starvation "mutant:$name" "$name"
done

check_starvation "supported-capture" "restored"
check_starvation "restoration" "restored"

if [ "$FAILURES" -ne 0 ]; then
  echo "6845 starvation sweep: $FAILURES item(s) did not turn the proof red"
  exit 1
fi
echo "6845 starvation sweep: $TOTAL/$TOTAL items turn the proof red"
exit 0
