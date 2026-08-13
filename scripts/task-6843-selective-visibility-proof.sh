#!/usr/bin/env bash
# TASK 6843 - mutation proof for TASK 6842 selective recipient-key delivery.
#
# Each run copies the actual 6842 implementation and its exact acceptance test
# into a separately named throwaway crate. It then applies one verified defect
# to that copy and runs TASK 6842 itself. A valid proof has one unmodified
# green control and one red (exit 1) TASK 6842 run for every named mutation.
# No modified source is ever compiled from the worktree.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$REPO_ROOT/crates/ipc/src/selective_visibility.rs"
CHECK="$REPO_ROOT/crates/ipc/tests/task_6842_selective_visibility.rs"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/osl-task6843-copies-XXXXXX")"
EXPECTED_TARGET="/mnt/d/osl-lane-targets/i"

if [ "${CARGO_TARGET_DIR:-}" != "$EXPECTED_TARGET" ]; then
  echo "TASK6843_FAIL CARGO_TARGET_DIR must be $EXPECTED_TARGET"
  exit 2
fi

SOURCE_SHA_BEFORE="$(sha256sum "$SOURCE" | cut -d' ' -f1)"
CHECK_SHA_BEFORE="$(sha256sum "$CHECK" | cut -d' ' -f1)"
echo "TASK6843_SOURCE=$SOURCE"
echo "TASK6843_CHECK=$CHECK"
echo "TASK6843_SOURCE_SHA256_BEFORE=$SOURCE_SHA_BEFORE"
echo "TASK6843_CHECK_SHA256_BEFORE=$CHECK_SHA_BEFORE"
echo "TASK6843_THROWAWAY_ROOT=$WORK"

discard() {
  rm -rf "$WORK"
  if [ -e "$WORK" ]; then
    echo "TASK6843_COPIES_DISCARDED=false"
  else
    echo "TASK6843_COPIES_DISCARDED=true"
  fi
}
trap discard EXIT

STATUS=0

make_copy() {
  local name="$1"
  local dir="$WORK/$name"
  mkdir -p "$dir/tests"
  cp "$SOURCE" "$dir/selective_visibility.rs"
  printf '%s\n' 'pub mod selective_visibility;' >"$dir/lib.rs"
  cp "$CHECK" "$dir/tests/task_6842_selective_visibility.rs"
  cat >"$dir/Cargo.toml" <<EOF
[package]
name = "task-6843-copy-$name"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
name = "ipc"
path = "lib.rs"

[dependencies]
base64 = "0.22"
sha2 = "0.10"
thiserror = "2"
crypto = { path = "$REPO_ROOT/crates/crypto" }

[workspace]
EOF
  echo "$dir"
}

require_applied() {
  local mode="$1"
  local file="$2"
  local count
  count="$(grep -c 'starved by TASK 6843' "$file" || true)"
  echo "TASK6843_SABOTAGE_LINES_APPLIED[$mode]=$count"
  if [ "$count" -lt 1 ]; then
    echo "TASK6843_FAIL $mode sabotage matched nothing"
    STATUS=1
    return 1
  fi
}

starve() {
  local dir="$1"
  local mode="$2"
  local file="$dir/selective_visibility.rs"
  case "$mode" in
    control) ;;
    identity-blindness)
      sed -i 's|raw.extend_from_slice(handshake.ek_x25519_pub.as_bytes());|raw.extend_from_slice(member.x25519_pub.as_bytes()); // starved by TASK 6843|' "$file"
      ;;
    hidden-key)
      sed -i '/TASK6843-KEY-ISSUANCE/,/let selected = selected_indices(members.len(), &audience)/{s|let selected = selected_indices(members.len(), &audience)?;|let selected = vec![0usize]; // starved by TASK 6843|;}' "$file"
      ;;
    placeholder|unread|notification|reaction-target|timing-hint)
      local field
      case "$mode" in
        placeholder) field="placeholders" ;;
        unread) field="unread_increments" ;;
        notification) field="notifications" ;;
        reaction-target) field="reaction_targets" ;;
        timing-hint) field="osl_timing_metadata" ;;
      esac
      sed -i "/TASK6843-HIDDEN-SURFACES/,/^[[:space:]]*}/{s|Self::default()|Self { $field: 1, ..Self::default() } // starved by TASK 6843|}" "$file"
      ;;
    manifest-binding)
      sed -i '/TASK6843-MANIFEST-SIGNATURE/{n;s|if !valid {|if false { // starved by TASK 6843|;}' "$file"
      ;;
    membership-mutation)
      sed -i '/TASK6843-MEMBERSHIP-BINDING/,/if manifest.membership_snapshot != membership_snapshot(current_members)/{s|if manifest.membership_snapshot != membership_snapshot(current_members) {|if false { // starved by TASK 6843|;}' "$file"
      ;;
    *)
      echo "TASK6843_FAIL unknown mutation $mode"
      return 1
      ;;
  esac
  if [ "$mode" = control ]; then
    if grep -q 'starved by TASK 6843' "$file"; then
      echo "TASK6843_FAIL control copy was modified"
      STATUS=1
      return 1
    fi
  else
    require_applied "$mode" "$file"
  fi
}

run_mode() {
  local mode="$1"
  local expected_exit="$2"
  local expected_trace="$3"
  local dir output code
  dir="$(make_copy "$mode")"
  starve "$dir" "$mode" || return
  echo "--- TASK6843_MODE=$mode ---"
  output="$(cd "$dir" && cargo +1.97.1 test --test task_6842_selective_visibility -- --test-threads=1 --nocapture 2>&1)"
  local cargo_code=$?
  # Cargo uses 101 for a failing test binary.  The proof's public per-copy
  # verdict is normalized to the promised red exit 1, while retaining Cargo's
  # actual code in the transcript.
  if [ "$cargo_code" -eq 0 ]; then code=0; else code=1; fi
  echo "$output"
  echo "TASK6843_CARGO_EXIT[$mode]=$cargo_code"
  echo "TASK6843_MODE_EXIT[$mode]=$code"
  if [ "$code" -ne "$expected_exit" ]; then
    echo "TASK6843_VERDICT[$mode]=FAIL exit $code expected $expected_exit"
    STATUS=1
    return
  fi
  if [ "$expected_exit" -eq 0 ]; then
    if echo "$output" | grep -q 'TASK6842_GREEN combinations=32' &&
       echo "$output" | grep -q 'TASK6842_FAULTS membership_race=MembershipRace'; then
      echo "TASK6843_VERDICT[$mode]=PASS exit=0 TASK6842=green"
    else
      echo "TASK6843_VERDICT[$mode]=FAIL control did not run both TASK6842 checks"
      STATUS=1
    fi
  elif echo "$output" | grep -Eq 'member [0-9]+.*message' &&
       echo "$output" | grep -Fq "$expected_trace"; then
    echo "TASK6843_VERDICT[$mode]=PASS exit=1 named-member-message trace=$expected_trace"
  else
    echo "TASK6843_VERDICT[$mode]=FAIL red copy lacked member/message trace=$expected_trace"
    STATUS=1
  fi
}

run_mode control 0 ""
run_mode identity-blindness 1 "identity-blind store exposed member"
run_mode hidden-key 1 "hidden key"
run_mode placeholder 1 "leaked placeholder"
run_mode unread 1 "leaked unread"
run_mode notification 1 "leaked notification"
run_mode reaction-target 1 "leaked reaction-target"
run_mode timing-hint 1 "leaked timing-hint"
run_mode manifest-binding 1 "detached audience manifest"
run_mode membership-mutation 1 "concurrent membership mutation"

SOURCE_SHA_AFTER="$(sha256sum "$SOURCE" | cut -d' ' -f1)"
CHECK_SHA_AFTER="$(sha256sum "$CHECK" | cut -d' ' -f1)"
echo "TASK6843_SOURCE_SHA256_AFTER=$SOURCE_SHA_AFTER"
echo "TASK6843_CHECK_SHA256_AFTER=$CHECK_SHA_AFTER"
if [ "$SOURCE_SHA_BEFORE" != "$SOURCE_SHA_AFTER" ] || [ "$CHECK_SHA_BEFORE" != "$CHECK_SHA_AFTER" ]; then
  echo "TASK6843_FAIL tracked TASK6842 sources changed during proof"
  STATUS=1
fi

echo "TASK6843_OVERALL_EXIT=$STATUS"
exit "$STATUS"
