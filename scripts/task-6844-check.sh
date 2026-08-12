#!/usr/bin/env bash
# TASK 6844 check — a real supported Windows capture while a view-once viewer
# is open produces exactly one authenticated sender notification after
# reconnect and restart, and nothing else does.
#
# Runs from WSL against the real Windows desktop through interop. The two
# phases are two separate Windows processes on purpose: "the event survived a
# restart" is not something one process can assert about itself.
#
#   scripts/task-6844-check.sh                 # green run
#   scripts/task-6844-check.sh --starve <item> # remove one ingredient; must go red
#
# TASK 6845 runs this same check against a throwaway copy of the crate, so the
# source it builds and the directory it runs in are both overridable:
#
#   scripts/task-6844-check.sh --manifest <copy>/Cargo.toml --run-name osl6845-<mutation>
#
# Exit 0 green, 1 red.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# A non-login shell does not get the rustup shim directory.
PATH="$HOME/.cargo/bin:$PATH"
export PATH
TARGET_DIR="${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/c}"
TRIPLE="x86_64-pc-windows-gnu"
STARVE=""
# Empty means "the source in this worktree".
MANIFEST=""
RUN_NAME="osl6844-check-run"
# A build of another source tree must not be able to leave its binary where
# this one looks for it. Each caller building a different tree passes its own
# subdirectory of the lane target directory.
TARGET_SUB=""

while [ $# -gt 0 ]; do
  case "$1" in
    --starve) STARVE="${2:-}"; shift 2 ;;
    --manifest) MANIFEST="${2:-}"; shift 2 ;;
    --run-name) RUN_NAME="${2:-}"; shift 2 ;;
    --target-sub) TARGET_SUB="${2:-}"; shift 2 ;;
    *) echo "6844 unknown argument $1" >&2; exit 2 ;;
  esac
done

if [ -n "$MANIFEST" ] && [ ! -f "$MANIFEST" ]; then
  echo "6844 FAIL there is no manifest at $MANIFEST to build the check from"
  echo "6844 RESULT=RED"
  exit 1
fi

[ -n "$TARGET_SUB" ] && TARGET_DIR="${TARGET_DIR%/}/$TARGET_SUB"

# The Windows side needs a Windows path. The lane target directory lives on D:.
case "$TARGET_DIR" in
  /mnt/[a-z]/*)
    DRIVE="$(printf '%s' "$TARGET_DIR" | cut -c6)"
    WIN_TARGET="$(printf '%s:%s' "$(printf '%s' "$DRIVE" | tr '[:lower:]' '[:upper:]')" "${TARGET_DIR#/mnt/?}" | tr '/' '\\')"
    ;;
  *)
    echo "6844 FAIL the target directory $TARGET_DIR is not on a Windows drive, so the check cannot run there" >&2
    exit 1
    ;;
esac

RUN_DIR_LINUX="/mnt/${DRIVE}/${RUN_NAME}"
RUN_DIR_WIN="$(printf '%s:\\%s' "$(printf '%s' "$DRIVE" | tr '[:lower:]' '[:upper:]')" "$RUN_NAME")"
CHECK_EXE="${WIN_TARGET}\\${TRIPLE}\\debug\\task-6844-check.exe"
CHECK_EXE_LINUX="${TARGET_DIR}/${TRIPLE}/debug/task-6844-check.exe"

if [ -n "$MANIFEST" ]; then
  echo "6844 build target=${TRIPLE} source=${MANIFEST}"
else
  echo "6844 build target=${TRIPLE}"
fi
BUILD_LOG="$(mktemp)"
if [ -n "$MANIFEST" ]; then
  SOURCE_DIR="$(cd "$(dirname "$MANIFEST")" && pwd)/crates/view-once-capture/src"
else
  SOURCE_DIR="$REPO_ROOT/crates/view-once-capture/src"
fi
# sccache cannot set permissions on the DrvFs target directory and kills the
# build; this check does not need it.
if [ -n "$MANIFEST" ]; then
  RUSTC_WRAPPER= CARGO_TARGET_DIR="$TARGET_DIR" \
    cargo build --manifest-path "$MANIFEST" -p view-once-capture \
      --target "$TRIPLE" --bins >"$BUILD_LOG" 2>&1
  BUILD_EXIT=$?
else
  ( cd "$REPO_ROOT" && RUSTC_WRAPPER= CARGO_TARGET_DIR="$TARGET_DIR" \
      cargo build -p view-once-capture --target "$TRIPLE" --bins ) >"$BUILD_LOG" 2>&1
  BUILD_EXIT=$?
fi
if [ "$BUILD_EXIT" -ne 0 ]; then
  echo "6844 FAIL the Windows check binaries did not build"
  grep -m3 '^error' "$BUILD_LOG"
  rm -f "$BUILD_LOG"
  echo "6844 RESULT=RED"
  exit 1
fi
rm -f "$BUILD_LOG"
# The binary has to be newer than every source file it was built from. All the
# builds share one target directory, so a binary left by a previous build of a
# *different* copy would otherwise be run against the source just named — which
# is exactly how a mutation proof silently turns into a re-run of the
# unmutated check.
STALE="$(find "$SOURCE_DIR" -type f -newer "$CHECK_EXE_LINUX" -print -quit 2>/dev/null)"
if [ -n "$STALE" ]; then
  echo "6844 FAIL the check binary predates $STALE, so it is not built from the source named"
  echo "6844 RESULT=RED"
  exit 1
fi
EXPECTED_SOURCE="${SOURCE_DIR%/src}"

rm -rf "$RUN_DIR_LINUX"
mkdir -p "$RUN_DIR_LINUX"

PHASE_LOG="$(mktemp)"
trap 'rm -f "$PHASE_LOG"' EXIT

# cmd.exe refuses a UNC working directory, so run from a Windows-visible one.
# stdin is closed: the Windows child inherits it, and a caller looping over a
# list on stdin would otherwise have that list eaten by the first phase.
run_phase() {
  local phase="$1"
  local starve_argument=""
  [ -n "$STARVE" ] && starve_argument="--starve $STARVE"
  ( cd /tmp && \
      timeout 300 cmd.exe /c "$CHECK_EXE --dir $RUN_DIR_WIN --phase $phase $starve_argument" \
      </dev/null 2>&1 ) \
    | tr -d '\r' | grep -v 'UNC paths\|CMD.EXE was started\|wsl.localhost' >"$PHASE_LOG"
  local code="${PIPESTATUS[0]}"
  cat "$PHASE_LOG"
  # The binary reports the tree it was compiled from. A build that cargo
  # considered fresh does not re-copy its artifact, so without this the binary
  # left by the previous build of a *different* tree would run here and its
  # verdict would be attributed to this source.
  local built_from
  built_from="$(grep -m1 '^6844 built_from=' "$PHASE_LOG" | sed 's/^6844 built_from=//')"
  if [ "$built_from" != "$EXPECTED_SOURCE" ]; then
    echo "6844 FAIL the check that ran was built from ${built_from:-a binary that does not say}, not $EXPECTED_SOURCE"
    return 90
  fi
  return "$code"
}

[ -n "$STARVE" ] && echo "6844 starving item=$STARVE"

run_phase open
OPEN_EXIT=$?
if [ "$OPEN_EXIT" -ne 0 ]; then
  echo "6844 phase=open exit=$OPEN_EXIT"
  echo "6844 RESULT=RED"
  exit 1
fi

run_phase deliver
DELIVER_EXIT=$?
if [ "$DELIVER_EXIT" -ne 0 ]; then
  echo "6844 phase=deliver exit=$DELIVER_EXIT"
  echo "6844 RESULT=RED"
  exit 1
fi

echo "6844 phase=open exit=0 phase=deliver exit=0"
echo "6844 RESULT=GREEN"
exit 0
