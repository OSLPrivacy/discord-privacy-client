#!/usr/bin/env bash
# Refuse to cut a hub release tag unless the release can pass signing preflight.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="$ROOT/apps/osl-hub/tauri.conf.json"
REQUIRED_SECRETS="HUB_TAURI_SIGNING_PRIVATE_KEY HUB_TAURI_SIGNING_PRIVATE_KEY_PASSWORD"

passes=0
failures=0
warnings=0
summary=""

usage() {
  echo "usage: scripts/release/preflight.sh <hub-vX.Y.Z>"
  echo "       scripts/release/preflight.sh --self-test"
}

add_summary() {
  local status="$1" name="$2" detail="$3"
  summary="${summary}${status} ${name}: ${detail}"$'\n'
}

pass_check() {
  add_summary "PASS" "$1" "$2"
  passes=$((passes + 1))
}

fail_check() {
  add_summary "FAIL" "$1" "$2"
  failures=$((failures + 1))
}

warn_check() {
  add_summary "WARN" "$1" "$2"
  warnings=$((warnings + 1))
}

manifest_version() {
  python3 - "$MANIFEST" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as f:
    print(json.load(f)["version"])
PY
}

check_tag_matches_manifest() {
  local tag="$1" version expected
  if ! version="$(manifest_version 2>&1)"; then
    fail_check "tag matches manifest" "could not parse apps/osl-hub/tauri.conf.json version: $version"
    return
  fi
  expected="hub-v${version}"
  if [ "$tag" = "$expected" ]; then
    pass_check "tag matches manifest" "$tag matches manifest version $version"
  else
    fail_check "tag matches manifest" "argument '$tag' must be '$expected'"
  fi
}

check_tag_absent() {
  local tag="$1"
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null 2>&1; then
    fail_check "tag does not already exist" "refs/tags/$tag already exists"
  else
    pass_check "tag does not already exist" "refs/tags/$tag is absent"
  fi
}

check_ci_green() {
  local head_sha runs verdict
  if ! head_sha="$(git rev-parse HEAD 2>&1)"; then
    fail_check "CI is green on HEAD" "could not read HEAD sha: $head_sha"
    return
  fi
  if ! runs="$(gh run list --limit 100 --json headSha,workflowName,conclusion 2>&1)"; then
    fail_check "CI is green on HEAD" "could not list GitHub runs: $runs"
    return
  fi
  if ! verdict="$(python3 -c '
import json
import sys

head = sys.argv[1]
wanted = ["Rust Test", "TypeScript Test"]
runs = json.load(sys.stdin)
bad = []

for workflow in wanted:
    match = None
    for run in runs:
        if run.get("headSha") == head and run.get("workflowName") == workflow:
            match = run
            break
    if match is None:
        bad.append(f"{workflow}: no run for HEAD {head}")
    else:
        conclusion = match.get("conclusion")
        if conclusion != "success":
            bad.append(f"{workflow}: conclusion {conclusion!r}")

if bad:
    print("; ".join(bad))
    sys.exit(1)
print(f"Rust Test and TypeScript Test succeeded for HEAD {head}")
' "$head_sha" <<<"$runs")"; then
    fail_check "CI is green on HEAD" "$verdict"
  else
    pass_check "CI is green on HEAD" "$verdict"
  fi
}

check_signing_secrets() {
  local secrets missing
  if ! secrets="$(gh secret list --env hub-release 2>&1)"; then
    fail_check "signing secrets present" "could not list hub-release secret names"
    return
  fi
  missing="$(python3 -c '
import sys

required = sys.argv[1].split()
names = set()
for line in sys.stdin:
    fields = line.strip().split()
    if fields:
        names.add(fields[0])
print(" ".join(name for name in required if name not in names))
' "$REQUIRED_SECRETS" <<<"$secrets")"
  if [ -n "$missing" ]; then
    fail_check "signing secrets present" "missing secret name(s): $missing"
  else
    pass_check "signing secrets present" "required hub-release secret names are listed"
  fi
}

check_gate_proof() {
  local script="$1" label="$2" out
  if out="$(bash "$ROOT/scripts/release/$script" 2>&1)"; then
    pass_check "$label" "$script exited 0"
  else
    fail_check "$label" "$script failed: $out"
  fi
}

check_release_cold_snapshots() {
  local snapshots count
  if ! snapshots="$(az snapshot list -o json 2>&1)"; then
    warn_check "release-cold snapshots" "could not list Azure snapshots; two-clean-VM attestation cannot be confirmed"
    return
  fi
  if ! count="$(python3 -c '
import json
import sys

snapshots = json.load(sys.stdin)
count = 0
for snapshot in snapshots:
    tags = snapshot.get("tags") or {}
    if tags.get("lineage") == "release-cold":
        count += 1
print(count)
' <<<"$snapshots")"; then
    warn_check "release-cold snapshots" "could not parse Azure snapshots; two-clean-VM attestation cannot be confirmed"
    return
  fi
  if [ "$count" -lt 2 ]; then
    warn_check "release-cold snapshots" "only $count lineage=release-cold snapshot(s); two-clean-VM attestation cannot be completed truthfully"
  else
    pass_check "release-cold snapshots" "$count lineage=release-cold snapshots exist"
  fi
}

check_hub_vm_qa_environment() {
  local environments found
  if ! environments="$(gh api repos/:owner/:repo/environments 2>&1)"; then
    warn_check "hub-vm-qa environment" "could not list GitHub environments; promotion approval cannot be confirmed"
    return
  fi
  if ! found="$(python3 -c '
import json
import sys

data = json.load(sys.stdin)
for environment in data.get("environments", []):
    if environment.get("name") == "hub-vm-qa":
        print("yes")
        break
' <<<"$environments")"; then
    warn_check "hub-vm-qa environment" "could not parse GitHub environments; promotion approval cannot be confirmed"
    return
  fi
  if [ "$found" = "yes" ]; then
    pass_check "hub-vm-qa environment" "hub-vm-qa exists"
  else
    warn_check "hub-vm-qa environment" "hub-vm-qa is missing; promotion would run with NO human approval"
  fi
}

run_preflight() {
  local tag="$1"
  passes=0
  failures=0
  warnings=0
  summary=""

  check_tag_matches_manifest "$tag"
  check_tag_absent "$tag"
  check_ci_green
  check_signing_secrets
  check_gate_proof "prove-promotion-gate.sh" "promotion gate proof"
  check_gate_proof "prove-rollback-guards.sh" "rollback guard proof"
  check_release_cold_snapshots
  check_hub_vm_qa_environment

  echo "Preflight summary:"
  printf '%s' "$summary"
  echo "Totals: $passes PASS, $failures FAIL, $warnings WARN"
  if [ "$failures" -ne 0 ]; then
    echo "PREFLIGHT FAILED - do not cut $tag"
    return 1
  fi
  echo "PREFLIGHT PASSED - safe to cut $tag"
  return 0
}

write_stub() {
  local path="$1" body="$2"
  {
    echo '#!/usr/bin/env bash'
    echo 'set -uo pipefail'
    printf '%s\n' "$body"
  } > "$path"
  chmod +x "$path"
}

self_test_case() {
  local work="$1" name="$2" expected_status="$3" expected_text="$4"
  shift 4
  local out status
  out="$(
    export PATH="$work/bin:$PATH"
    export PREFLIGHT_GIT_SHA="${PREFLIGHT_GIT_SHA:-abc123}"
    export PREFLIGHT_TAG_EXISTS="${PREFLIGHT_TAG_EXISTS:-0}"
    export PREFLIGHT_RUST_CONCLUSION="${PREFLIGHT_RUST_CONCLUSION:-success}"
    export PREFLIGHT_TS_CONCLUSION="${PREFLIGHT_TS_CONCLUSION:-success}"
    export PREFLIGHT_CI_MODE="${PREFLIGHT_CI_MODE:-normal}"
    export PREFLIGHT_SECRETS="${PREFLIGHT_SECRETS:-both}"
    export PREFLIGHT_SNAPSHOTS="${PREFLIGHT_SNAPSHOTS:-2}"
    export PREFLIGHT_ENV_PRESENT="${PREFLIGHT_ENV_PRESENT:-1}"
    "$@" 2>&1
  )"
  status=$?
  if [ "$expected_status" = "pass" ] && [ "$status" -ne 0 ]; then
    echo "  FAIL  $name -> expected exit 0, got $status"
    echo "$out"
    return 1
  fi
  if [ "$expected_status" = "fail" ] && [ "$status" -eq 0 ]; then
    echo "  FAIL  $name -> expected non-zero exit"
    echo "$out"
    return 1
  fi
  if [ -n "$expected_text" ] && ! printf '%s' "$out" | grep -qF "$expected_text"; then
    echo "  FAIL  $name -> expected output containing '$expected_text'"
    echo "$out"
    return 1
  fi
  echo "  ok    $name"
  return 0
}

self_test() {
  local work test_root script rc passed failed
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  test_root="$work/repo"
  script="$test_root/scripts/release/preflight.sh"
  mkdir -p "$work/bin" "$test_root/apps/osl-hub" "$test_root/scripts/release"
  cp "$0" "$script"
  printf '{"version":"0.1.0"}\n' > "$test_root/apps/osl-hub/tauri.conf.json"

  # shellcheck disable=SC2016
  write_stub "$test_root/scripts/release/prove-promotion-gate.sh" 'exit "${PREFLIGHT_PROMOTION_PROOF_STATUS:-0}"'
  # shellcheck disable=SC2016
  write_stub "$test_root/scripts/release/prove-rollback-guards.sh" 'exit "${PREFLIGHT_ROLLBACK_PROOF_STATUS:-0}"'
  # shellcheck disable=SC2016
  write_stub "$work/bin/git" '
case "$*" in
  "rev-parse HEAD") printf "%s\n" "$PREFLIGHT_GIT_SHA" ;;
  rev-parse\ -q\ --verify\ refs/tags/*)
    if [ "$PREFLIGHT_TAG_EXISTS" = 1 ]; then
      printf "%s\n" "${4#refs/tags/}"
      exit 0
    fi
    exit 1
    ;;
  *) echo "unexpected git command: $*" >&2; exit 2 ;;
esac'
  # shellcheck disable=SC2016
  write_stub "$work/bin/gh" '
if [ "$1 $2 $3" = "run list --limit" ]; then
  if [ "${PREFLIGHT_CI_MODE:-normal}" = "none" ]; then
    printf "[]\n"
  else
    python3 - "$PREFLIGHT_GIT_SHA" "$PREFLIGHT_RUST_CONCLUSION" "$PREFLIGHT_TS_CONCLUSION" <<PY
import json
import sys
sha, rust, ts = sys.argv[1:4]
print(json.dumps([
    {"headSha": sha, "workflowName": "Rust Test", "conclusion": rust},
    {"headSha": sha, "workflowName": "TypeScript Test", "conclusion": ts},
    {"headSha": "older", "workflowName": "Rust Test", "conclusion": "success"},
]))
PY
  fi
elif [ "$1 $2 $3" = "secret list --env" ] && [ "${4:-}" = "hub-release" ]; then
  case "${PREFLIGHT_SECRETS:-both}" in
    both)
      printf "HUB_TAURI_SIGNING_PRIVATE_KEY\tupdated\n"
      printf "HUB_TAURI_SIGNING_PRIVATE_KEY_PASSWORD\tupdated\n"
      ;;
    missing_password) printf "HUB_TAURI_SIGNING_PRIVATE_KEY\tupdated\n" ;;
    *) exit 2 ;;
  esac
elif [ "$1 $2" = "api repos/:owner/:repo/environments" ]; then
  if [ "${PREFLIGHT_ENV_PRESENT:-1}" = 1 ]; then
    printf "{\"environments\":[{\"name\":\"hub-vm-qa\"}]}\n"
  else
    printf "{\"environments\":[]}\n"
  fi
else
  echo "unexpected gh command: $*" >&2
  exit 2
fi'
  # shellcheck disable=SC2016
  write_stub "$work/bin/az" '
if [ "$1 $2 $3" = "snapshot list -o" ] && [ "${4:-}" = "json" ]; then
  python3 - "${PREFLIGHT_SNAPSHOTS:-2}" <<PY
import json
import sys
count = int(sys.argv[1])
print(json.dumps([{"tags": {"lineage": "release-cold"}} for _ in range(count)]))
PY
else
  echo "unexpected az command: $*" >&2
  exit 2
fi'

  passed=0
  failed=0
  echo "Self-test:"

  if self_test_case "$work" "everything green" pass "PREFLIGHT PASSED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if self_test_case "$work" "manifest mismatch" fail "PREFLIGHT FAILED" bash "$script" hub-v0.2.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_TAG_EXISTS=1 self_test_case "$work" "tag already exists" fail "PREFLIGHT FAILED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_RUST_CONCLUSION=failure self_test_case "$work" "Rust Test failure" fail "PREFLIGHT FAILED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_TS_CONCLUSION=failure self_test_case "$work" "TypeScript Test failure" fail "PREFLIGHT FAILED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_CI_MODE=none self_test_case "$work" "no CI run for HEAD" fail "PREFLIGHT FAILED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_SECRETS=missing_password self_test_case "$work" "missing signing secret" fail "PREFLIGHT FAILED" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_SNAPSHOTS=0 self_test_case "$work" "zero release-cold snapshots warns" pass "WARN" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi
  if PREFLIGHT_ENV_PRESENT=0 self_test_case "$work" "missing hub-vm-qa warns" pass "WARN" bash "$script" hub-v0.1.0; then passed=$((passed + 1)); else failed=$((failed + 1)); fi

  echo "$passed passed, $failed failed"
  rc=0
  if [ "$failed" -ne 0 ]; then
    rc=1
  fi
  rm -rf "$work"
  trap - RETURN
  return "$rc"
}

main() {
  if [ "$#" -ne 1 ]; then
    usage >&2
    exit 2
  fi
  case "$1" in
    --self-test) self_test ;;
    hub-v*) run_preflight "$1" ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
