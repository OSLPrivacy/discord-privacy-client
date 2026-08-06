#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "$script_dir/../.." && pwd)"
required_target_dir="$repo/.cargo-target"
target_dir="${CARGO_TARGET_DIR:-}"
switches=()
command_args=()

usage() {
  printf 'usage: %s [--switch name=value ...] [--] [command ...]\n' "$0" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --switch)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      switches+=("$2")
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      command_args=("$@")
      break
      ;;
    -*)
      usage
      exit 64
      ;;
    *)
      command_args=("$@")
      break
      ;;
  esac
done

if [ "$target_dir" != "$required_target_dir" ]; then
  printf 'fast-direct-test-start: CARGO_TARGET_DIR must be %s\n' "$required_target_dir" >&2
  exit 64
fi

cargo --locked build -j1 \
  --manifest-path "$repo/apps/osl-hub/Cargo.toml" \
  --no-default-features \
  --features core \
  --example runtime_switch_reader

default_command=(
  "$target_dir/debug/examples/runtime_switch_reader"
  status
  --hold-after-print-until-killed
)
if [ "${#command_args[@]}" -eq 0 ]; then
  command_args=("${default_command[@]}")
fi

if [ "${#switches[@]}" -gt 0 ]; then
  switch_string="${switches[*]}"
else
  switch_string="${OSL_TEST_ONLY_RUNTIME_SWITCHES:-}"
fi

tmp_dir="$(mktemp -d)"
child_stdout="$tmp_dir/child.out"
child_stderr="$tmp_dir/child.err"
pid_file="${OSL_FAST_DIRECT_TEST_PID_FILE:-}"
disable_cleanup="${OSL_FAST_DIRECT_TEST_DISABLE_CLEANUP:-0}"
child_pid=""

cleanup_tmp() {
  rm -rf -- "$tmp_dir"
}
trap cleanup_tmp EXIT

: >"$child_stdout"
: >"$child_stderr"
OSL_TEST_ONLY_RUNTIME_SWITCHES="$switch_string" "${command_args[@]}" \
  >"$child_stdout" 2>"$child_stderr" &
child_pid="$!"
if [ -n "$pid_file" ]; then
  printf '%s\n' "$child_pid" >"$pid_file"
fi

deadline=$((SECONDS + 10))
while [ "$SECONDS" -lt "$deadline" ]; do
  if grep -Fq 'RUN-TIME SWITCH STATUS: all-defaults-safe' "$child_stdout"; then
    break
  fi
  if ! kill -0 "$child_pid" 2>/dev/null; then
    cat "$child_stdout"
    cat "$child_stderr" >&2
    printf 'fast-direct-test-start: direct command exited before switch status was printed\n' >&2
    exit 1
  fi
  sleep 0.1
done

cat "$child_stdout"
cat "$child_stderr" >&2

if ! grep -Fq 'RUN-TIME SWITCH STATUS: all-defaults-safe' "$child_stdout"; then
  printf 'fast-direct-test-start: direct command did not print safe switch status\n' >&2
  exit 1
fi

printf 'TASK0062_DIRECT_COMMAND_COUNT=1\n'
printf 'TASK0062_OSL_PROCESS_PID=%s\n' "$child_pid"
printf 'TASK0062_RUNTIME_SWITCHES="%s"\n' "$switch_string"

if [ "$disable_cleanup" != "1" ] && kill -0 "$child_pid" 2>/dev/null; then
  kill "$child_pid" 2>/dev/null || true
  wait "$child_pid" 2>/dev/null || true
fi

if kill -0 "$child_pid" 2>/dev/null; then
  printf 'TASK0062_OSL_PROCESS_COUNT_AFTER_CLEANUP=1\n'
  printf 'fast-direct-test-start: OSL process cleanup check failed for pid=%s\n' "$child_pid" >&2
  exit 1
fi

wait "$child_pid" 2>/dev/null || true
printf 'TASK0062_OSL_PROCESS_COUNT_AFTER_CLEANUP=0\n'
