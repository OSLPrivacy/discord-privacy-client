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
# Exit 0 green, 1 red.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# A non-login shell does not get the rustup shim directory.
PATH="$HOME/.cargo/bin:$PATH"
export PATH
TARGET_DIR="${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/c}"
TRIPLE="x86_64-pc-windows-gnu"
STARVE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --starve) STARVE="${2:-}"; shift 2 ;;
    *) echo "6844 unknown argument $1" >&2; exit 2 ;;
  esac
done

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

RUN_DIR_LINUX="/mnt/${DRIVE}/osl6844-check-run"
RUN_DIR_WIN="$(printf '%s:\\osl6844-check-run' "$(printf '%s' "$DRIVE" | tr '[:lower:]' '[:upper:]')")"
CHECK_EXE="${WIN_TARGET}\\${TRIPLE}\\debug\\task-6844-check.exe"

echo "6844 build target=${TRIPLE}"
# sccache cannot set permissions on the DrvFs target directory and kills the
# build; this check does not need it.
if ! (cd "$REPO_ROOT" && RUSTC_WRAPPER= CARGO_TARGET_DIR="$TARGET_DIR" \
      cargo build -p view-once-capture --target "$TRIPLE" --bins >/dev/null 2>&1); then
  echo "6844 FAIL the Windows check binaries did not build"
  echo "6844 RESULT=RED"
  exit 1
fi

rm -rf "$RUN_DIR_LINUX"
mkdir -p "$RUN_DIR_LINUX"

# cmd.exe refuses a UNC working directory, so run from a Windows-visible one.
run_phase() {
  local phase="$1"
  local starve_argument=""
  [ -n "$STARVE" ] && starve_argument="--starve $STARVE"
  ( cd /tmp && \
      timeout 300 cmd.exe /c "$CHECK_EXE --dir $RUN_DIR_WIN --phase $phase $starve_argument" 2>&1 ) \
    | tr -d '\r' | grep -v 'UNC paths\|CMD.EXE was started\|wsl.localhost'
  return "${PIPESTATUS[0]}"
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
