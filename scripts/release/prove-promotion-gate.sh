#!/usr/bin/env bash
# Prove the two-clean-VM promotion gate can REJECT, not just accept.
#
#   scripts/release/prove-promotion-gate.sh
#
# A gate that has only ever been run against input designed to pass is
# decoration: you cannot tell an enforcing verifier from a `return True`.
# This builds a synthetic candidate that passes, then mutates it one field
# at a time and requires that every mutation is refused for the stated
# reason. It touches no network, no release, and no real installer, so it
# runs on every PR.
#
# Each negative case below is a way a candidate could be promoted without
# having actually been tested on two clean VMs.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
verifier="$here/scripts/verify_hub_vm_qa_attestation.py"
tag="hub-v0.1.0"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

candidate="$work/candidate"
mkdir -p "$candidate"
printf 'not a real installer, fixture bytes only\n' > "$candidate/osl-hub-0.1.0-x64-nsis.exe"
printf '{"version":"0.1.0"}\n' > "$candidate/latest.json"
installer_sha="$(python3 -c '
import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$candidate/osl-hub-0.1.0-x64-nsis.exe")"

base_attestation() {
  python3 -c '
import json,sys
print(json.dumps({
  "schemaVersion": 1,
  "candidateTag": sys.argv[1],
  "candidateSha256": sys.argv[2],
  "completedAtUtc": "2026-07-26T23:00:00Z",
  "operator": "release-lane fixture operator",
  "captchaHandling": "paused_for_manual_completion",
  "vms": [
    {"name": "OSL-QA-A", "goldenSnapshotId": "snapshot-a", "cleanRestore": True},
    {"name": "OSL-QA-B", "goldenSnapshotId": "snapshot-b", "cleanRestore": True}
  ],
  "cases": {
    "onboarding": True, "identityCreate": True, "identityRecover": True,
    "twoAccountLogin": True, "persistenceRestart": True, "signedUpdate": True,
    "oneSidedEncryption": True, "twoSidedEncryption": True, "fullCleanup": True
  }
}, indent=2))' "$tag" "$installer_sha"
}

# mutate <python-expression-over-`d`>  -> writes $work/att.json
mutate() {
  base_attestation | python3 -c '
import json,sys
d = json.load(sys.stdin)
exec(sys.argv[1])
json.dump(d, sys.stdout, indent=2)' "$1" > "$work/att.json"
}

passes=0
failures=0

# expect_reject <label> <expected-message-substring>
expect_reject() {
  local label="$1" expected="$2" out status
  out="$(python3 "$verifier" --tag "$tag" --candidate-dir "$candidate" --attestation "$work/att.json" 2>&1)"
  status=$?
  if [ "$status" -eq 0 ]; then
    echo "  FAIL  $label -> gate ACCEPTED a candidate it must reject"
    failures=$((failures + 1))
  elif ! printf '%s' "$out" | grep -qF "$expected"; then
    echo "  FAIL  $label -> rejected, but for the wrong reason:"
    echo "        got:      $out"
    echo "        expected: $expected"
    failures=$((failures + 1))
  else
    echo "  ok    $label -> refused: $out"
    passes=$((passes + 1))
  fi
}

echo "== positive control: a well-formed attestation must be ACCEPTED =="
mutate 'pass'
if out="$(python3 "$verifier" --tag "$tag" --candidate-dir "$candidate" --attestation "$work/att.json" 2>&1)"; then
  echo "  ok    valid candidate -> $out"
  passes=$((passes + 1))
else
  echo "  FAIL  valid candidate was REJECTED: $out"
  echo "        The battery below is meaningless if the gate rejects everything."
  failures=$((failures + 1))
fi

echo
echo "== negative controls: each of these must be REFUSED =="

mutate 'd["candidateTag"] = "hub-v9.9.9"'
expect_reject "attestation is for a different tag" "QA attestation tag does not match"

mutate 'd["candidateSha256"] = "0" * 64'
expect_reject "attestation describes a different binary" "does not match the exact candidate installer"

mutate 'd["candidateSha256"] = "not-a-hash"'
expect_reject "malformed candidate hash" "candidateSha256 is invalid"

mutate 'd["schemaVersion"] = 2'
expect_reject "unknown schema version" "unsupported QA attestation schema"

mutate 'd["vms"] = d["vms"][:1]'
expect_reject "only one VM was tested" "exactly two clean VM runs are required"

mutate 'd["vms"][1]["goldenSnapshotId"] = d["vms"][0]["goldenSnapshotId"]'
expect_reject "same golden snapshot restored twice" "golden snapshot IDs must be distinct"

mutate 'd["vms"][1]["name"] = d["vms"][0]["name"]'
expect_reject "same VM name used twice" "VM names must be distinct"

mutate 'd["vms"][0]["cleanRestore"] = False'
expect_reject "VM was not a clean restore" "must attest a clean golden restore"

mutate 'd["vms"][0]["cleanRestore"] = "true"'
expect_reject "clean restore attested as a string, not a boolean" "must attest a clean golden restore"

mutate 'del d["cases"]["signedUpdate"]'
expect_reject "a required QA case was omitted" "case set is incomplete or unknown"

mutate 'd["cases"]["extraCase"] = True'
expect_reject "an unrecognised QA case was added" "case set is incomplete or unknown"

mutate 'd["cases"]["twoSidedEncryption"] = False'
expect_reject "a required QA case failed" "every required QA case must pass"

mutate 'd["captchaHandling"] = "automated"'
expect_reject "CAPTCHA was automated around" "CAPTCHA handling must explicitly pause"

mutate 'd["operator"] = "   "'
expect_reject "no accountable operator" "needs an accountable operator"

mutate 'd["completedAtUtc"] = "2026-07-26 23:00:00"'
expect_reject "completion time is not UTC" "needs a UTC completion timestamp"

echo
echo "== negative controls on the candidate artifacts themselves =="
mutate 'pass'

mv "$candidate/latest.json" "$work/latest.json.hidden"
expect_reject "signed updater manifest missing" "signed updater manifest is missing"
mv "$work/latest.json.hidden" "$candidate/latest.json"

printf 'a second, ambiguous installer\n' > "$candidate/osl-hub-0.1.0-x64-setup.exe"
expect_reject "two installers, ambiguous artifact" "exactly one Windows installer"
rm -f "$candidate/osl-hub-0.1.0-x64-setup.exe"

mv "$candidate/osl-hub-0.1.0-x64-nsis.exe" "$work/installer.hidden"
expect_reject "no installer at all" "exactly one Windows installer"
mv "$work/installer.hidden" "$candidate/osl-hub-0.1.0-x64-nsis.exe"

echo
echo "== final control: the gate still ACCEPTS the untouched candidate =="
mutate 'pass'
if out="$(python3 "$verifier" --tag "$tag" --candidate-dir "$candidate" --attestation "$work/att.json" 2>&1)"; then
  echo "  ok    valid candidate still accepted -> $out"
  passes=$((passes + 1))
else
  echo "  FAIL  gate now rejects a valid candidate: $out"
  failures=$((failures + 1))
fi

echo
echo "----------------------------------------"
echo "promotion gate proof: $passes passed, $failures failed"
if [ "$failures" -ne 0 ]; then
  echo "::error::The promotion gate did not behave as specified" >&2
  exit 1
fi
echo "The promotion gate demonstrably accepts a valid candidate and refuses"
echo "every tested way of promoting an untested or substituted one."
