#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
starter="${OSL_FAST_TWO_PERSON_STARTER:-${repo_root}/scripts/qa/osl-fast-two-person-test.sh}"

fail() {
  printf 'test-osl-fast-two-person-starter: %s\n' "$1" >&2
  exit 1
}

json_value() {
  local file="$1"
  local expr="$2"
  python3 - "$file" "$expr" <<'PY'
import json
import sys

path, expr = sys.argv[1:3]
record = json.loads(open(path, encoding="utf-8").read())
value = record
for key in expr.split("."):
    if not key:
        continue
    value = value[key]
if isinstance(value, list):
    print("\n".join(str(item) for item in value))
else:
    print(value)
PY
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

good_out="${tmpdir}/good-output"
good_root="${tmpdir}/good-root"
good_log="${tmpdir}/good.log"

"$starter" --output "$good_out" --root "$good_root" >"$good_log"
cat "$good_log"

status_count="$(find "$good_out" -maxdepth 1 -name 'status-*.txt' | wc -l | tr -d ' ')"
metadata_count="$(find "$good_out" -maxdepth 1 -name 'metadata-*.json' | wc -l | tr -d ' ')"
[[ "$status_count" == "2" ]] || fail "expected two saved status results, got ${status_count}"
[[ "$metadata_count" == "2" ]] || fail "expected two saved metadata records, got ${metadata_count}"

grep -Fq 'TASK0031 two-copy-process status copy=A' "$good_out/status-A.txt" ||
  fail "copy A status result is missing process status"
grep -Fq 'TASK0031 two-copy-process status copy=B' "$good_out/status-B.txt" ||
  fail "copy B status result is missing process status"
grep -Fq 'identity_name=osl-copy-a-disposable' "$good_out/status-A.txt" ||
  fail "copy A status result is missing identity"
grep -Fq 'identity_name=osl-copy-b-disposable' "$good_out/status-B.txt" ||
  fail "copy B status result is missing identity"

switches_a="$(json_value "$good_out/metadata-A.json" switches)"
switches_b="$(json_value "$good_out/metadata-B.json" switches)"
printf '%s\n' "$switches_a" | grep -Fxq 'OSL_DISPOSABLE_IDENTITY_NAME=osl-copy-a-disposable' ||
  fail "copy A metadata is missing its identity switch"
printf '%s\n' "$switches_b" | grep -Fxq 'OSL_DISPOSABLE_IDENTITY_NAME=osl-copy-b-disposable' ||
  fail "copy B metadata is missing its identity switch"
printf '%s\n' "$switches_a" | grep -Fxq 'OSL_PASSWORD_SCREEN=on' ||
  fail "copy A metadata is missing OSL_PASSWORD_SCREEN=on"
printf '%s\n' "$switches_b" | grep -Fxq 'OSL_PASSWORD_SCREEN=off' ||
  fail "copy B metadata is missing OSL_PASSWORD_SCREEN=off"

grep -Fq 'TASK0064 two-person-starter identity copy=A identity_name=osl-copy-a-disposable' "$good_log" ||
  fail "starter did not print copy A identity"
grep -Fq 'TASK0064 two-person-starter identity copy=B identity_name=osl-copy-b-disposable' "$good_log" ||
  fail "starter did not print copy B identity"
grep -Fq 'OSL_PASSWORD_SCREEN=on' "$good_log" ||
  fail "starter did not print copy A switches"
grep -Fq 'OSL_PASSWORD_SCREEN=off' "$good_log" ||
  fail "starter did not print copy B switches"
grep -Fq 'TASK0064 two-person-starter cleanup pid_a_absent=true pid_b_absent=true root_exists=false leftover_copies=0' "$good_log" ||
  fail "starter did not prove cleanup"
[[ ! -e "$good_root" ]] || fail "copy root still exists after starter cleanup"

identity_a="$(sed -n 's/.*identity_name=\([^ ]*\).*/\1/p' "$good_out/status-A.txt" | sed -n '1p')"
identity_b="$(sed -n 's/.*identity_name=\([^ ]*\).*/\1/p' "$good_out/status-B.txt" | sed -n '1p')"
printf 'TASK0064 two-person-starter saved_status_count=%s\n' "$status_count"
printf 'TASK0064 two-person-starter saved_metadata_count=%s\n' "$metadata_count"
printf 'TASK0064 two-person-starter identities=%s,%s\n' "$identity_a" "$identity_b"
printf 'TASK0064 two-person-starter switch_values=OSL_PASSWORD_SCREEN=on,OSL_PASSWORD_SCREEN=off\n'
printf 'TASK0064 two-person-starter leftover_copies=0\n'

shared_out="${tmpdir}/shared-output"
shared_root="${tmpdir}/shared-root"
shared_log="${tmpdir}/shared.log"
set +e
"$starter" \
  --output "$shared_out" \
  --root "$shared_root" \
  --identity-a "osl-shared-disposable" \
  --identity-b "osl-shared-disposable" >"$shared_log" 2>&1
shared_rc=$?
set -e
cat "$shared_log"
[[ "$shared_rc" == "1" ]] || fail "shared identity returned ${shared_rc}, expected 1"
grep -Fq 'shared disposable identity name: osl-shared-disposable' "$shared_log" ||
  fail "shared identity failure did not name the shared identity"
shared_status_count="$(find "$shared_out" -maxdepth 1 -name 'status-*.txt' 2>/dev/null | wc -l | tr -d ' ')"
shared_metadata_count="$(find "$shared_out" -maxdepth 1 -name 'metadata-*.json' 2>/dev/null | wc -l | tr -d ' ')"
[[ "$shared_status_count" == "0" ]] || fail "shared identity saved status results"
[[ "$shared_metadata_count" == "0" ]] || fail "shared identity saved metadata"
[[ ! -e "$shared_root" ]] || fail "shared identity left a copy root"
printf 'TASK0064 two-person-starter shared_identity_exit=%s\n' "$shared_rc"
printf 'TASK0064 two-person-starter shared_identity_status_count=0 shared_identity_metadata_count=0\n'

same_port_out="${tmpdir}/same-port-output"
same_port_root="${tmpdir}/same-port-root"
same_port_log="${tmpdir}/same-port.log"
set +e
"$starter" \
  --output "$same_port_out" \
  --root "$same_port_root" \
  --port-a 48267 \
  --port-b 48267 >"$same_port_log" 2>&1
same_port_rc=$?
set -e
cat "$same_port_log"
[[ "$same_port_rc" == "1" ]] || fail "same port returned ${same_port_rc}, expected 1"
grep -Fq 'shared port: 48267' "$same_port_log" ||
  fail "same port failure did not name the shared port"
same_port_status_count="$(find "$same_port_out" -maxdepth 1 -name 'status-*.txt' 2>/dev/null | wc -l | tr -d ' ')"
same_port_metadata_count="$(find "$same_port_out" -maxdepth 1 -name 'metadata-*.json' 2>/dev/null | wc -l | tr -d ' ')"
same_port_plan_bytes=0
if [[ -f "$same_port_out/launch-plan.txt" ]]; then
  same_port_plan_bytes="$(wc -c <"$same_port_out/launch-plan.txt" | tr -d ' ')"
fi
same_port_pid_count=0
if [[ -e "$same_port_root" ]]; then
  same_port_pid_count="$(find "$same_port_root" -name pid 2>/dev/null | wc -l | tr -d ' ')"
fi
[[ "$same_port_status_count" == "0" ]] || fail "same port saved status results"
[[ "$same_port_metadata_count" == "0" ]] || fail "same port saved metadata"
[[ "$same_port_plan_bytes" == "0" ]] || fail "same port printed a launch plan"
[[ "$same_port_pid_count" == "0" ]] || fail "same port started a copy process"
[[ ! -e "$same_port_root" ]] || fail "same port left a copy root"
printf 'TASK0067 two-person-starter same_port_exit=%s\n' "$same_port_rc"
printf 'TASK0067 two-person-starter refusal=%q\n' "$(head -n1 "$same_port_log")"
printf 'TASK0067 two-person-starter status_count=0 metadata_count=0 launch_plan_bytes=0 pid_count=0 root_exists=false\n'
