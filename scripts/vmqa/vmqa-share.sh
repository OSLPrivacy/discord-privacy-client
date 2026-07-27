#!/usr/bin/env bash
# Azure Blob rendezvous helper for VM QA runs.
set -euo pipefail

DEFAULT_RG="osl-two-client-lab"
DEFAULT_SA="osltestartifactsa7d5"
DEFAULT_CONTAINER="vmqa"
POLL_SECONDS=3

RG="${VMQA_RG:-$DEFAULT_RG}"
SA="${VMQA_SA:-$DEFAULT_SA}"
CONTAINER="${VMQA_CONTAINER:-$DEFAULT_CONTAINER}"
: "$RG"

# Keeping usage centralized makes automation failures predictable.
usage() {
  cat >&2 <<'USAGE'
vmqa-share.sh - Azure Blob rendezvous helper
  ensure-share
  init-run <vm> <runid>
  put <local-path> <blob-name>
  get <blob-name> <local-path>
  exists <blob-name>
  ls <blob-prefix>
  wait-for <blob-name> <timeout-seconds>
  put-build <local-exe> <local-loader>
  put-atomic <local-path> <blob-name>
  get-atomic <blob-name> <local-path>
USAGE
}

az_storage() {
  az storage "$@" \
    --account-name "$SA" \
    --auth-mode login \
    --only-show-errors
}

# Refusing ambiguous blob names keeps callers inside the rendezvous namespace by construction.
normalize_rel_path() {
  local input="${1:-}" part out=""
  local -a parts
  case "$input" in
    ""|"."|"/") printf '%s\n' ""; return 0 ;;
    /*) echo "blob name must be relative: $input" >&2; return 64 ;;
  esac
  IFS='/' read -r -a parts <<<"$input"
  for part in "${parts[@]}"; do
    case "$part" in
      ""|".") continue ;;
      "..") echo "blob name must not contain '..': $input" >&2; return 64 ;;
    esac
    if [ -z "$out" ]; then out="$part"; else out="$out/$part"; fi
  done
  printf '%s\n' "$out"
}

require_blob_name() {
  local remote
  remote="$(normalize_rel_path "$1")"
  [ -n "$remote" ] || { echo "blob name must not be empty" >&2; return 64; }
  printf '%s\n' "$remote"
}

validate_run_segment() {
  local label="$1" value="$2"
  [ -n "$value" ] || { echo "$label must not be empty" >&2; return 64; }
  case "$value" in
    "."|".."|*/*) echo "$label contains invalid path syntax: $value" >&2; return 64 ;;
  esac
}

# Conflating "absent" with "the query failed" is how a harness reports green while measuring nothing.
storage_blob_exists() {
  local remote exists err
  remote="$(require_blob_name "$1")"
  err="$(mktemp)"
  if exists="$(az_storage blob exists --container-name "$CONTAINER" --name "$remote" --query exists -o tsv 2>"$err")"; then
    :
  else
    cat "$err" >&2
    rm -f -- "$err"
    return 1
  fi
  rm -f -- "$err"
  case "$exists" in
    true) return 0 ;;
    false) return 3 ;;
    *) echo "unexpected blob exists result for $remote: $exists" >&2; return 1 ;;
  esac
}

put_file() {
  local local_path="$1" remote
  remote="$(require_blob_name "$2")"
  [ -f "$local_path" ] || { echo "local file not found: $local_path" >&2; return 66; }
  az_storage blob upload \
    --container-name "$CONTAINER" \
    --name "$remote" \
    --file "$local_path" \
    --overwrite true \
    --no-progress \
    -o none
}

get_file() {
  local remote="$1" local_path="$2"
  remote="$(require_blob_name "$remote")"
  storage_blob_exists "$remote" || return $?
  mkdir -p -- "$(dirname -- "$local_path")"
  az_storage blob download \
    --container-name "$CONTAINER" \
    --name "$remote" \
    --file "$local_path" \
    --no-progress \
    -o none
}

cmd_ensure_share() {
  # `az storage container create` is already idempotent: it returns created=false when the
  # container is there and exits 0. --fail-on-exist is a store-true switch, so passing it a value
  # is a CLI usage error (exit 2) rather than the "do not fail" it looks like.
  az_storage container create --name "$CONTAINER" -o none
  printf 'container=ok\n'
}

cmd_init_run() {
  local vm="${1:-}" runid="${2:-}" prefix
  [ $# -eq 2 ] || { echo "init-run needs <vm> <runid>" >&2; return 64; }
  validate_run_segment "vm" "$vm"
  validate_run_segment "runid" "$runid"
  prefix="runs/$vm/$runid"
  # Creating directories is a Files concept; zero-byte placeholder blobs would leave junk the agent might read.
  printf '%s\n' "$prefix"
}

cmd_ls() {
  local prefix
  [ $# -eq 1 ] || { echo "ls needs <blob-prefix>" >&2; return 64; }
  prefix="$(normalize_rel_path "$1")"
  az_storage blob list \
    --container-name "$CONTAINER" \
    --prefix "$prefix" \
    --query '[].name' \
    -o tsv
}

cmd_wait_for() {
  local remote="${1:-}" timeout="${2:-}" start elapsed exists_rc
  [ $# -eq 2 ] || { echo "wait-for needs <blob-name> <timeout-seconds>" >&2; return 64; }
  [[ "$timeout" =~ ^[0-9]+$ ]] || { echo "timeout must be an integer number of seconds" >&2; return 64; }
  remote="$(require_blob_name "$remote")"
  start="$(date +%s)"
  while :; do
    if storage_blob_exists "$remote"; then
      elapsed=$(( $(date +%s) - start ))
      printf '%s\n' "$elapsed"
      return 0
    else
      exists_rc=$?
      [ "$exists_rc" -eq 3 ] || return "$exists_rc"
    fi
    elapsed=$(( $(date +%s) - start ))
    if [ "$elapsed" -ge "$timeout" ]; then
      printf '\nTIMEOUT: %s absent after %ss\n' "$remote" "$elapsed" >&2
      return 4
    fi
    printf '.' >&2
    sleep "$POLL_SECONDS"
  done
}

cmd_put_build() {
  local exe="${1:-}" loader="${2:-}" hash exe_remote loader_remote exists_rc
  [ $# -eq 2 ] || { echo "put-build needs <local-exe> <local-loader>" >&2; return 64; }
  [ -f "$exe" ] || { echo "local exe not found: $exe" >&2; return 66; }
  [ -f "$loader" ] || { echo "local loader not found: $loader" >&2; return 66; }
  hash="$(sha256sum "$exe" | awk '{print $1}')"
  exe_remote="builds/$hash/osl-privacy-hub.exe"
  loader_remote="builds/$hash/WebView2Loader.dll"
  if storage_blob_exists "$exe_remote"; then
    :
  else
    exists_rc=$?
    [ "$exists_rc" -eq 3 ] || return "$exists_rc"
    put_file "$exe" "$exe_remote" >/dev/null
    put_file "$loader" "$loader_remote" >/dev/null
  fi
  printf '%s\n' "$hash"
}

cmd_put_atomic() {
  local local_path="${1:-}" remote="${2:-}" hash ready_tmp
  [ $# -eq 2 ] || { echo "put-atomic needs <local-path> <blob-name>" >&2; return 64; }
  [ -f "$local_path" ] || { echo "local file not found: $local_path" >&2; return 66; }
  remote="$(require_blob_name "$remote")"
  hash="$(sha256sum "$local_path" | awk '{print $1}')"
  ready_tmp="$(mktemp)"
  printf '%s' "$hash" >"$ready_tmp"
  if ! put_file "$local_path" "$remote"; then rm -f -- "$ready_tmp"; return 1; fi
  if ! put_file "$ready_tmp" "$remote.ready"; then rm -f -- "$ready_tmp"; return 1; fi
  rm -f -- "$ready_tmp"
}

cmd_get_atomic() {
  local remote="${1:-}" local_path="${2:-}" local_parent local_base payload_tmp ready_tmp expected actual
  [ $# -eq 2 ] || { echo "get-atomic needs <blob-name> <local-path>" >&2; return 64; }
  remote="$(require_blob_name "$remote")"
  storage_blob_exists "$remote.ready" || return $?
  local_parent="$(dirname -- "$local_path")"
  local_base="$(basename -- "$local_path")"
  mkdir -p -- "$local_parent"
  payload_tmp="$(mktemp --tmpdir="$local_parent" ".$local_base.payload.XXXXXX")"
  ready_tmp="$(mktemp --tmpdir="$local_parent" ".$local_base.ready.XXXXXX")"
  if ! get_file "$remote.ready" "$ready_tmp"; then rm -f -- "$payload_tmp" "$ready_tmp"; return 1; fi
  if ! get_file "$remote" "$payload_tmp"; then rm -f -- "$payload_tmp" "$ready_tmp"; return 1; fi
  expected="$(tr -d '\r\n' <"$ready_tmp")"
  actual="$(sha256sum "$payload_tmp" | awk '{print $1}')"
  if [ "$expected" != "$actual" ]; then
    echo "sha256 mismatch for $remote: ready=$expected actual=$actual" >&2
    rm -f -- "$payload_tmp" "$ready_tmp" "$local_path"
    return 5
  fi
  mv -f -- "$payload_tmp" "$local_path"
  rm -f -- "$ready_tmp"
}

main() {
  local cmd="${1:-}"
  [ -n "$cmd" ] || { usage; return 64; }
  shift
  case "$cmd" in
    ensure-share) [ $# -eq 0 ] || { echo "ensure-share takes no arguments" >&2; return 64; }; cmd_ensure_share "$@" ;;
    init-run) cmd_init_run "$@" ;;
    put) [ $# -eq 2 ] || { echo "put needs <local-path> <blob-name>" >&2; return 64; }; put_file "$@" ;;
    get) [ $# -eq 2 ] || { echo "get needs <blob-name> <local-path>" >&2; return 64; }; get_file "$@" ;;
    exists) [ $# -eq 1 ] || { echo "exists needs <blob-name>" >&2; return 64; }; storage_blob_exists "$1" ;;
    ls) cmd_ls "$@" ;;
    wait-for) cmd_wait_for "$@" ;;
    put-build) cmd_put_build "$@" ;;
    put-atomic) cmd_put_atomic "$@" ;;
    get-atomic) cmd_get_atomic "$@" ;;
    *) usage; return 64 ;;
  esac
}

[ "${BASH_SOURCE[0]}" != "$0" ] || main "$@"
