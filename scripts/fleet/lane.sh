#!/usr/bin/env bash
# Run one Cargo command in a lane-private target directory.  Do not replace
# `cargo` below with `osl-cargo`: that wrapper deliberately shares a target
# directory and serialises the fleet.
set -euo pipefail

readonly JOB_CAP=4
readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPOSITORY_ROOT="$(git -C "$SCRIPT_DIR/../.." rev-parse --show-toplevel)"

usage() {
  echo "Usage: $0 <lane-name> <cargo arguments...>" >&2
}

validate_lane_name() {
  local lane_name="$1"
  [[ "$lane_name" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$lane_name" != *..* ]] || {
    echo "lane name must be a single safe path component: $lane_name" >&2
    return 64
  }
}

reject_caller_job_override() {
  local after_separator=false argument
  for argument in "$@"; do
    "$after_separator" && continue
    [[ "$argument" == "--" ]] && {
      after_separator=true
      continue
    }
    case "$argument" in
      -j|--jobs|-j[0-9]*|--jobs=*)
        echo "lane launcher fixes Cargo parallelism at -j $JOB_CAP; remove $argument" >&2
        return 64
        ;;
    esac
  done
}

run_lane() {
  local lane_name="$1"
  shift
  validate_lane_name "$lane_name"
  (($# > 0)) || {
    usage
    return 64
  }
  reject_caller_job_override "$@"

  local target_root target_dir
  target_root="${OSL_FLEET_TARGET_ROOT:-$REPOSITORY_ROOT/target/fleet}"
  target_dir="$target_root/$lane_name"
  mkdir -p -- "$target_dir"

  exec env CARGO_TARGET_DIR="$target_dir" cargo -j "$JOB_CAP" "$@"
}

self_test() {
  local test_dir fake_cargo log_file target_root lane target_dir
  local -a pids=()
  local attempts active_jobs max_jobs
  test_dir="$(mktemp -d)"
  trap 'rm -rf "$test_dir"' RETURN
  fake_cargo="$test_dir/cargo"
  log_file="$test_dir/cargo.log"
  target_root="$test_dir/targets"

  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'jobs=11' \
    'while (($#)); do' \
    '  if [[ "$1" == "-j" ]]; then jobs="$2"; shift 2; continue; fi' \
    '  shift' \
    'done' \
    'for ((worker = 0; worker < jobs; worker++)); do' \
    '  bash -c '\''exec -a cargo-job sleep 0.4'\'' &' \
    'done' \
    'printf "%s\\t%s\\n" "${CARGO_TARGET_DIR:?}" "$*" >>"${LANE_TEST_LOG:?}"' \
    'wait' >"$fake_cargo"
  chmod +x "$fake_cargo"

  for lane in alpha bravo charlie delta; do
    PATH="$test_dir:$PATH" LANE_TEST_LOG="$log_file" OSL_FLEET_TARGET_ROOT="$target_root" \
      "$0" "$lane" check &
    pids+=("$!")
  done

  for ((attempts = 0; attempts < 100; attempts++)); do
    [[ -f "$log_file" && $(wc -l <"$log_file") -eq 4 ]] && break
    sleep 0.01
  done
  [[ -f "$log_file" && $(wc -l <"$log_file") -eq 4 ]] || {
    echo "self-test did not start four Cargo lanes" >&2
    return 1
  }

  max_jobs=0
  for ((attempts = 0; attempts < 5; attempts++)); do
    active_jobs=0
    local process
    for process in /proc/[0-9]*; do
      grep -azq '^cargo-job$' "$process/cmdline" 2>/dev/null && ((active_jobs++)) || true
    done
    ((active_jobs > max_jobs)) && max_jobs="$active_jobs"
    sleep 0.02
  done
  ((max_jobs <= 16)) || {
    echo "self-test observed $max_jobs Cargo jobs; fleet cap is 16" >&2
    return 1
  }

  for lane in alpha bravo charlie delta; do
    target_dir="$target_root/$lane"
    [[ -d "$target_dir" ]] || {
      echo "self-test missing target directory for $lane" >&2
      return 1
    }
  done
  [[ $(cut -f1 "$log_file" | sort -u | wc -l) -eq 4 ]] || {
    echo "self-test observed shared target directories" >&2
    return 1
  }

  local pid
  for pid in "${pids[@]}"; do
    wait "$pid"
  done
}

case "${1:-}" in
  --self-test) self_test ;;
  *) run_lane "$@" ;;
esac
