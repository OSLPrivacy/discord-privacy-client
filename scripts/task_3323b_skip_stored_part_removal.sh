#!/usr/bin/env bash
# TASK 3323b — prove the stored-part removal can fail.
#
# In a throwaway copy of the TASK 3323 build, skip the stored-part removal in
# the hub's timed-delete sweep job and run 3323. 3323 must exit 1 and must name
# the protected item that is still stored on the OSL service. This lane's
# working copy is never edited: the throwaway is built from git objects into a
# temporary directory outside the repository, and the working copy is
# fingerprinted before and after so "unchanged" is a measurement, not a claim.
#
# TASK 3323 itself was built in lane e; its files are read out of the committed
# tree of the branch that holds them (default lane/e, override with REF=...).
# Nothing in a sibling lane's working directory is read or written.
#
# Usage:  scripts/task_3323b_skip_stored_part_removal.sh
set -u -o pipefail

REPO="${REPO:-/home/liamw/osl-exec-h}"
REF="${REF:-lane/e}"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/h}"
export CARGO_TARGET_DIR
JOB_PATH="apps/osl-hub/src/timed_delete_sweep_job.rs"
CHECK_DIR="apps/osl-hub/task_3323_stored_protected_part"
PROTECTED_ITEM="osl-protected-part:discord/dm:task-3323/discord-message-3323-due-protected"

say() { printf '\n=== %s ===\n' "$*"; }

# A fingerprint of this lane's working copy: the commit, every tracked
# difference from it, and the content of every untracked file.
fingerprint() {
  git -C "$REPO" rev-parse HEAD
  git -C "$REPO" status --porcelain=v1 -uall
  git -C "$REPO" diff HEAD --binary | sha256sum
  git -C "$REPO" ls-files --others --exclude-standard -z \
    | xargs -0 -r -n 50 sha256sum -- 2>/dev/null | sort
}

git -C "$REPO" cat-file -e "$REF:$CHECK_DIR/src/lib.rs" 2>/dev/null || {
  echo "3323b_ERROR the TASK 3323 build is not in $REF" >&2; exit 2; }

say "working copy fingerprint before"
BEFORE="$(mktemp)"; AFTER="$(mktemp)"
fingerprint > "$BEFORE"
echo "3323b_WORKING_COPY_BEFORE sha256=$(sha256sum < "$BEFORE" | cut -d' ' -f1) lines=$(wc -l < "$BEFORE")"

TW="$(mktemp -d /tmp/task-3323b-throwaway.XXXXXX)"
trap 'rm -rf "$TW" "$BEFORE" "$AFTER"' EXIT
mkdir -p "$TW/apps/osl-hub/src"
git -C "$REPO" archive "$REF" "$CHECK_DIR" | tar -x -C "$TW"
git -C "$REPO" show "$REF:$JOB_PATH" > "$TW/$JOB_PATH"
JOB="$TW/$JOB_PATH"
CRATE="$TW/$CHECK_DIR"
echo "3323b_THROWAWAY dir=$TW ref=$REF job_sha256=$(sha256sum "$JOB" | cut -d' ' -f1)"

# The job is compiled into the check through #[path], and this lane's target
# directory is on a drvfs mount whose timestamps are too coarse to be trusted
# for that: a first attempt at this run re-used a stale binary and reported the
# broken copy as green. So every phase drops this package's artifacts first and
# then insists the compiler says "Compiling" — a stale binary cannot be mistaken
# for a result here.
rebuild_3323() {
  ( cd "$CRATE" && "$CARGO" clean --offline -p task-3323-stored-protected-part \
      >/dev/null 2>&1
    "$CARGO" build --offline -j 2 --all-targets 2>&1 )
}
assert_freshly_compiled() {
  local out="$1" phase="$2"
  if printf '%s\n' "$out" | grep -q 'Compiling task-3323-stored-protected-part'; then
    echo "3323b_FRESH_BUILD phase=$phase compiled=yes"
  else
    echo "3323b_ERROR phase=$phase used a stale binary" >&2
    printf '%s\n' "$out" | tail -5 >&2
    exit 2
  fi
}
run_3323() {
  ( cd "$CRATE" && "$CARGO" run --offline --quiet -j 2 \
      --bin task-3323-stored-protected-part 2>/dev/null )
}
test_3323() {
  ( cd "$CRATE" && "$CARGO" test --offline -j 2 \
      --test task_3323_stored_protected_part -- --test-threads=1 2>&1 )
}

say "1. the throwaway is green before the break"
assert_freshly_compiled "$(rebuild_3323)" baseline
run_3323; BASE_EXIT=$?
echo "3323b_BASELINE_DIRECT_RUN_EXIT=$BASE_EXIT"
[ "$BASE_EXIT" -eq 0 ] || { echo "3323b_ERROR baseline is not green" >&2; exit 2; }
test_3323 | grep -E '^test result|^running'
echo "3323b_BASELINE_TEST_EXIT=${PIPESTATUS[0]}"

say "2. the break: skip the stored-part removal"
python3 - "$JOB" <<'PY' || exit 2
import sys
path = sys.argv[1]
src = open(path, encoding="utf-8").read()
old = """            if record.is_protected() {
                match self.take_protected_part(record, parts) {
                    Ok(removal) => pass.protected_parts_removed.push(removal),
                    Err(refusal) => {
                        pass.refused.push(refusal);
                        continue;
                    }
                }
            }
"""
new = """            // TASK 3323b BREAK: the stored-part removal is skipped here.
            let _ = &parts;
"""
if src.count(old) != 1:
    print("3323b_ERROR the stored-part removal block was not found exactly once")
    sys.exit(1)
open(path, "w", encoding="utf-8").write(src.replace(old, new))
print("3323b_BREAK_APPLIED removed_lines=9 added_lines=2")
PY
diff -u <(git -C "$REPO" show "$REF:$JOB_PATH") "$JOB" | sed -n '1,40p'

say "3. 3323 against the broken copy"
assert_freshly_compiled "$(rebuild_3323)" broken
BROKEN_OUT="$TW/broken.txt"
run_3323 > "$BROKEN_OUT" 2>&1; BROKEN_EXIT=$?
cat "$BROKEN_OUT"
echo "3323b_BROKEN_DIRECT_RUN_EXIT=$BROKEN_EXIT"
NAMED=$(grep -c "still stored on the OSL service.*$PROTECTED_ITEM\|$PROTECTED_ITEM is still stored" "$BROKEN_OUT")
echo "3323b_NAMES_STORED_ITEM count=$NAMED item=$PROTECTED_ITEM"
test_3323 | grep -E '^test result|^ +task_3323|^failures:' | head -20
echo "3323b_BROKEN_TEST_EXIT=${PIPESTATUS[0]}"

say "4. put it back"
git -C "$REPO" show "$REF:$JOB_PATH" > "$JOB"
echo "3323b_RESTORED job_sha256=$(sha256sum "$JOB" | cut -d' ' -f1)"
assert_freshly_compiled "$(rebuild_3323)" put-back
run_3323 | tail -1; GREEN_EXIT=${PIPESTATUS[0]}
echo "3323b_GREEN_AGAIN_EXIT=$GREEN_EXIT"

say "5. throwaway removed, working copy re-fingerprinted"
rm -rf "$TW"
[ -e "$TW" ] && echo "3323b_THROWAWAY_LEFT=yes" || echo "3323b_THROWAWAY_REMOVED=yes"
fingerprint > "$AFTER"
echo "3323b_WORKING_COPY_AFTER sha256=$(sha256sum < "$AFTER" | cut -d' ' -f1) lines=$(wc -l < "$AFTER")"
if diff -u "$BEFORE" "$AFTER" > /dev/null; then
  echo "3323b_WORKING_COPY_UNCHANGED=yes"
  UNCHANGED=1
else
  echo "3323b_WORKING_COPY_UNCHANGED=no"
  diff -u "$BEFORE" "$AFTER" | head -20
  UNCHANGED=0
fi

say "3323b finish line"
FAILED=0
[ "$BROKEN_EXIT" -eq 1 ] || { echo "  FAIL 3323 exited $BROKEN_EXIT, expected 1"; FAILED=1; }
[ "$NAMED" -ge 1 ]      || { echo "  FAIL 3323 did not name the still-stored item"; FAILED=1; }
[ "$UNCHANGED" -eq 1 ]  || { echo "  FAIL the working copy changed"; FAILED=1; }
[ "$GREEN_EXIT" -eq 0 ] || { echo "  FAIL 3323 was not green again after the break was put back"; FAILED=1; }
if [ "$FAILED" -eq 0 ]; then
  echo "3323b_RESULT result=PASS broken_exit=1 names_item=yes working_copy_unchanged=yes green_again=0"
  exit 0
fi
echo "3323b_RESULT result=FAIL"
exit 1
