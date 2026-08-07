#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="${repo_root}/scripts/qa/osl-two-copy-startup.sh"

fail() {
  printf 'test-osl-two-copy-startup: %s\n' "$1" >&2
  exit 1
}

field_value() {
  local key="$1"
  local file="$2"
  sed -n "s/.*${key}=\\([^ ]*\\).*/\\1/p" "$file"
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

good_out="${tmpdir}/good.txt"
shared_data_out="${tmpdir}/shared-data.txt"
shared_port_out="${tmpdir}/shared-port.txt"

"$launcher" --root "${tmpdir}/good-root" >"$good_out"
cat "$good_out"

data_a="$(field_value "data_folder" "$good_out" | sed -n '1p')"
data_b="$(field_value "data_folder" "$good_out" | sed -n '2p')"
port_a="$(field_value "port" "$good_out" | sed -n '1p')"
port_b="$(field_value "port" "$good_out" | sed -n '2p')"

[[ -n "$data_a" && -n "$data_b" ]] || fail "launcher did not print two data folders"
[[ -n "$port_a" && -n "$port_b" ]] || fail "launcher did not print two ports"
[[ "$data_a" != "$data_b" ]] || fail "launcher printed a shared data folder: ${data_a}"
[[ "$port_a" != "$port_b" ]] || fail "launcher printed a shared port: ${port_a}"
grep -Fq 'TASK0029 two-copy-startup different_data_folders=true' "$good_out" ||
  fail "launcher did not report different_data_folders=true"
grep -Fq 'TASK0029 two-copy-startup different_ports=true' "$good_out" ||
  fail "launcher did not report different_ports=true"

printf 'TASK0029 two-copy-startup checked_data_folders=%s,%s\n' "$data_a" "$data_b"
printf 'TASK0029 two-copy-startup checked_ports=%s,%s\n' "$port_a" "$port_b"

set +e
"$launcher" --data-a "${tmpdir}/shared" --data-b "${tmpdir}/shared" >"$shared_data_out" 2>&1
shared_data_rc=$?
set -e
cat "$shared_data_out"
[[ "$shared_data_rc" -eq 1 ]] || fail "shared data folder returned ${shared_data_rc}, expected 1"
grep -Fq 'shared data folder:' "$shared_data_out" ||
  fail "shared data folder failed for the wrong reason"
printf 'TASK0029 two-copy-startup shared_data_folder_exit=%s\n' "$shared_data_rc"

set +e
"$launcher" --port-a 49110 --port-b 49110 --root "${tmpdir}/shared-port-root" >"$shared_port_out" 2>&1
shared_port_rc=$?
set -e
cat "$shared_port_out"
[[ "$shared_port_rc" -eq 1 ]] || fail "shared port returned ${shared_port_rc}, expected 1"
grep -Fq 'shared port: 49110' "$shared_port_out" ||
  fail "shared port failed for the wrong reason"
printf 'TASK0029 two-copy-startup shared_port_exit=%s\n' "$shared_port_rc"
