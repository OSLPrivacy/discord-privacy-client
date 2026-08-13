#!/usr/bin/env bash
# TASK 5180b - prove the hard quarantine limits on archive expansion cannot be
# starved silently.
#
# Builds separate throwaway copies of the production archive boundary
# (apps/osl-hub/src/protected_archive.rs plus the TASK 5166 quarantine it runs
# inside, copied verbatim) and drives each one through the real release door,
# ProtectedDownloadQuarantine::scan_and_release:
#
#   real             untouched
#                    -> must exit 0: bundle.zip releases only after all six
#                       entry receipts, and every bound fixture is withheld
#   expanded-bytes   the expanded-byte guard disabled  -> must exit 1 naming bomb.zip
#   entry-count      the entry-count guard disabled    -> must exit 1 naming swarm.zip
#   nesting-depth    the nesting-depth guard disabled  -> must exit 1 naming deep.zip
#   scan-time        the wall-clock deadline disabled  -> must exit 1 naming slow.zip
#   clean-control    the per-entry receipt skipped     -> must exit 1 naming bundle.zip
#   quarantine-root  the workspace moved out of the quarantine
#                                                      -> must exit 1 naming bundle.zip
#
# TASK 5180a adds the hostile-entry guards. The two path modes replace the
# rejection with the raw component - the "normalise it into the destination"
# mistake - so the entry really is written outside the quarantine:
#
#   parent-traversal the `..` rejection replaced   -> must exit 1 naming traversal.zip
#                                                     and the outside-quarantine path
#   absolute-path    the absolute rejection replaced-> must exit 1 naming absolute.tar
#                                                     and the outside-quarantine path
#   symbolic-link    the symlink rejection disabled -> must exit 1 naming symlink.tar
#   hard-link        the hard-link rejection disabled-> must exit 1 naming hardlink.tar
#   special-file     the special-file rejection disabled
#                                                   -> must exit 1 naming special.tar
#
# This proof runs only the two starvation modes that TASK 5180b owns: the
# expanded-byte bound and the parent-traversal path guard.  Each copy still
# drives the complete real control fixture set before reporting its verdict.
# Each sabotage is verified to have actually landed in the copy before the run;
# a sed that matched nothing fails the script instead of quietly producing a
# green "starved" run. Every copy is discarded before this script returns, and
# the tracked sources are checked afterwards to be byte-identical to what they
# started as.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_ARCHIVE="$REPO_ROOT/apps/osl-hub/src/protected_archive.rs"
SOURCE_QUARANTINE="$REPO_ROOT/apps/osl-hub/src/protected_download_quarantine.rs"
SOURCE_HELPER="$REPO_ROOT/apps/osl-hub/src/protected_download_quarantine_amsi.ps1"
HARNESS="$REPO_ROOT/scripts/task-5180b"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/osl-task5180b-copies-XXXXXX")"
if [ "${CARGO_TARGET_DIR:-}" != "/mnt/d/osl-lane-targets/i" ]; then
  echo "TASK5180B_FAIL CARGO_TARGET_DIR must be /mnt/d/osl-lane-targets/i"
  exit 2
fi

ARCHIVE_SHA_BEFORE="$(sha256sum "$SOURCE_ARCHIVE" | cut -d' ' -f1)"
QUARANTINE_SHA_BEFORE="$(sha256sum "$SOURCE_QUARANTINE" | cut -d' ' -f1)"
echo "TASK5180B_HARNESS_SOURCE=$SOURCE_ARCHIVE"
echo "TASK5180B_ARCHIVE_SHA256_BEFORE=$ARCHIVE_SHA_BEFORE"
echo "TASK5180B_QUARANTINE_SHA256_BEFORE=$QUARANTINE_SHA_BEFORE"
echo "TASK5180B_THROWAWAY_ROOT=$WORK"

discard() {
  rm -rf "$WORK"
  if [ -e "$WORK" ]; then
    echo "TASK5180B_COPIES_DISCARDED=false"
  else
    echo "TASK5180B_COPIES_DISCARDED=true"
  fi
}
trap discard EXIT

STATUS=0

make_copy() {
  local name="$1"
  local dir="$WORK/$name"
  mkdir -p "$dir"
  cp "$SOURCE_ARCHIVE" "$dir/protected_archive.rs"
  cp "$SOURCE_QUARANTINE" "$dir/protected_download_quarantine.rs"
  cp "$SOURCE_HELPER" "$dir/protected_download_quarantine_amsi.ps1"
  cp "$HARNESS/main.rs" "$dir/main.rs"
  cp "$HARNESS/Cargo.toml" "$dir/Cargo.toml"
  # All copies use the lane's required CARGO_TARGET_DIR.  Give each copy its
  # own Cargo package identity as well, otherwise Cargo can execute a stale
  # sibling binary that happens to have the same package name.
  sed -i "s/name = \"task-5180b-check\"/name = \"task-5180b-check-$name\"/" "$dir/Cargo.toml"
  echo "$dir"
}

# Disable exactly one bound in one copy, and prove the edit landed.
starve() {
  local dir="$1"
  local mode="$2"
  local file="$dir/protected_archive.rs"
  case "$mode" in
    real) ;;
    expanded-bytes)
      sed -i '/TASK5180-BOUND-EXPANDED-BYTES/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    entry-count)
      sed -i '/TASK5180-BOUND-ENTRY-COUNT/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    nesting-depth)
      sed -i '/TASK5180-BOUND-NESTING-DEPTH/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    scan-time)
      sed -i '/TASK5180-BOUND-SCAN-TIME/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    clean-control)
      sed -i '/TASK5180-BOUND-CLEAN-CONTROL/{n;s|^\( *\)self.scan_entry(.*$|\1// starved by TASK 5180b: the per-entry receipt is skipped|}' "$file"
      ;;
    quarantine-root)
      sed -i '/TASK5180-BOUND-QUARANTINE-ROOT/{n;s|^\( *\)let workspace_parent = .*$|\1let workspace_parent = std::env::temp_dir(); // starved by TASK 5180b|}' "$file"
      ;;
    # --- TASK 5180a: the hostile-entry guards ---------------------------------
    # The two path guards are not merely skipped: the raw component is pushed
    # instead, which is precisely the "normalise it into the destination"
    # mistake. The entry then really is written outside the quarantine.
    parent-traversal)
      sed -i '/TASK5180A-GUARD-PARENT-TRAVERSAL/{n;s|^\( *\)return Err(.*$|\1built.push(component.as_os_str()); // starved by TASK 5180b|}' "$file"
      ;;
    absolute-path)
      sed -i '/TASK5180A-GUARD-ABSOLUTE-PATH/{n;s|^\( *\)return Err(.*$|\1built.push(component.as_os_str()); // starved by TASK 5180b|}' "$file"
      ;;
    symbolic-link)
      sed -i '/TASK5180A-GUARD-SYMBOLIC-LINK/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    hard-link)
      sed -i '/TASK5180A-GUARD-HARD-LINK/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    special-file)
      sed -i '/TASK5180A-GUARD-SPECIAL-FILE/{n;s|^\( *\)if .*{$|\1if false { // starved by TASK 5180b|}' "$file"
      ;;
    *)
      echo "TASK5180B_FAIL unknown starvation mode $mode"
      exit 2
      ;;
  esac
  local applied
  applied="$(grep -c 'starved by TASK 5180b' "$file")"
  echo "TASK5180B_SABOTAGE_LINES_APPLIED[$mode]=$applied"
  if [ "$mode" = "real" ]; then
    if [ "$applied" -ne 0 ]; then
      echo "TASK5180B_FAIL the untouched copy is not untouched"
      exit 2
    fi
  elif [ "$applied" -lt 1 ]; then
    echo "TASK5180B_FAIL the $mode sabotage matched nothing; the marker moved"
    exit 2
  fi
}

run_mode() {
  local mode="$1"
  local expected_exit="$2"
  local expected_fixture="$3"
  local dir
  dir="$(make_copy "$mode")"
  starve "$dir" "$mode"

  echo "--- TASK5180B_MODE=$mode ---"
  local output
  output="$(cd "$dir" && cargo run --quiet --release 2>&1)"
  local code=$?
  echo "$output"
  echo "TASK5180B_MODE_EXIT[$mode]=$code"

  if [ "$code" -ne "$expected_exit" ]; then
    echo "TASK5180B_VERDICT[$mode]=FAIL exit $code, expected $expected_exit"
    STATUS=1
    return
  fi
  if [ -n "$expected_fixture" ]; then
    if echo "$output" | grep -q "TASK5180B_ESCAPED_FIXTURE=$expected_fixture"; then
      echo "TASK5180B_VERDICT[$mode]=PASS exit $code naming $expected_fixture"
    else
      echo "TASK5180B_VERDICT[$mode]=FAIL exit $code but did not name $expected_fixture"
      STATUS=1
    fi
  else
    echo "TASK5180B_VERDICT[$mode]=PASS exit $code, no fixture escaped"
  fi
}

run_mode real 0 ""
run_mode expanded-bytes 1 "bomb.zip"
run_mode parent-traversal 1 "traversal.zip"

ARCHIVE_SHA_AFTER="$(sha256sum "$SOURCE_ARCHIVE" | cut -d' ' -f1)"
QUARANTINE_SHA_AFTER="$(sha256sum "$SOURCE_QUARANTINE" | cut -d' ' -f1)"
echo "TASK5180B_ARCHIVE_SHA256_AFTER=$ARCHIVE_SHA_AFTER"
echo "TASK5180B_QUARANTINE_SHA256_AFTER=$QUARANTINE_SHA_AFTER"
if [ "$ARCHIVE_SHA_BEFORE" != "$ARCHIVE_SHA_AFTER" ] ||
   [ "$QUARANTINE_SHA_BEFORE" != "$QUARANTINE_SHA_AFTER" ]; then
  echo "TASK5180B_FAIL the tracked sources were modified"
  STATUS=1
fi

echo "TASK5180B_OVERALL_EXIT=$STATUS"
exit "$STATUS"
