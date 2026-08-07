#!/usr/bin/env bash
set -Eeuo pipefail

copy=""
data_dir=""
identity_name=""

usage() {
  cat >&2 <<'USAGE'
usage: scripts/qa/osl-two-copy-identity-status.sh --copy <A|B> --data <path> --identity-name <name>

Creates or loads the disposable test identity metadata for exactly one local
copy, then prints a direct status line. The record lives under that copy's data
folder and refuses to load as another identity name.
USAGE
  exit 2
}

need_value() {
  if [[ $# -lt 2 || -z "$2" ]]; then
    printf 'osl-two-copy-identity-status: missing value for %s\n' "$1" >&2
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
      printf 'osl-two-copy-identity-status: unknown argument: %s\n' "$1" >&2
      usage
      ;;
  esac
done

fail_contract() {
  printf 'osl-two-copy-identity-status: %s\n' "$1" >&2
  exit 1
}

[[ "$copy" == "A" || "$copy" == "B" ]] || fail_contract "copy must be A or B"
[[ -n "$data_dir" ]] || fail_contract "data folder is empty"
[[ -n "$identity_name" ]] || fail_contract "disposable identity name is empty"
[[ "$identity_name" =~ ^[A-Za-z0-9._-]+$ ]] ||
  fail_contract "disposable identity name must use only letters, numbers, dot, underscore, or hyphen"

data_dir="$(realpath -m -- "$data_dir")"
identity_dir="${data_dir}/identity"
identity_file="${identity_dir}/disposable-test-identity.v1"
mkdir -p "$identity_dir"

action="loaded"
if [[ ! -e "$identity_file" ]]; then
  action="created"
  {
    printf 'version=1\n'
    printf 'copy=%s\n' "$copy"
    printf 'identity_name=%s\n' "$identity_name"
  } >"$identity_file"
fi

stored_copy="$(sed -n 's/^copy=//p' "$identity_file" | sed -n '1p')"
stored_identity_name="$(sed -n 's/^identity_name=//p' "$identity_file" | sed -n '1p')"

[[ "$stored_copy" == "$copy" ]] ||
  fail_contract "data folder belongs to copy ${stored_copy:-<missing>}, not ${copy}"
[[ "$stored_identity_name" == "$identity_name" ]] ||
  fail_contract "data folder identity is ${stored_identity_name:-<missing>}, not ${identity_name}"

printf 'TASK0030 two-copy-identity status copy=%s action=%s identity_name=%s data_folder=%q\n' \
  "$copy" "$action" "$stored_identity_name" "$data_dir"
