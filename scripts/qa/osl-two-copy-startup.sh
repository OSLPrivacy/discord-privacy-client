#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

name_a="OSL Copy A"
name_b="OSL Copy B"
identity_a="osl-copy-a-disposable"
identity_b="osl-copy-b-disposable"
port_a="47291"
port_b="47292"
root="${TMPDIR:-/tmp}/osl-two-copy-startup"
data_a=""
data_b=""
command_args=()

usage() {
  cat >&2 <<'USAGE'
usage: scripts/qa/osl-two-copy-startup.sh [options] [-- <command> [args...]]

Starts or prints a two-copy OSL startup plan with isolated names, ports, and
data folders. Without a command it validates and prints the launch contract.

options:
  --root <path>       Root used for default per-copy data folders.
  --name-a <name>     Display/test name for copy A.
  --name-b <name>     Display/test name for copy B.
  --identity-a <name> Disposable test identity name for copy A.
  --identity-b <name> Disposable test identity name for copy B.
  --port-a <port>     Loopback/control port for copy A.
  --port-b <port>     Loopback/control port for copy B.
  --data-a <path>     Data folder for copy A.
  --data-b <path>     Data folder for copy B.
USAGE
  exit 2
}

need_value() {
  if [[ $# -lt 2 || -z "$2" ]]; then
    printf 'osl-two-copy-startup: missing value for %s\n' "$1" >&2
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      need_value "$@"
      root="$2"
      shift 2
      ;;
    --name-a)
      need_value "$@"
      name_a="$2"
      shift 2
      ;;
    --name-b)
      need_value "$@"
      name_b="$2"
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
    --port-a)
      need_value "$@"
      port_a="$2"
      shift 2
      ;;
    --port-b)
      need_value "$@"
      port_b="$2"
      shift 2
      ;;
    --data-a)
      need_value "$@"
      data_a="$2"
      shift 2
      ;;
    --data-b)
      need_value "$@"
      data_b="$2"
      shift 2
      ;;
    --)
      shift
      command_args=("$@")
      break
      ;;
    -h|--help)
      usage
      ;;
    *)
      printf 'osl-two-copy-startup: unknown argument: %s\n' "$1" >&2
      usage
      ;;
  esac
done

if [[ -z "$data_a" ]]; then
  data_a="${root}/copy-a"
fi
if [[ -z "$data_b" ]]; then
  data_b="${root}/copy-b"
fi

normalize_path() {
  realpath -m -- "$1"
}

valid_port() {
  [[ "$1" =~ ^[0-9]+$ ]] && (( "$1" >= 1 && "$1" <= 65535 ))
}

valid_identity_name() {
  [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]]
}

fail_contract() {
  printf 'osl-two-copy-startup: %s\n' "$1" >&2
  exit 1
}

[[ -n "$name_a" ]] || fail_contract "copy A name is empty"
[[ -n "$name_b" ]] || fail_contract "copy B name is empty"
[[ "$name_a" != "$name_b" ]] || fail_contract "shared name: ${name_a}"
[[ -n "$identity_a" ]] || fail_contract "copy A disposable identity name is empty"
[[ -n "$identity_b" ]] || fail_contract "copy B disposable identity name is empty"
valid_identity_name "$identity_a" || fail_contract "copy A disposable identity name must use only letters, numbers, dot, underscore, or hyphen"
valid_identity_name "$identity_b" || fail_contract "copy B disposable identity name must use only letters, numbers, dot, underscore, or hyphen"
[[ "$identity_a" != "$identity_b" ]] || fail_contract "shared disposable identity name: ${identity_a}"
valid_port "$port_a" || fail_contract "invalid copy A port: ${port_a}"
valid_port "$port_b" || fail_contract "invalid copy B port: ${port_b}"
[[ "$port_a" != "$port_b" ]] || fail_contract "shared port: ${port_a}"

data_a="$(normalize_path "$data_a")"
data_b="$(normalize_path "$data_b")"
[[ "$data_a" != "$data_b" ]] || fail_contract "shared data folder: ${data_a}"

printf 'TASK0029 two-copy-startup copy=A name=%q identity_name=%q port=%s data_folder=%q\n' \
  "$name_a" "$identity_a" "$port_a" "$data_a"
printf 'TASK0029 two-copy-startup copy=B name=%q identity_name=%q port=%s data_folder=%q\n' \
  "$name_b" "$identity_b" "$port_b" "$data_b"
printf 'TASK0029 two-copy-startup different_data_folders=true\n'
printf 'TASK0029 two-copy-startup different_ports=true\n'
printf 'TASK0030 two-copy-identity different_identity_names=true\n'

if [[ ${#command_args[@]} -eq 0 ]]; then
  exit 0
fi

mkdir -p \
  "${data_a}/config" "${data_a}/data" "${data_a}/local" "${data_a}/roaming" \
  "${data_b}/config" "${data_b}/data" "${data_b}/local" "${data_b}/roaming"

env \
  OSL_COPY_NAME="$name_a" \
  OSL_DISPOSABLE_IDENTITY_NAME="$identity_a" \
  OSL_COPY_PORT="$port_a" \
  OSL_COPY_DATA_DIR="$data_a" \
  XDG_CONFIG_HOME="${data_a}/config" \
  XDG_DATA_HOME="${data_a}/data" \
  APPDATA="${data_a}/roaming" \
  LOCALAPPDATA="${data_a}/local" \
  "${command_args[@]}" >"${data_a}/stdout.log" 2>"${data_a}/stderr.log" &
pid_a=$!

env \
  OSL_COPY_NAME="$name_b" \
  OSL_DISPOSABLE_IDENTITY_NAME="$identity_b" \
  OSL_COPY_PORT="$port_b" \
  OSL_COPY_DATA_DIR="$data_b" \
  XDG_CONFIG_HOME="${data_b}/config" \
  XDG_DATA_HOME="${data_b}/data" \
  APPDATA="${data_b}/roaming" \
  LOCALAPPDATA="${data_b}/local" \
  "${command_args[@]}" >"${data_b}/stdout.log" 2>"${data_b}/stderr.log" &
pid_b=$!

printf '%s\n' "$pid_a" >"${data_a}/pid"
printf '%s\n' "$pid_b" >"${data_b}/pid"
printf 'TASK0029 two-copy-startup copy=A pid=%s\n' "$pid_a"
printf 'TASK0029 two-copy-startup copy=B pid=%s\n' "$pid_b"
printf 'TASK0029 two-copy-startup repo=%q\n' "$repo_root"
