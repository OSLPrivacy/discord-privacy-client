#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "$script_dir/../.." && pwd)"
starter="$repo/scripts/qa/start-fast-direct-test.sh"
required_target_dir="$repo/.cargo-target"

if [ "${CARGO_TARGET_DIR:-}" != "$required_target_dir" ]; then
  printf 'test-fast-direct-test-starter: CARGO_TARGET_DIR must be %s\n' "$required_target_dir" >&2
  exit 64
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  if [ -f "$tmp_dir/bad.pid" ]; then
    pid="$(sed -n '1p' "$tmp_dir/bad.pid")"
    stop_pid "$pid" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$tmp_dir"
}
trap cleanup EXIT

stop_pid() {
  local pid="$1"
  if ! printf '%s\n' "$pid" | grep -Eq '^[0-9]+$'; then
    return 0
  fi
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 30); do
      if ! kill -0 "$pid" 2>/dev/null; then
        return 0
      fi
      sleep 0.1
    done
    kill -9 "$pid" 2>/dev/null || true
    for _ in $(seq 1 30); do
      if ! kill -0 "$pid" 2>/dev/null; then
        return 0
      fi
      sleep 0.1
    done
  fi
  return 0
}

require_line() {
  local pattern="$1"
  local file="$2"
  if ! grep -Fq -- "$pattern" "$file"; then
    printf 'missing required line: %s\n' "$pattern" >&2
    exit 1
  fi
}

green="$tmp_dir/green.out"
"$starter" >"$green"
cat "$green"

require_line 'RUN-TIME SWITCH STATUS: count=2' "$green"
require_line 'SWITCH password_screen_access value=require-password-screen default=safe sending=not-required' "$green"
require_line 'SWITCH safe_sending value=live-send-requires-authority default=safe sending=disabled-without-authority' "$green"
require_line 'RUN-TIME SWITCH STATUS: all-defaults-safe' "$green"
require_line 'TASK0062_DIRECT_COMMAND_COUNT=1' "$green"
require_line 'TASK0062_OSL_PROCESS_COUNT_AFTER_CLEANUP=0' "$green"

green_pid="$(sed -n 's/^TASK0062_OSL_PROCESS_PID=//p' "$green" | head -n 1)"
if ! printf '%s\n' "$green_pid" | grep -Eq '^[0-9]+$'; then
  printf 'TASK0062_GREEN_PID_NOT_NUMERIC=%s\n' "$green_pid" >&2
  exit 1
fi
if kill -0 "$green_pid" 2>/dev/null; then
  printf 'TASK0062_GREEN_OSL_PROCESS_ALIVE_AFTER=1\n' >&2
  exit 1
fi
printf 'TASK0062_GREEN_OSL_PROCESS_ALIVE_AFTER=0\n'

bad_out="$tmp_dir/bad.out"
bad_err="$tmp_dir/bad.err"
set +e
OSL_FAST_DIRECT_TEST_DISABLE_CLEANUP=1 \
OSL_FAST_DIRECT_TEST_PID_FILE="$tmp_dir/bad.pid" \
  "$starter" >"$bad_out" 2>"$bad_err"
bad_rc="$?"
set -e

cat "$bad_out"
cat "$bad_err"

if [ "$bad_rc" -ne 1 ]; then
  printf 'TASK0062_DISABLED_CLEANUP_DID_NOT_FAIL rc=%s\n' "$bad_rc" >&2
  exit 1
fi
require_line 'TASK0062_OSL_PROCESS_COUNT_AFTER_CLEANUP=1' "$bad_out"
require_line 'fast-direct-test-start: OSL process cleanup check failed' "$bad_err"
printf 'TASK0062_DISABLED_CLEANUP_EXIT=%s\n' "$bad_rc"

bad_pid="$(sed -n '1p' "$tmp_dir/bad.pid")"
stop_pid "$bad_pid"
if kill -0 "$bad_pid" 2>/dev/null; then
  printf 'TASK0062_DISABLED_CLEANUP_LEFTOVER_AFTER_TEST=1\n' >&2
  exit 1
fi
printf 'TASK0062_DISABLED_CLEANUP_LEFTOVER_AFTER_TEST=0\n'
