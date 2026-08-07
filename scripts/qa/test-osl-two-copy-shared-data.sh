#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
startup="${OSL_TWO_COPY_STARTUP:-${repo_root}/scripts/qa/osl-two-copy-startup.sh}"

fail() {
  printf 'test-osl-two-copy-shared-data: %s\n' "$1" >&2
  exit 1
}

root="$(mktemp -d)"
token="osl0032-$$"

cleanup() {
  pkill -f "$token" 2>/dev/null || true
  rm -rf "$root"
}
trap cleanup EXIT

# The launched command drops a "${token}-started-<port>" marker in its data
# folder, so the test can count exactly how many copies got to run. exec -a
# keeps the token in the process name so pgrep/pkill still see it afterwards.
copy_command=(bash -c "touch \"\${OSL_COPY_DATA_DIR}/${token}-started-\${OSL_COPY_PORT}\"; exec -a \"${token}-sleep\" sleep 45")

started_count() {
  find "$root" -name "${token}-started-*" 2>/dev/null | wc -l
}

live_count() {
  pgrep -c -f "$token" || true
}

expect_refusal() {
  local case_name="$1" data_a="$2" data_b="$3"
  local out="${root}/${case_name}.out" err="${root}/${case_name}.err" status

  set +e
  "$startup" --root "$root" --data-a "$data_a" --data-b "$data_b" \
    -- "${copy_command[@]}" >"$out" 2>"$err"
  status=$?
  set -e

  printf 'TASK0032 two-copy-shared-data case=%s exit=%s\n' "$case_name" "$status"
  [[ "$status" -eq 1 ]] || fail "${case_name}: expected exit 1 for one shared data folder, got ${status}"
  grep -q 'shared data folder' "$err" || fail "${case_name}: refusal did not name the shared data folder"
  [[ ! -s "$out" ]] || fail "${case_name}: startup printed a launch plan despite the shared data folder"
  [[ "$(started_count)" -eq 0 ]] || fail "${case_name}: a copy started despite the shared data folder"
  [[ "$(live_count)" -eq 0 ]] || fail "${case_name}: a copy process is alive despite the shared data folder"
  printf 'TASK0032 two-copy-shared-data case=%s copies_started=0 refusal=%q\n' \
    "$case_name" "$(head -n1 "$err")"
}

# Both copies handed the very same folder.
expect_refusal same-literal-path "${root}/one-folder" "${root}/one-folder"

# Two different spellings of the same folder must not sneak past the check.
expect_refusal aliased-path "${root}/one-folder" "${root}/detour/../one-folder"

# Control: with two separate folders the very same harness must start both
# copies, so the zero-copies-started result above cannot come from a launcher
# that never launches anything.
set +e
"$startup" --root "$root" --data-a "${root}/apart-a" --data-b "${root}/apart-b" \
  -- "${copy_command[@]}" >"${root}/control.out" 2>"${root}/control.err"
control_exit=$?
set -e
[[ "$control_exit" -eq 0 ]] || fail "control: separate folders should launch cleanly, got exit ${control_exit}"

for _ in $(seq 1 50); do
  [[ "$(started_count)" -eq 2 ]] && break
  sleep 0.2
done
[[ "$(started_count)" -eq 2 ]] || fail "control: expected both copies to start with separate folders"
printf 'TASK0032 two-copy-shared-data case=control-separate-folders exit=%s copies_started=2\n' "$control_exit"
pkill -f "$token" 2>/dev/null || true

printf 'TASK0032 two-copy-shared-data isolation_check_blocks_shared_folder=true\n'
