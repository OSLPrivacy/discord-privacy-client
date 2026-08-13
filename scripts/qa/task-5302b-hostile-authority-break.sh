#!/usr/bin/env bash
set -euo pipefail

readonly TASK5302B_TARGET_DIR="/mnt/d/osl-lane-targets/i"
readonly TASK5302B_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly TASK5302B_GUARD="crates/shipping-transition-guard/src/shipping_transition_guard.rs"
readonly TASK5302B_FIXTURE="scripts/qa/fixtures/task5302b_stale_authority_resolver.rs"

readonly -a TASK5302B_CLASSES=(
  removed-friend
  old-friend-key
  expired-pro-grant
  stale-mailbox-token
  revoked-root-or-release-signer
)
readonly -a TASK5302B_CONSTANTS=(
  ENFORCE_REMOVED_FRIEND
  ENFORCE_ROTATED_FRIEND_KEY
  ENFORCE_PRO_GRANT
  ENFORCE_MAILBOX_TOKEN
  ENFORCE_REVOKED_SIGNER
)

TASK5302B_TEMP_ROOT=""
TASK5302B_NEW_COPY=""
declare -a TASK5302B_LIVE_COPIES=()

fail() {
  echo "TASK5302B_FAIL $*" >&2
  exit 1
}

cleanup() {
  local copy
  for copy in "${TASK5302B_LIVE_COPIES[@]:-}"; do
    if [[ -e "$copy/.git" ]]; then
      git -C "$TASK5302B_ROOT" worktree remove --force "$copy" >/dev/null 2>&1 || true
    fi
  done
  if [[ -n "$TASK5302B_TEMP_ROOT" && -d "$TASK5302B_TEMP_ROOT" ]]; then
    rmdir "$TASK5302B_TEMP_ROOT" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

contains_class() {
  local wanted="$1"
  local class
  for class in "${TASK5302B_CLASSES[@]}"; do
    [[ "$class" == "$wanted" ]] && return 0
  done
  return 1
}

require_complete_inventory() {
  local starved="$1"
  local observed=0
  local class
  for class in "${TASK5302B_CLASSES[@]}"; do
    [[ "$class" == "$starved" ]] || observed=$((observed + 1))
  done
  if [[ "$observed" -ne 5 ]]; then
    fail "absent hostile class=$starved observed=$observed expected=5"
  fi
  echo "TASK5302B_INVENTORY hostile_classes=$observed expected=5"
}

new_copy() {
  local name="$1"
  local copy="$TASK5302B_TEMP_ROOT/$name"
  git -C "$TASK5302B_ROOT" worktree add --quiet --detach "$copy" HEAD
  TASK5302B_LIVE_COPIES+=("$copy")
  TASK5302B_NEW_COPY="$copy"
}

discard_copy() {
  local copy="$1"
  git -C "$TASK5302B_ROOT" worktree remove --force "$copy" >/dev/null
}

run_5302() {
  local copy="$1"
  local output_file="$2"
  local status
  set +e
  (
    cd "$copy"
    CARGO_TARGET_DIR="$TASK5302B_TARGET_DIR" \
      CARGO_BUILD_JOBS=1 \
      RUSTC_WRAPPER= \
      cargo run --quiet --locked -p shipping-transition-guard --example task_5302
  ) >"$output_file" 2>&1
  status=$?
  set -e
  return "$status"
}

require_text() {
  local output_file="$1"
  local expected="$2"
  local context="$3"
  grep -Fq "$expected" "$output_file" || {
    sed -n '1,120p' "$output_file" >&2
    fail "$context missing=$expected"
  }
}

require_hostile_diagnostics() {
  local class="$1"
  local output_file="$2"
  require_text "$output_file" "TASK5302 accepted stale authority" "$class"
  require_text "$output_file" "hostile_signer=" "$class"
  require_text "$output_file" "current_controls=10" "$class current-authority-controls"
  case "$class" in
    removed-friend)
      require_text "$output_file" "stale_authority=friend/reach/removed/3558 changed_state=message:1" "$class"
      ;;
    old-friend-key)
      require_text "$output_file" "stale_authority=friend/key/3983/old changed_state=roster:1" "$class"
      require_text "$output_file" "stale_authority=friend/key/3983/old changed_state=message:1" "$class"
      ;;
    expired-pro-grant)
      require_text "$output_file" "stale_authority=pro/grant/3728/last-valid changed_state=file:1" "$class"
      require_text "$output_file" "stale_authority=pro/grant/3730/last-valid changed_state=message:1" "$class"
      require_text "$output_file" "stale_authority=pro/grant/3732/last-valid changed_state=entitlement:1" "$class"
      require_text "$output_file" "stale_authority=pro/grant/3789/last-valid changed_state=file:1" "$class"
      ;;
    stale-mailbox-token)
      require_text "$output_file" "stale_authority=mailbox/token/4354/signed-out changed_state=mailbox:1" "$class"
      ;;
    revoked-root-or-release-signer)
      require_text "$output_file" "stale_authority=account/root/5168/compromised changed_state=roster:1" "$class"
      require_text "$output_file" "stale_authority=release/signer/5170/revoked changed_state=update:1" "$class"
      require_text "$output_file" "stale_authority=release/signer/5171/revoked changed_state=carrier-table:1" "$class"
      ;;
    *) fail "unknown hostile class=$class" ;;
  esac
}

if [[ "${CARGO_TARGET_DIR:-}" != "$TASK5302B_TARGET_DIR" ]]; then
  fail "CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-unset} expected=$TASK5302B_TARGET_DIR"
fi

starved="${TASK5302B_STARVE_CLASS:-}"
if [[ -n "$starved" ]] && ! contains_class "$starved"; then
  fail "unknown starvation class=$starved"
fi
if [[ "${TASK5302B_INVENTORY_ONLY:-0}" == "1" ]]; then
  require_complete_inventory "$starved"
  exit 0
fi

TASK5302B_TEMP_ROOT="$(mktemp -d -p /tmp task5302b.XXXXXXXX)"

# Negative control 1: cooperation/no hostile action leaves the shipping build green.
new_copy cooperative-no-action
copy="$TASK5302B_NEW_COPY"
output="$TASK5302B_TEMP_ROOT/cooperative-no-action.output"
if ! run_5302 "$copy" "$output"; then
  sed -n '1,120p' "$output" >&2
  fail "cooperative no-action unexpectedly made 5302 red"
fi
require_text "$output" "hostile_state_changes=0 current_controls=10" "cooperative no-action"
echo "TASK5302B_NEGATIVE kind=cooperative-no-action task5302_exit=0 qualifies_as_attack=false"
discard_copy "$copy"

# Negative control 2: an unrelated test-only resolver cannot bypass submit.
new_copy fixture-resolver
copy="$TASK5302B_NEW_COPY"
cp "$TASK5302B_ROOT/$TASK5302B_FIXTURE" "$copy/crates/shipping-transition-guard/src/task5302b_fixture_resolver.rs"
output="$TASK5302B_TEMP_ROOT/fixture-resolver.output"
if ! run_5302 "$copy" "$output"; then
  sed -n '1,120p' "$output" >&2
  fail "fixture resolver unexpectedly affected the shipping entry point"
fi
require_text "$output" "hostile_state_changes=0 current_controls=10" "fixture resolver"
echo "TASK5302B_NEGATIVE kind=fixture-resolver task5302_exit=0 qualifies_as_attack=false shipping_entry_unchanged=true"
discard_copy "$copy"

# Negative control 3: invalid signatures can make the executable red, but do not
# qualify because no stale authority is accepted and current controls fail too.
new_copy invalid-signature
copy="$TASK5302B_NEW_COPY"
sed -i \
  's/action.signature = secret.sign(&action.signing_bytes());/action.signature = Signature::from_bytes(\&[0; 64]);/' \
  "$copy/$TASK5302B_GUARD"
output="$TASK5302B_TEMP_ROOT/invalid-signature.output"
if run_5302 "$copy" "$output"; then
  fail "invalid-signature negative control unexpectedly exited 0"
fi
require_text "$output" "InvalidSignature" "invalid-signature negative control"
if grep -Fq "TASK5302 accepted stale authority" "$output"; then
  sed -n '1,120p' "$output" >&2
  fail "invalid signature was misclassified as stale-authority acceptance"
fi
if grep -Fq "current_controls=10" "$output"; then
  sed -n '1,120p' "$output" >&2
  fail "invalid signatures unexpectedly preserved current-authority controls"
fi
echo "TASK5302B_NEGATIVE kind=invalid-signature task5302_exit=1 qualifies_as_attack=false accepted_stale_authority=0"
discard_copy "$copy"

observed=0
for index in "${!TASK5302B_CLASSES[@]}"; do
  class="${TASK5302B_CLASSES[$index]}"
  constant="${TASK5302B_CONSTANTS[$index]}"
  [[ "$class" == "$starved" ]] && continue

  new_copy "hostile-$class"
  copy="$TASK5302B_NEW_COPY"
  guard="$copy/$TASK5302B_GUARD"
  before="$(grep -Fc "const $constant: bool = true;" "$guard")"
  [[ "$before" -eq 1 ]] || fail "$class mutation seam count=$before expected=1"
  sed -i "s/const $constant: bool = true;/const $constant: bool = false;/" "$guard"

  output="$TASK5302B_TEMP_ROOT/$class.output"
  if run_5302 "$copy" "$output"; then
    sed -n '1,120p' "$output" >&2
    fail "$class accepted stale state without making 5302 exit 1"
  fi
  require_hostile_diagnostics "$class" "$output"
  echo "TASK5302B_RED class=$class task5302_exit=1 current_controls=10"
  tr ';' '\n' <"$output" | grep -F "TASK5302 accepted stale authority" || true
  observed=$((observed + 1))
  discard_copy "$copy"
done

if [[ "$observed" -ne 5 ]]; then
  fail "absent hostile class=$starved observed=$observed expected=5"
fi

echo "TASK5302B_THROWAWAY_COPIES_DISCARDED=8"
echo "TASK5302B hostile_classes=5 red_exits=5 current_controls_per_mutant=10 negative_controls=3 finish_line=CHECKED_OFF"
