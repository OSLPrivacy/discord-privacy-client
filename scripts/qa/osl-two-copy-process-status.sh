#!/usr/bin/env bash
set -Eeuo pipefail

copy=""
data_dir=""
identity_name=""

usage() {
  cat >&2 <<'USAGE'
usage: scripts/qa/osl-two-copy-process-status.sh --copy <A|B> --data <path> --identity-name <name>

Calls identity status for one copy, verifies its recorded child process is still
running, and prints the direct process-number status line for TASK0031.
USAGE
  exit 2
}

need_value() {
  if [[ $# -lt 2 || -z "$2" ]]; then
    printf 'osl-two-copy-process-status: missing value for %s\n' "$1" >&2
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --copy)
      need_value "$@"
      copy="$2"
      shift 2
      ;;
    --data)
      need_value "$@"
      data_dir="$2"
      shift 2
      ;;
    --identity-name)
      need_value "$@"
      identity_name="$2"
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      printf 'osl-two-copy-process-status: unknown argument: %s\n' "$1" >&2
      usage
      ;;
  esac
done

fail_contract() {
  printf 'osl-two-copy-process-status: %s\n' "$1" >&2
  exit 1
}

[[ "$copy" == "A" || "$copy" == "B" ]] || fail_contract "copy must be A or B"
[[ -n "$data_dir" ]] || fail_contract "data folder is empty"
[[ -n "$identity_name" ]] || fail_contract "disposable identity name is empty"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
data_dir="$(realpath -m -- "$data_dir")"
pid_file="${data_dir}/pid"

"${repo_root}/scripts/qa/osl-two-copy-identity-status.sh" \
  --copy "$copy" \
  --data "$data_dir" \
  --identity-name "$identity_name"

[[ -f "$pid_file" ]] || fail_contract "missing pid file for copy ${copy}: ${pid_file}"
pid="$(sed -n '1p' "$pid_file")"
[[ "$pid" =~ ^[0-9]+$ ]] || fail_contract "invalid process number for copy ${copy}: ${pid:-<empty>}"
process_state="$({ ps -p "$pid" -o stat= 2>/dev/null || true; } | sed -n '1p' | tr -d ' ')"
[[ -n "$process_state" ]] || fail_contract "copy ${copy} process ${pid} is not running"
[[ "$process_state" != Z* ]] || fail_contract "copy ${copy} process ${pid} is not running"

printf 'TASK0031 two-copy-process status copy=%s pid=%s alive=true data_folder=%q\n' \
  "$copy" "$pid" "$data_dir"
