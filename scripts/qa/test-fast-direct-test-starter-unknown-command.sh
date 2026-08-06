#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "$script_dir/../.." && pwd)"
starter="$repo/scripts/qa/start-fast-direct-test.sh"
required_target_dir="$repo/.cargo-target"
unknown_command="definitely-not-an-app-command"

if [ "${CARGO_TARGET_DIR:-}" != "$required_target_dir" ]; then
  printf 'test-fast-direct-test-starter-unknown-command: CARGO_TARGET_DIR must be %s\n' "$required_target_dir" >&2
  exit 64
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf -- "$tmp_dir"
}
trap cleanup EXIT

require_line() {
  local pattern="$1"
  local file="$2"
  if ! grep -Fq -- "$pattern" "$file"; then
    printf 'missing required line: %s\n' "$pattern" >&2
    exit 1
  fi
}

out="$tmp_dir/unknown.out"
err="$tmp_dir/unknown.err"

set +e
"$starter" -- \
  "$required_target_dir/debug/examples/runtime_switch_reader" \
  "$unknown_command" \
  >"$out" 2>"$err"
starter_rc="$?"
set -e

cat "$out"
cat "$err"

if [ "$starter_rc" -ne 1 ]; then
  printf 'TASK0065_UNKNOWN_COMMAND_EXPECTED_EXIT_1_ACTUAL=%s\n' "$starter_rc" >&2
  exit 1
fi

require_line 'RUN-TIME SWITCH STATUS: count=2' "$out"
require_line 'SWITCH password_screen_access value=require-password-screen default=safe sending=not-required' "$out"
require_line 'SWITCH safe_sending value=live-send-requires-authority default=safe sending=disabled-without-authority' "$out"
require_line 'RUN-TIME SWITCH STATUS: all-defaults-safe' "$out"
require_line 'TASK0062_DIRECT_COMMAND_COUNT=1' "$out"
require_line 'TASK0062_RUNTIME_SWITCHES=""' "$out"
require_line 'TASK0062_DIRECT_COMMAND_EXIT=1' "$out"
require_line 'TASK0062_OSL_PROCESS_COUNT_AFTER_CLEANUP=0' "$out"
require_line "unknown app command: $unknown_command" "$err"
require_line 'fast-direct-test-start: direct command exited with status 1' "$err"

pid="$(sed -n 's/^TASK0062_OSL_PROCESS_PID=//p' "$out" | head -n 1)"
if ! printf '%s\n' "$pid" | grep -Eq '^[0-9]+$'; then
  printf 'TASK0065_METADATA_PID_NOT_NUMERIC=%s\n' "$pid" >&2
  exit 1
fi
if kill -0 "$pid" 2>/dev/null; then
  printf 'TASK0065_OSL_PROCESS_ALIVE_AFTER=1\n' >&2
  exit 1
fi

printf 'TASK0065_UNKNOWN_APP_COMMAND=%s\n' "$unknown_command"
printf 'TASK0065_STARTER_EXIT=%s\n' "$starter_rc"
printf 'TASK0065_METADATA_DIRECT_COMMAND_COUNT=1\n'
printf 'TASK0065_METADATA_RUNTIME_SWITCHES=""\n'
printf 'TASK0065_METADATA_DIRECT_COMMAND_EXIT=1\n'
printf 'TASK0065_CLEANUP_OSL_PROCESS_COUNT=0\n'
printf 'TASK0065_OSL_PROCESS_ALIVE_AFTER=0\n'
