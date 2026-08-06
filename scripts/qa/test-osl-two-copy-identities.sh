#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="${repo_root}/scripts/qa/osl-two-copy-startup.sh"
status_cmd="${repo_root}/scripts/qa/osl-two-copy-identity-status.sh"

fail() {
  printf 'test-osl-two-copy-identities: %s\n' "$1" >&2
  exit 1
}

field_value() {
  local key="$1"
  local file="$2"
  sed -n "s/.*${key}=\\([^ ]*\\).*/\\1/p" "$file"
}

assert_status_names_differ() {
  local left="$1"
  local right="$2"
  local name_a name_b
  name_a="$(field_value "identity_name" "$left" | sed -n '1p')"
  name_b="$(field_value "identity_name" "$right" | sed -n '1p')"
  [[ -n "$name_a" && -n "$name_b" ]] || fail "direct status calls did not print both identity names"
  if [[ "$name_a" == "$name_b" ]]; then
    printf 'test-osl-two-copy-identities: matching identity names: %s\n' "$name_a" >&2
    return 1
  fi
  printf 'TASK0030 two-copy-identity direct_status_names=%s,%s\n' "$name_a" "$name_b"
  printf 'TASK0030 two-copy-identity different_identity_names=true\n'
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

good_plan="${tmpdir}/good-plan.txt"
a_status="${tmpdir}/a-status.txt"
b_status="${tmpdir}/b-status.txt"
a_reload="${tmpdir}/a-reload.txt"
b_reload="${tmpdir}/b-reload.txt"
same_a_status="${tmpdir}/same-a-status.txt"
same_b_status="${tmpdir}/same-b-status.txt"

"$launcher" \
  --root "${tmpdir}/good-root" \
  --identity-a "osl-copy-a-disposable" \
  --identity-b "osl-copy-b-disposable" >"$good_plan"
cat "$good_plan"

data_a="$(field_value "data_folder" "$good_plan" | sed -n '1p')"
data_b="$(field_value "data_folder" "$good_plan" | sed -n '2p')"
identity_a="$(field_value "identity_name" "$good_plan" | sed -n '1p')"
identity_b="$(field_value "identity_name" "$good_plan" | sed -n '2p')"

[[ -n "$data_a" && -n "$data_b" ]] || fail "launcher did not print both data folders"
[[ -n "$identity_a" && -n "$identity_b" ]] || fail "launcher did not print both identity names"

"$status_cmd" --copy A --data "$data_a" --identity-name "$identity_a" >"$a_status"
"$status_cmd" --copy B --data "$data_b" --identity-name "$identity_b" >"$b_status"
cat "$a_status"
cat "$b_status"
assert_status_names_differ "$a_status" "$b_status"

"$status_cmd" --copy A --data "$data_a" --identity-name "$identity_a" >"$a_reload"
"$status_cmd" --copy B --data "$data_b" --identity-name "$identity_b" >"$b_reload"
cat "$a_reload"
cat "$b_reload"
grep -Fq 'action=loaded' "$a_reload" || fail "copy A did not load its existing disposable identity"
grep -Fq 'action=loaded' "$b_reload" || fail "copy B did not load its existing disposable identity"
printf 'TASK0030 two-copy-identity reload_actions=loaded,loaded\n'

"$status_cmd" --copy A --data "${tmpdir}/same-root/copy-a" --identity-name "osl-shared-disposable" >"$same_a_status"
"$status_cmd" --copy B --data "${tmpdir}/same-root/copy-b" --identity-name "osl-shared-disposable" >"$same_b_status"
cat "$same_a_status"
cat "$same_b_status"

set +e
assert_status_names_differ "$same_a_status" "$same_b_status" >"${tmpdir}/same-check.txt" 2>&1
same_check_rc=$?
set -e
cat "${tmpdir}/same-check.txt"
[[ "$same_check_rc" -eq 1 ]] || fail "matching identity names returned ${same_check_rc}, expected 1"
printf 'TASK0030 two-copy-identity matching_identity_names_exit=%s\n' "$same_check_rc"

set +e
"$status_cmd" --copy B --data "$data_a" --identity-name "$identity_b" >"${tmpdir}/wrong-folder.txt" 2>&1
wrong_folder_rc=$?
set -e
cat "${tmpdir}/wrong-folder.txt"
[[ "$wrong_folder_rc" -eq 1 ]] || fail "cross-copy identity load returned ${wrong_folder_rc}, expected 1"
printf 'TASK0030 two-copy-identity cross_copy_load_exit=%s\n' "$wrong_folder_rc"
