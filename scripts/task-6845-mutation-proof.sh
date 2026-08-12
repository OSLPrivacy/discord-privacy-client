#!/usr/bin/env bash
# TASK 6845 — prove screenshot notices cannot be forged or overstated.
#
# TASK 6844 built the honest notice and a check that says it works. This says
# the check can tell. For each mutation:
#
#   1. copy the shipping crate somewhere disposable,
#   2. break one property in the copy — authenticity, binding, delivery or
#      honesty,
#   3. run the 6844 check against that copy and require exit 1 with a reason
#      that names the event or the message it is about,
#   4. delete the copy.
#
# and then run the 6844 check against the untouched worktree and require a real
# supported capture, green.
#
#   scripts/task-6845-mutation-proof.sh
#   scripts/task-6845-mutation-proof.sh --starve <item> --only <scope>
#
# Starvation items: `mutant:<name>` (the copy is left unmutated),
# `supported-capture` (the restored run is denied its real capture),
# `restoration` (the restored run is made against a copy that was never put
# back). Each must make this exit 1; scripts/task-6845-starvation.sh sweeps
# them. `--only` narrows the run and is allowed only together with `--starve`,
# so a narrowed run can never be mistaken for the proof passing.
#
# Exit 0 when every mutation went red and the restored flow went green, 1
# otherwise.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK="$REPO_ROOT/scripts/task-6844-check.sh"
MUTANTS="$REPO_ROOT/scripts/task-6845-mutants.py"
COPY_ROOT="${TASK_6845_COPY_ROOT:-/tmp/osl6845-copies}"

STARVE=""
ONLY="all"

while [ $# -gt 0 ]; do
  case "$1" in
    --starve) STARVE="${2:-}"; shift 2 ;;
    --only) ONLY="${2:-}"; shift 2 ;;
    *) echo "6845 unknown argument $1" >&2; exit 2 ;;
  esac
done

if [ "$ONLY" != "all" ] && [ -z "$STARVE" ]; then
  echo "6845 --only is for showing a starvation fail; a passing run must be the whole proof" >&2
  exit 2
fi

FAILURES=0
COPIES_MADE=0
NAMED_EVENT=0
NAMED_MESSAGE=0
RED_COUNT=0
# The check's run directories live on the same Windows drive as the lane target
# directory, because that is the only place the Windows side can reach.
TARGET_DIR="${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/c}"
RUN_DRIVE="/mnt/$(printf '%s' "$TARGET_DIR" | cut -c6)"

# A fingerprint of the shipping source. Every mutation happens in a copy; if
# this moves, one of them leaked into the worktree and the "restored" run below
# would be measuring the wrong tree.
fingerprint() {
  find "$REPO_ROOT/crates/view-once-capture" -type f -print0 \
    | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16
}
BEFORE_FINGERPRINT="$(fingerprint)"

fail() {
  echo "6845 FAIL $*"
  FAILURES=$((FAILURES + 1))
}

# Does a check failure name the thing it is about? "something went wrong" is
# not a screenshot notice defect report; the open nonce identifies the capture
# event and `msg-...` identifies the view-once message.
names_event_or_message() {
  printf '%s' "$1" | grep -oE 'msg-6844[a-z-]*|open [0-9a-f]{32}' | sort -u | tr '\n' ' '
}

run_mutation() {
  local name="$1" category="$2"
  local copy="$COPY_ROOT/$name"
  local run_name="osl6845-$name"

  rm -rf "$copy"
  if ! python3 "$MUTANTS" copy --dest "$copy" >/dev/null; then
    fail "the throwaway copy for $name could not be made"
    return
  fi
  COPIES_MADE=$((COPIES_MADE + 1))

  if [ "$STARVE" = "mutant:$name" ]; then
    echo "6845 starved item=mutant:$name (the copy is left exactly as shipped)"
  elif ! python3 "$MUTANTS" apply --name "$name" --root "$copy"; then
    fail "mutation $name did not apply"
    rm -rf "$copy"
    return
  fi

  local output code reason named
  # Its own target subdirectory: cargo does not re-copy an artifact it thinks
  # is fresh, so two source trees sharing one output path can end up running
  # each other's binary.
  output="$(bash "$CHECK" --manifest "$copy/Cargo.toml" --run-name "$run_name" \
    --target-sub "6845/$name" 2>&1 </dev/null)"
  code=$?
  reason="$(printf '%s' "$output" | grep -m1 '^6844 FAIL' | sed 's/^6844 FAIL //')"

  # Discarded before the verdict is printed: a copy is not allowed to survive a
  # verdict about it, whichever way the verdict goes. The build products of a
  # mutated tree go with it.
  rm -rf "$copy" "$RUN_DRIVE/$run_name" "$TARGET_DIR/6845/$name"
  if [ -e "$copy" ]; then
    fail "the throwaway copy $copy could not be discarded"
  fi

  if [ "$code" -ne 1 ]; then
    fail "mutation=$name category=$category exit=$code the 6844 check did not go red"
    return
  fi
  named="$(names_event_or_message "$reason")"
  if [ -z "$named" ]; then
    fail "mutation=$name category=$category went red without naming the event or the message: \"$reason\""
    return
  fi
  case "$named" in *msg-6844*) NAMED_MESSAGE=$((NAMED_MESSAGE + 1)) ;; esac
  case "$named" in *open\ *) NAMED_EVENT=$((NAMED_EVENT + 1)) ;; esac
  RED_COUNT=$((RED_COUNT + 1))
  echo "6845 mutation=$name category=$category exit=1 RED names=\"${named% }\" reason=\"$reason\""
}

run_restored() {
  local arguments=(--run-name osl6845-restored)
  local label="the worktree, unmutated"
  local left_mutated=""

  if [ "$STARVE" = "restoration" ]; then
    # The copy is never put back: the "restored" flow is run against source
    # that still carries a mutation.
    left_mutated="$COPY_ROOT/never-restored"
    rm -rf "$left_mutated"
    python3 "$MUTANTS" copy --dest "$left_mutated" >/dev/null
    COPIES_MADE=$((COPIES_MADE + 1))
    python3 "$MUTANTS" apply --name forged-event-accepted --root "$left_mutated"
    arguments+=(--manifest "$left_mutated/Cargo.toml" --target-sub 6845/never-restored)
    label="a copy that was never restored"
    echo "6845 starved item=restoration (the real-capture flow is run against $label)"
  fi
  if [ "$STARVE" = "supported-capture" ]; then
    arguments+=(--starve real-capture)
    echo "6845 starved item=supported-capture (no PrintScreen is pressed during the restored run)"
  fi

  local output code capture summary
  output="$(bash "$CHECK" "${arguments[@]}" 2>&1 </dev/null)"
  code=$?
  [ -n "$left_mutated" ] && rm -rf "$left_mutated" "$TARGET_DIR/6845/never-restored"
  rm -rf "$RUN_DRIVE/osl6845-restored"

  capture="$(printf '%s' "$output" | grep -m1 '^6844 capture path=')"
  summary="$(printf '%s' "$output" | grep -m1 '^6844 summary ')"
  if [ "$code" -ne 0 ]; then
    fail "the restored real-capture flow ($label) did not pass: $(printf '%s' "$output" | grep -m1 '^6844 FAIL' | sed 's/^6844 FAIL //')"
    return
  fi
  if [ -z "$capture" ]; then
    fail "the restored run notified nobody about a real capture: it observed none"
    return
  fi
  case "$summary" in
    *"real_captures=1 notifications=1"*) ;;
    *) fail "the restored run did not produce exactly one capture and one notification: $summary"; return ;;
  esac
  echo "6845 restored source=$label exit=0 GREEN ${capture#6844 } | ${summary#6844 summary }"
}

echo "6845 mutation proof starting scope=$ONLY starve=${STARVE:-none} copies=$COPY_ROOT"
mkdir -p "$COPY_ROOT"

MUTATION_COUNT=0
RAN=0
# Read the whole list first. Each check spawns a Windows process, and a process
# that inherits this loop's stdin can swallow the rest of the list — which
# would end the sweep after one mutation and still exit 0.
mapfile -t MUTATION_LINES < <(python3 "$MUTANTS" list)
for line in "${MUTATION_LINES[@]}"; do
  [ -z "$line" ] && continue
  IFS='|' read -r name category file defect <<<"$line"
  MUTATION_COUNT=$((MUTATION_COUNT + 1))
  case "$ONLY" in
    all) ;;
    "$name") ;;
    *) continue ;;
  esac
  RAN=$((RAN + 1))
  echo "6845 --- $category/$name: $defect ($file)"
  run_mutation "$name" "$category"
done

if [ "$ONLY" = "all" ] || [ "$ONLY" = "restored" ]; then
  echo "6845 --- restored: the shipping source, a real capture, green"
  run_restored
  RAN=$((RAN + 1))
fi

if [ "$RAN" -eq 0 ]; then
  echo "6845 FAIL scope $ONLY selected nothing to run"
  exit 1
fi

AFTER_FINGERPRINT="$(fingerprint)"
if [ "$BEFORE_FINGERPRINT" != "$AFTER_FINGERPRINT" ]; then
  fail "a mutation leaked into the worktree: crate fingerprint $BEFORE_FINGERPRINT -> $AFTER_FINGERPRINT"
fi
REMAINING="$(find "$COPY_ROOT" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)"
if [ "$REMAINING" -ne 0 ]; then
  fail "$REMAINING throwaway copy/copies were not discarded"
fi
rmdir "$COPY_ROOT" "$TARGET_DIR/6845" 2>/dev/null

echo "6845 copies_made=$COPIES_MADE copies_remaining=$REMAINING worktree_fingerprint=$AFTER_FINGERPRINT unchanged=true"
if [ "$ONLY" = "all" ]; then
  echo "6845 mutations=$MUTATION_COUNT red=$RED_COUNT named_message=$NAMED_MESSAGE named_event=$NAMED_EVENT"
fi

if [ "$FAILURES" -ne 0 ]; then
  echo "6845 RESULT=RED failures=$FAILURES"
  exit 1
fi
# A narrowed run reaching here means the starvation it was given did not make
# anything fail. It exits 0 so the sweep can tell the two apart: the sweep
# requires exit 1, and this is the case where the starvation was survived.
if [ "$ONLY" != "all" ]; then
  echo "6845 RESULT=SURVIVED scope=$ONLY starve=${STARVE:-none} (nothing caught the starvation)"
  exit 0
fi
echo "6845 RESULT=GREEN"
exit 0
