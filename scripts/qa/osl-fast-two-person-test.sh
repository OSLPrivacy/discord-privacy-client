#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="${repo_root}/scripts/qa/osl-two-copy-startup.sh"
status_cmd="${repo_root}/scripts/qa/osl-two-copy-process-status.sh"
vmqa_run="${repo_root}/scripts/vmqa/vmqa-run.sh"
direct_command="f1_live_windows_walkthrough_imports_nonempty_receipt"
metadata_name="two-copy-${direct_command}.json"

output_dir=""
root=""
identity_a="osl-copy-a-disposable"
identity_b="osl-copy-b-disposable"
password_screen_a="on"
password_screen_b="off"
timeout_seconds=60

usage() {
  cat >&2 <<'USAGE'
usage: scripts/qa/osl-fast-two-person-test.sh [options]

Launches two disposable local OSL copies, saves one status result and one VMQA
test-command metadata record for each copy, prints each identity and switch
list, then removes the disposable copy root.

options:
  --output <path>             Directory where status and metadata results are saved.
  --root <path>               Disposable copy root. Removed before success.
  --identity-a <name>         Disposable identity name for copy A.
  --identity-b <name>         Disposable identity name for copy B.
  --password-screen-a <on|off>
  --password-screen-b <on|off>
  --timeout <seconds>         Seconds to wait for both metadata records.
USAGE
  exit 2
}

need_value() {
  if [[ $# -lt 2 || -z "$2" ]]; then
    printf 'osl-fast-two-person-test: missing value for %s\n' "$1" >&2
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      need_value "$@"
      output_dir="$2"
      shift 2
      ;;
    --root)
      need_value "$@"
      root="$2"
      shift 2
      ;;
    --identity-a)
      need_value "$@"
      identity_a="$2"
      shift 2
      ;;
    --identity-b)
      need_value "$@"
      identity_b="$2"
      shift 2
      ;;
    --password-screen-a)
      need_value "$@"
      password_screen_a="$2"
      shift 2
      ;;
    --password-screen-b)
      need_value "$@"
      password_screen_b="$2"
      shift 2
      ;;
    --timeout)
      need_value "$@"
      timeout_seconds="$2"
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      printf 'osl-fast-two-person-test: unknown argument: %s\n' "$1" >&2
      usage
      ;;
  esac
done

fail() {
  printf 'osl-fast-two-person-test: %s\n' "$1" >&2
  exit 1
}

[[ "$password_screen_a" == "on" || "$password_screen_a" == "off" ]] ||
  fail "copy A password screen switch must be on or off"
[[ "$password_screen_b" == "on" || "$password_screen_b" == "off" ]] ||
  fail "copy B password screen switch must be on or off"
[[ "$timeout_seconds" =~ ^[0-9]+$ && "$timeout_seconds" -gt 0 ]] ||
  fail "timeout must be a positive integer"

if [[ -z "$output_dir" ]]; then
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  output_dir="${repo_root}/docs/reports/vmqa/two-person-starter/${stamp}-$$"
fi
if [[ -z "$root" ]]; then
  root="$(mktemp -d "${TMPDIR:-/tmp}/osl-fast-two-person-test.XXXXXX")"
fi

output_dir="$(realpath -m -- "$output_dir")"
root="$(realpath -m -- "$root")"
mkdir -p -- "$output_dir"

pid_a=""
pid_b=""
cleanup_done=false

stop_pid() {
  local pid="${1:-}"
  if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
      local state
      state="$({ ps -p "$pid" -o stat= 2>/dev/null || true; } | sed -n '1p' | tr -d ' ')"
      if [[ -z "$state" || "$state" == Z* ]]; then
        return 0
      fi
      sleep 0.2
    done
    kill -KILL "$pid" 2>/dev/null || true
  fi
}

pid_absent() {
  local pid="${1:-}"
  local state
  if [[ ! "$pid" =~ ^[0-9]+$ ]]; then
    printf 'true\n'
    return 0
  fi
  state="$({ ps -p "$pid" -o stat= 2>/dev/null || true; } | sed -n '1p' | tr -d ' ')"
  if [[ -z "$state" || "$state" == Z* ]]; then
    printf 'true\n'
  else
    printf 'false\n'
  fi
}

cleanup() {
  if [[ "$cleanup_done" == "true" ]]; then
    return 0
  fi
  cleanup_done=true
  stop_pid "$pid_a"
  stop_pid "$pid_b"
  rm -rf -- "$root"
}
trap cleanup EXIT

field_value() {
  local key="$1"
  local file="$2"
  sed -n "s/.*${key}=\\([^ ]*\\).*/\\1/p" "$file"
}

wait_for_file() {
  local path="$1"
  local deadline=$((SECONDS + timeout_seconds))
  while (( SECONDS <= deadline )); do
    [[ -f "$path" ]] && return 0
    sleep 0.2
  done
  return 1
}

print_metadata_summary() {
  local copy="$1"
  local identity="$2"
  local metadata="$3"
  python3 - "$copy" "$identity" "$metadata" <<'PY'
import json
import sys

copy, identity, metadata_path = sys.argv[1:4]
record = json.loads(open(metadata_path, encoding="utf-8").read())
switches = record.get("switches")
if not isinstance(switches, list):
    raise SystemExit("metadata switches field is not a list")
print(f"TASK0064 two-person-starter identity copy={copy} identity_name={identity}")
print(
    f"TASK0064 two-person-starter switches copy={copy} "
    f"switch_count={len(switches)} switches={json.dumps(switches, separators=(',', ':'))}"
)
PY
}

plan_file="${output_dir}/launch-plan.txt"
command_script='
set -Eeuo pipefail
case "$OSL_COPY_NAME" in
  "OSL Copy A") export OSL_PASSWORD_SCREEN="${TASK0064_PASSWORD_SCREEN_A}" ;;
  "OSL Copy B") export OSL_PASSWORD_SCREEN="${TASK0064_PASSWORD_SCREEN_B}" ;;
  *) echo "unknown copy name: ${OSL_COPY_NAME}" >&2; exit 2 ;;
esac
export VMQA_TEST_METADATA_DIR="${OSL_COPY_DATA_DIR}/metadata"
set +e
"${TASK0064_VMQA_RUN}" "${TASK0064_DIRECT_COMMAND}" >"${OSL_COPY_DATA_DIR}/vmqa.stdout" 2>"${OSL_COPY_DATA_DIR}/vmqa.stderr"
rc=$?
set -e
printf "%s\n" "$rc" >"${OSL_COPY_DATA_DIR}/vmqa.exit"
touch "${OSL_COPY_DATA_DIR}/vmqa.ready"
trap "exit 0" TERM INT
while :; do sleep 1; done
'

TASK0064_VMQA_RUN="$vmqa_run" \
TASK0064_DIRECT_COMMAND="$direct_command" \
TASK0064_PASSWORD_SCREEN_A="$password_screen_a" \
TASK0064_PASSWORD_SCREEN_B="$password_screen_b" \
"$launcher" \
  --root "$root" \
  --identity-a "$identity_a" \
  --identity-b "$identity_b" \
  -- bash -c "$command_script" >"$plan_file"
cat "$plan_file"

data_a="$(field_value "data_folder" "$plan_file" | sed -n '1p')"
data_b="$(field_value "data_folder" "$plan_file" | sed -n '2p')"
identity_a="$(field_value "identity_name" "$plan_file" | sed -n '1p')"
identity_b="$(field_value "identity_name" "$plan_file" | sed -n '2p')"
pid_a="$(field_value "pid" "$plan_file" | sed -n '1p')"
pid_b="$(field_value "pid" "$plan_file" | sed -n '2p')"

[[ -n "$data_a" && -n "$data_b" ]] || fail "launcher did not print both data folders"
[[ -n "$identity_a" && -n "$identity_b" ]] || fail "launcher did not print both identities"
[[ -n "$pid_a" && -n "$pid_b" ]] || fail "launcher did not print both process numbers"

wait_for_file "${data_a}/vmqa.ready" || fail "timed out waiting for copy A metadata"
wait_for_file "${data_b}/vmqa.ready" || fail "timed out waiting for copy B metadata"

rc_a="$(sed -n '1p' "${data_a}/vmqa.exit")"
rc_b="$(sed -n '1p' "${data_b}/vmqa.exit")"
[[ "$rc_a" == "0" ]] || fail "copy A VMQA command returned ${rc_a:-<empty>}; see ${data_a}/vmqa.stderr"
[[ "$rc_b" == "0" ]] || fail "copy B VMQA command returned ${rc_b:-<empty>}; see ${data_b}/vmqa.stderr"

"$status_cmd" --copy A --data "$data_a" --identity-name "$identity_a" >"${output_dir}/status-A.txt"
"$status_cmd" --copy B --data "$data_b" --identity-name "$identity_b" >"${output_dir}/status-B.txt"
cat "${output_dir}/status-A.txt"
cat "${output_dir}/status-B.txt"

metadata_a="${data_a}/metadata/${metadata_name}"
metadata_b="${data_b}/metadata/${metadata_name}"
[[ -f "$metadata_a" ]] || fail "missing copy A metadata: ${metadata_a}"
[[ -f "$metadata_b" ]] || fail "missing copy B metadata: ${metadata_b}"
cp -- "$metadata_a" "${output_dir}/metadata-A.json"
cp -- "$metadata_b" "${output_dir}/metadata-B.json"

print_metadata_summary A "$identity_a" "${output_dir}/metadata-A.json"
print_metadata_summary B "$identity_b" "${output_dir}/metadata-B.json"
printf 'TASK0064 two-person-starter saved_status_results=2 files=%q,%q\n' \
  "${output_dir}/status-A.txt" "${output_dir}/status-B.txt"
printf 'TASK0064 two-person-starter saved_metadata_records=2 files=%q,%q\n' \
  "${output_dir}/metadata-A.json" "${output_dir}/metadata-B.json"

cleanup
trap - EXIT

pid_a_absent="$(pid_absent "$pid_a")"
pid_b_absent="$(pid_absent "$pid_b")"
[[ "$pid_a_absent" == "true" ]] || fail "copy A process remains after cleanup: ${pid_a}"
[[ "$pid_b_absent" == "true" ]] || fail "copy B process remains after cleanup: ${pid_b}"
[[ ! -e "$root" ]] || fail "disposable copy root remains after cleanup: ${root}"
printf 'TASK0064 two-person-starter cleanup pid_a_absent=%s pid_b_absent=%s root_exists=false leftover_copies=0\n' \
  "$pid_a_absent" "$pid_b_absent"
printf 'TASK0064 two-person-starter output=%q\n' "$output_dir"
