#!/usr/bin/env bash
# TASK 3338b - prove the shared delete approval (TASK 3338) can fail.
#
#   do:        In a throwaway copy, make timed delete require a Scrub mark and
#              add a timer-only Discord delete action, then run 3338.
#   done when: 3338 exits 1 naming both the refused valid timer and the second
#              Discord delete action, and the working copy is unchanged
#              afterwards.
#
# TASK 3338 was built on branch `lane/f` (commit 159c09ece) and is not on this
# lane's branch. This script therefore materialises 3338's *own* files straight
# out of that commit's git blobs into a working copy, runs 3338 against it,
# copies that working copy, applies `breaks.patch` to the COPY ONLY, runs 3338
# again there, and then proves the working copy is byte-for-byte what it was.
#
# Nothing outside $WORK and $TARGET_ROOT is written, and no sibling lane's
# directory or target directory is read or written.
#
# Usage: apps/osl-hub/task_3338b_shared_delete_approval_can_fail/run-3338b.sh
# Exit 0 when 3338b holds: 3338 green in the working copy, exit 1 in the broken
# copy naming both breaks, and the working copy unchanged afterwards.

set -uo pipefail

[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
export PATH

# sccache is unreliable against this host's /mnt/d target directory ("failed to
# set permissions for file ...: No such file or directory"), and a build that
# dies in the compiler cache would be indistinguishable from a red 3338. Both
# trees are built with the compiler directly.
export RUSTC_WRAPPER=""

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git -C "$HERE" rev-parse --show-toplevel)"

# The commit TASK 3338 was proven on. Its blobs are the working copy.
TASK_3338_COMMIT="${TASK_3338_COMMIT:-159c09ece}"
MANIFEST="apps/osl-hub/task_3338_shared_delete_action/Cargo.toml"
TEST_TARGET="task_3338_shared_delete_action"
BIN="task-3338-shared-delete-action"

WORK="${TASK_3338B_WORK:-/tmp/task-3338b}"
# Lane k's own target directory. The working copy and the broken copy build the
# same package name from different source paths, so they get one subdirectory
# each rather than overwriting each other's artefacts.
TARGET_ROOT="${CARGO_TARGET_DIR:?set it to this lane target directory}/task-3338b"

# The eight files TASK 3338 is: the three hub sources its check crate compiles
# through #[path], and the check crate itself.
HUB_SOURCES=(attachment_scan privacy_scan shared_delete_action)
CRATE_FILES=(Cargo.toml Cargo.lock src/lib.rs src/main.rs "tests/${TEST_TARGET}.rs")

say() { printf 'TASK3338B %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Put TASK 3338's own files, unmodified, into $1.
# ---------------------------------------------------------------------------
materialise() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p "$dest/apps/osl-hub/src" \
           "$dest/apps/osl-hub/task_3338_shared_delete_action/src" \
           "$dest/apps/osl-hub/task_3338_shared_delete_action/tests"
  local f
  for f in "${HUB_SOURCES[@]}"; do
    git -C "$REPO" show "$TASK_3338_COMMIT:apps/osl-hub/src/$f.rs" \
      > "$dest/apps/osl-hub/src/$f.rs" || return 1
  done
  for f in "${CRATE_FILES[@]}"; do
    git -C "$REPO" show "$TASK_3338_COMMIT:apps/osl-hub/task_3338_shared_delete_action/$f" \
      > "$dest/apps/osl-hub/task_3338_shared_delete_action/$f" || return 1
  done
}

fingerprint() {
  (cd "$1" && find . -type f | sort | xargs sha256sum)
}

# ---------------------------------------------------------------------------
# Run TASK 3338 against the tree in $1, building into $2, labelled $3.
#
# This is 3338's whole finish line, item by item. It returns 0 when 3338 holds
# and 1 when it does not, and prints one TASK3338B_3338_FAIL line naming each
# thing that did not hold, so a failure can be read without the build log.
# ---------------------------------------------------------------------------
run_3338() {
  local tree="$1" target="$2" label="$3"
  local failures=0
  local out status

  out="$(cd "$tree" && CARGO_TARGET_DIR="$target" cargo test \
          --manifest-path "$MANIFEST" --locked --offline -j3 \
          --test "$TEST_TARGET" -- --test-threads=1 2>&1)"
  status=$?
  if [ "$status" -eq 0 ]; then
    printf '%s\n' "$out" | sed -n '/^running /,$p'
  else
    # Print everything, so a failure that never reached the tests is visible.
    printf '%s\n' "$out"
  fi
  say "3338_CARGO_TEST tree=$label exit=$status"
  if [ "$status" -ne 0 ]; then
    printf '%s\n' "$out" | sed -n 's/^    \(task_3338_[a-z_]*\)$/TASK3338B_3338_FAIL item=test name=\1/p'
    failures=$((failures + 1))
  fi

  (cd "$tree" && CARGO_TARGET_DIR="$target" cargo build \
     --manifest-path "$MANIFEST" --locked --offline -j3 >/dev/null 2>&1) || {
       say "3338_BUILD tree=$label exit=1 (command not built)"
       return 1
     }
  local cmd="$target/debug/$BIN"

  # A Scrub-marked owned message deletes through the app's named action.
  out="$("$cmd" --case scrub-marked-mine)"; status=$?
  printf '%s\n' "$out"
  if [ "$status" -ne 0 ] || ! printf '%s' "$out" | grep -q \
      'result=DELETED approved_by=scrub_mark action=discord.delete-own-message'; then
    echo "TASK3338B_3338_FAIL item=scrub-marked-mine detail=the confirmed Scrub mark did not delete through discord.delete-own-message"
    failures=$((failures + 1))
  fi

  # An unmarked, valid, due timer deletes through THE SAME named action.
  # The two ways this can go wrong are reported apart, because they are the two
  # different breaks: refused outright, or delivered to some other action.
  out="$("$cmd" --case timer-due-mine)"; status=$?
  printf '%s\n' "$out"
  if [ "$status" -ne 0 ] || printf '%s' "$out" | grep -q 'result=ERR'; then
    echo "TASK3338B_3338_FAIL item=timer-due-mine-refused detail=a valid due timer on an owned message was REFUSED: $(printf '%s' "$out" | grep -o 'code=[a-z_]*' | head -1)"
    failures=$((failures + 1))
  elif ! printf '%s' "$out" | grep -q \
      'result=DELETED approved_by=timed_delete_due action=discord.delete-own-message'; then
    echo "TASK3338B_3338_FAIL item=timer-due-mine-action detail=the due timer did not go through discord.delete-own-message: $(printf '%s' "$out" | grep -o 'approved_by=[a-z_]* action=[a-z.-]*' | head -1)"
    failures=$((failures + 1))
  fi

  # Nothing approves it: refused, and the app is never asked.
  out="$("$cmd" --case unmarked-no-record-mine)"; status=$?
  printf '%s\n' "$out"
  if [ "$status" -ne 1 ] || ! printf '%s' "$out" | grep -q 'result=ERR code=not_approved'; then
    echo "TASK3338B_3338_FAIL item=unmarked-no-record-mine detail=an unapproved message was not refused with not_approved"
    failures=$((failures + 1))
  fi

  # Someone else's message: refused, and the app is never asked.
  out="$("$cmd" --case not-mine)"; status=$?
  printf '%s\n' "$out"
  if [ "$status" -ne 1 ] || ! printf '%s' "$out" | grep -q 'result=ERR code=not_yours'; then
    echo "TASK3338B_3338_FAIL item=not-mine detail=another person's message was not refused with not_yours"
    failures=$((failures + 1))
  fi

  # Each app has exactly 1 delete action.
  out="$("$cmd" --action-count)"; status=$?
  printf '%s\n' "$out"
  local app line
  for app in discord whatsapp; do
    line="$(printf '%s\n' "$out" | grep -o "app=$app delete_action_count=[0-9]* actions=\[[^]]*\]" | head -1)"
    if ! printf '%s' "$line" | grep -q "app=$app delete_action_count=1 "; then
      echo "TASK3338B_3338_FAIL item=$app-delete-action-count detail=$app does not have exactly 1 delete action: $line"
      failures=$((failures + 1))
    fi
  done

  say "3338_RESULT tree=$label failed_items=$failures"
  [ "$failures" -eq 0 ] && return 0 || return 1
}

# ---------------------------------------------------------------------------

mkdir -p "$WORK"
WORKING="$WORK/working-copy"
THROWAWAY="$WORK/throwaway"

say "COMMIT $TASK_3338_COMMIT ($(git -C "$REPO" log -1 --format=%s "$TASK_3338_COMMIT"))"
say "WORK $WORK"
say "TARGET_ROOT $TARGET_ROOT"

materialise "$WORKING" || { say "RESULT=ERROR could not materialise TASK 3338"; exit 2; }
fingerprint "$WORKING" > "$WORK/working-copy.before.sha256"
say "WORKING_COPY_BEFORE"
cat "$WORK/working-copy.before.sha256"

REPO_STATUS_BEFORE="$(git -C "$REPO" status --porcelain)"

echo
say "=== 1. TASK 3338 in the working copy, before anything is broken ==="
run_3338 "$WORKING" "$TARGET_ROOT/working-copy" working-copy
GREEN_BEFORE=$?
say "GREEN_BEFORE exit=$GREEN_BEFORE"

echo
say "=== 2. the throwaway copy, with both breaks applied ==="
rm -rf "$THROWAWAY"
cp -a "$WORKING" "$THROWAWAY"
BREAKS_PATCH="${BREAKS_PATCH:-$HERE/breaks.patch}"
say "BREAKS_PATCH $BREAKS_PATCH"
(cd "$THROWAWAY" && patch -p1 --silent < "$BREAKS_PATCH") || {
  say "RESULT=ERROR breaks.patch did not apply"; exit 2; }
say "BREAK 1 timed delete now requires a confirmed Scrub mark"
say "BREAK 2 discord takes a second, timer-only delete action (discord.timer-only-delete)"

BROKEN_OUT="$WORK/broken-3338.out"
run_3338 "$THROWAWAY" "$TARGET_ROOT/throwaway" throwaway > "$BROKEN_OUT" 2>&1
RED=$?
cat "$BROKEN_OUT"
say "RED exit=$RED"

# The second delete action is a live delete path, not just a name.
"$TARGET_ROOT/throwaway/debug/$BIN" --timer-only-route

# Does the red run name BOTH breaks?
NAMES_TIMER=0; NAMES_SECOND_ACTION=0
grep -q 'TASK3338B_3338_FAIL item=timer-due-mine-refused' "$BROKEN_OUT" && NAMES_TIMER=1
grep -q 'TASK3338B_3338_FAIL item=discord-delete-action-count' "$BROKEN_OUT" && NAMES_SECOND_ACTION=1
say "RED_NAMES refused_valid_timer=$NAMES_TIMER second_discord_delete_action=$NAMES_SECOND_ACTION"

echo
say "=== 3. the working copy is unchanged ==="
UNCHANGED=0
(cd "$WORKING" && sha256sum -c "$WORK/working-copy.before.sha256") && UNCHANGED=1
REPO_STATUS_AFTER="$(git -C "$REPO" status --porcelain)"
REPO_UNCHANGED=0
[ "$REPO_STATUS_BEFORE" = "$REPO_STATUS_AFTER" ] && REPO_UNCHANGED=1
say "WORKING_COPY_UNCHANGED=$UNCHANGED LANE_GIT_STATUS_UNCHANGED=$REPO_UNCHANGED"

echo
say "=== 4. TASK 3338 in the working copy again ==="
run_3338 "$WORKING" "$TARGET_ROOT/working-copy" working-copy
GREEN_AFTER=$?
say "GREEN_AFTER exit=$GREEN_AFTER"

rm -rf "$THROWAWAY" "$TARGET_ROOT/throwaway"
say "THROWAWAY_REMOVED $THROWAWAY"

echo
if [ "$GREEN_BEFORE" -eq 0 ] && [ "$RED" -eq 1 ] && [ "$NAMES_TIMER" -eq 1 ] \
   && [ "$NAMES_SECOND_ACTION" -eq 1 ] && [ "$UNCHANGED" -eq 1 ] \
   && [ "$REPO_UNCHANGED" -eq 1 ] && [ "$GREEN_AFTER" -eq 0 ]; then
  say "RESULT=PASS 3338 green before ($GREEN_BEFORE), exit $RED in the broken copy naming both breaks, working copy unchanged, green again ($GREEN_AFTER)"
  exit 0
fi
say "RESULT=FAIL green_before=$GREEN_BEFORE red=$RED names_timer=$NAMES_TIMER names_second_action=$NAMES_SECOND_ACTION unchanged=$UNCHANGED repo_unchanged=$REPO_UNCHANGED green_after=$GREEN_AFTER"
exit 1
