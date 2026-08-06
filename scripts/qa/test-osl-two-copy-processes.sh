#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="${repo_root}/scripts/qa/osl-two-copy-startup.sh"
status_cmd="${repo_root}/scripts/qa/osl-two-copy-process-status.sh"

fail() {
  printf 'test-osl-two-copy-processes: %s\n' "$1" >&2
  exit 1
}

field_value() {
  local key="$1"
  local file="$2"
  sed -n "s/.*${key}=\\([^ ]*\\).*/\\1/p" "$file"
}

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
      sleep 1
    done
    kill -KILL "$pid" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
      local state
      state="$({ ps -p "$pid" -o stat= 2>/dev/null || true; } | sed -n '1p' | tr -d ' ')"
      if [[ -z "$state" || "$state" == Z* ]]; then
        return 0
      fi
      sleep 1
    done
  fi
}

assert_process_numbers_differ() {
  local left="$1"
  local right="$2"
  local pid_a pid_b
  pid_a="$(field_value "pid" "$left" | tail -n 1)"
  pid_b="$(field_value "pid" "$right" | tail -n 1)"
  [[ -n "$pid_a" && -n "$pid_b" ]] || fail "direct status calls did not print both process numbers"
  if [[ "$pid_a" == "$pid_b" ]]; then
    printf 'test-osl-two-copy-processes: matching process numbers: %s\n' "$pid_a" >&2
    return 1
  fi
  printf 'TASK0031 two-copy-process direct_status_pids=%s,%s\n' "$pid_a" "$pid_b"
  printf 'TASK0031 two-copy-process different_process_numbers=true\n'
}

tmpdir="$(mktemp -d)"
pid_a=""
pid_b=""
trap 'stop_pid "$pid_a"; stop_pid "$pid_b"; rm -rf "$tmpdir"' EXIT

duration_seconds="${TASK0031_DURATION_SECONDS:-60}"
interval_seconds="${TASK0031_INTERVAL_SECONDS:-15}"

launch_pair() {
  local plan_file="$1"
  local root="$2"
  "$launcher" \
    --root "$root" \
    --identity-a "osl-copy-a-disposable" \
    --identity-b "osl-copy-b-disposable" \
    -- bash -c 'trap "exit 0" TERM INT; while :; do sleep 1; done' >"$plan_file"
  cat "$plan_file"
}

assert_stopped_copy_fails() {
  local stopped_copy="$1"
  local plan_file="$2"
  local data_a data_b identity_a identity_b case_pid_a case_pid_b status_file stopped_rc
  plan_file="${tmpdir}/${plan_file}"
  launch_pair "$plan_file" "${tmpdir}/break-${stopped_copy}"

  data_a="$(field_value "data_folder" "$plan_file" | sed -n '1p')"
  data_b="$(field_value "data_folder" "$plan_file" | sed -n '2p')"
  identity_a="$(field_value "identity_name" "$plan_file" | sed -n '1p')"
  identity_b="$(field_value "identity_name" "$plan_file" | sed -n '2p')"
  case_pid_a="$(field_value "pid" "$plan_file" | sed -n '1p')"
  case_pid_b="$(field_value "pid" "$plan_file" | sed -n '2p')"

  if [[ "$stopped_copy" == "A" ]]; then
    stop_pid "$case_pid_a"
    case_pid_a=""
    status_file="${tmpdir}/stopped-a.txt"
    set +e
    "$status_cmd" --copy A --data "$data_a" --identity-name "$identity_a" >"$status_file" 2>&1
    stopped_rc=$?
    set -e
  else
    stop_pid "$case_pid_b"
    case_pid_b=""
    status_file="${tmpdir}/stopped-b.txt"
    set +e
    "$status_cmd" --copy B --data "$data_b" --identity-name "$identity_b" >"$status_file" 2>&1
    stopped_rc=$?
    set -e
  fi

  cat "$status_file"
  [[ "$stopped_rc" -eq 1 ]] || fail "stopped copy ${stopped_copy} status returned ${stopped_rc}, expected 1"
  printf 'TASK0031 two-copy-process stopped_copy=%s status_exit=%s\n' "$stopped_copy" "$stopped_rc"

  stop_pid "$case_pid_a"
  stop_pid "$case_pid_b"
}

plan="${tmpdir}/plan.txt"
launch_pair "$plan" "${tmpdir}/good-root"

data_a="$(field_value "data_folder" "$plan" | sed -n '1p')"
data_b="$(field_value "data_folder" "$plan" | sed -n '2p')"
identity_a="$(field_value "identity_name" "$plan" | sed -n '1p')"
identity_b="$(field_value "identity_name" "$plan" | sed -n '2p')"
pid_a="$(field_value "pid" "$plan" | sed -n '1p')"
pid_b="$(field_value "pid" "$plan" | sed -n '2p')"

[[ -n "$data_a" && -n "$data_b" ]] || fail "launcher did not print both data folders"
[[ -n "$identity_a" && -n "$identity_b" ]] || fail "launcher did not print both identity names"
[[ -n "$pid_a" && -n "$pid_b" ]] || fail "launcher did not print both process numbers"

start_epoch="$(date +%s)"
end_epoch=$((start_epoch + duration_seconds))
checks=0
last_a_status=""
last_b_status=""

while :; do
  now="$(date +%s)"
  elapsed=$((now - start_epoch))
  a_status="${tmpdir}/a-status-${checks}.txt"
  b_status="${tmpdir}/b-status-${checks}.txt"
  "$status_cmd" --copy A --data "$data_a" --identity-name "$identity_a" >"$a_status"
  "$status_cmd" --copy B --data "$data_b" --identity-name "$identity_b" >"$b_status"
  cat "$a_status"
  cat "$b_status"
  assert_process_numbers_differ "$a_status" "$b_status"
  checks=$((checks + 1))
  last_a_status="$a_status"
  last_b_status="$b_status"
  printf 'TASK0031 two-copy-process minute_check=%s elapsed_seconds=%s\n' "$checks" "$elapsed"
  (( now >= end_epoch )) && break
  sleep_for="$interval_seconds"
  next=$((now + sleep_for))
  if (( next > end_epoch )); then
    sleep_for=$((end_epoch - now))
  fi
  (( sleep_for > 0 )) && sleep "$sleep_for"
done

assert_process_numbers_differ "$last_a_status" "$last_b_status"
printf 'TASK0031 two-copy-process kept_running_seconds=%s checks=%s pids=%s,%s\n' \
  "$duration_seconds" "$checks" "$pid_a" "$pid_b"

stop_pid "$pid_a"
pid_a=""
stop_pid "$pid_b"
pid_b=""

assert_stopped_copy_fails A "break-a-plan.txt"
assert_stopped_copy_fails B "break-b-plan.txt"
