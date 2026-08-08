#!/usr/bin/env bash
# TASK 3308b - prove the due-only rule can fail.
#
# TASK 3308 (the timed-delete sweep job) was built on branch `lane/e`, not in
# this lane's worktree. This script never reads or writes /home/liamw/osl-exec-e:
# it pulls TASK 3308's own source out of the git object store at ref `lane/e`
# into a throwaway tree under /tmp, breaks the due rule there, runs TASK 3308's
# check against the break, restores the throwaway, and shows the check green
# again. Nothing outside the throwaway tree is ever written.
#
# What it proves, in order:
#   1. baseline: 3308's direct run exits 0, deleted=2 left=1
#   2. break:    is_due() ignores the time and returns true
#   3. red:      3308's direct run exits 1 and NAMES the not-due record
#   4. red:      3308's test target fails 4 of 10
#   5. restore:  the throwaway is put back from the same git ref
#   6. green:    3308 exits 0 again, 10/10 tests pass
#   7. unchanged: the git blob ids at ref lane/e are byte-identical to step 0
#
# Usage:  CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/g apps/osl-hub/task_3308b_due_only_break/run-3308b.sh

set -uo pipefail

REPO="$(git rev-parse --show-toplevel)"
REF="${TASK_3308B_REF:-lane/e}"
THROWAWAY="${TASK_3308B_THROWAWAY:-/tmp/task-3308b-throwaway}"
JOB_REL="apps/osl-hub/src/timed_delete_sweep_job.rs"
CRATE_REL="apps/osl-hub/task_3308_timed_delete_sweep_job"
MANIFEST="$THROWAWAY/$CRATE_REL/Cargo.toml"
TARGET_DIR="${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set (this lane uses /mnt/d/osl-lane-targets/g)}"
BIN="$TARGET_DIR/debug/task-3308-timed-delete-sweep-job"
NOT_DUE="discord-message-3308-not-due"

cd "$REPO"

say() { printf '\n== %s ==\n' "$*"; }

blob_ids() {
  git ls-tree -r "$REF" -- "$CRATE_REL" "$JOB_REL" | awk '{print $3, $4}'
}

say "0. TASK 3308 sources at ref $REF ($(git rev-parse --short "$REF"))"
BLOBS_BEFORE="$(blob_ids)"
printf '%s\n' "$BLOBS_BEFORE"

say "1. extract into the throwaway tree $THROWAWAY"
rm -rf "$THROWAWAY"
while read -r f; do
  mkdir -p "$THROWAWAY/$(dirname "$f")"
  git show "$REF:$f" > "$THROWAWAY/$f"
done < <(git ls-tree -r --name-only "$REF" -- "$CRATE_REL" "$JOB_REL")
(cd "$THROWAWAY" && find . -type f | sort | xargs sha256sum)

say "2. baseline: TASK 3308 against the unbroken copy"
cargo build --manifest-path "$MANIFEST" --locked --offline -j2 2>&1 | tail -2
"$BIN"; BASELINE_EXIT=$?
echo "baseline_exit=$BASELINE_EXIT"

say "3. break: make the job ignore the time and delete everything it finds"
python3 - "$THROWAWAY/$JOB_REL" <<'PY'
import sys
path = sys.argv[1]
src = open(path).read()
old = """    pub fn is_due(&self, now: i64, record: &DueTimedDeleteRecord) -> bool {
        record.delete_at_unix_seconds <= now
    }"""
new = """    pub fn is_due(&self, _now: i64, _record: &DueTimedDeleteRecord) -> bool {
        // TASK 3308b break: the job ignores the time and deletes everything it finds.
        true
    }"""
assert src.count(old) == 1, "the due rule is not where TASK 3308b expects it"
open(path, "w").write(src.replace(old, new))
print("broken is_due() written")
PY
sed -n '/pub fn is_due/,/^    }/p' "$THROWAWAY/$JOB_REL"

say "4. run TASK 3308 against the break"
cargo build --manifest-path "$MANIFEST" --locked --offline -j2 2>&1 | tail -2
# Captured, not piped: the binary exits 1 here on purpose and `set -o pipefail`
# would make any `"$BIN" | grep` read as a failed probe.
BROKEN_OUT="$("$BIN" 2>&1)"; BROKEN_EXIT=$?
printf '%s\n' "$BROKEN_OUT"
echo "broken_exit=$BROKEN_EXIT"
if printf '%s\n' "$BROKEN_OUT" | grep -qF "$NOT_DUE was deleted"; then NAMED=yes; else NAMED=no; fi
echo "broken_names_not_due_record=$NAMED"
echo "broken_lines_naming_not_due_record=$(printf '%s\n' "$BROKEN_OUT" | grep -cF "$NOT_DUE")"

say "5. run TASK 3308's test target against the break"
cargo test --manifest-path "$MANIFEST" --locked --offline -j2 \
  --test task_3308_timed_delete_sweep_job -- --test-threads=1 2>&1 \
  | grep -E '^test |^test result'
BROKEN_TEST_EXIT=${PIPESTATUS[0]}
echo "broken_test_exit=$BROKEN_TEST_EXIT"

say "6. restore the throwaway from the same git ref"
git show "$REF:$JOB_REL" > "$THROWAWAY/$JOB_REL"
sha256sum "$THROWAWAY/$JOB_REL"

say "7. green again"
cargo build --manifest-path "$MANIFEST" --locked --offline -j2 2>&1 | tail -2
"$BIN" | tail -3; RESTORED_EXIT=${PIPESTATUS[0]}
echo "restored_exit=$RESTORED_EXIT"
cargo test --manifest-path "$MANIFEST" --locked --offline -j2 \
  --test task_3308_timed_delete_sweep_job -- --test-threads=1 2>&1 | tail -2
RESTORED_TEST_EXIT=${PIPESTATUS[0]}
echo "restored_test_exit=$RESTORED_TEST_EXIT"

say "8. the working copy is unchanged"
rm -rf "$THROWAWAY"
BLOBS_AFTER="$(blob_ids)"
if [ "$BLOBS_BEFORE" = "$BLOBS_AFTER" ]; then
  echo "WORKING_COPY_UNCHANGED"
  printf '%s\n' "$BLOBS_AFTER"
  UNCHANGED=yes
else
  echo "WORKING_COPY_CHANGED"
  diff <(printf '%s\n' "$BLOBS_BEFORE") <(printf '%s\n' "$BLOBS_AFTER")
  UNCHANGED=no
fi
echo "throwaway_removed=$([ -e "$THROWAWAY" ] && echo no || echo yes)"

say "TASK 3308b verdict"
FAILURES=0
check() { # name expected actual
  if [ "$2" = "$3" ]; then echo "  ok   $1 = $3"; else echo "  FAIL $1 = $3 (expected $2)"; FAILURES=$((FAILURES + 1)); fi
}
check baseline_exit 0 "$BASELINE_EXIT"
check broken_exit 1 "$BROKEN_EXIT"
check broken_names_not_due_record yes "$NAMED"
check broken_test_exit_nonzero yes "$([ "$BROKEN_TEST_EXIT" -ne 0 ] && echo yes || echo no)"
check restored_exit 0 "$RESTORED_EXIT"
check restored_test_exit 0 "$RESTORED_TEST_EXIT"
check working_copy_unchanged yes "$UNCHANGED"
if [ "$FAILURES" -eq 0 ]; then
  echo "TASK3308B_RESULT result=PASS failures=0"
  exit 0
fi
echo "TASK3308B_RESULT result=FAIL failures=$FAILURES"
exit 1
