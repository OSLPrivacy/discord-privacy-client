#!/usr/bin/env bash
# Regression test for scripts/vmqa/vmqa-fleet.sh — the script that starts, stops, snapshots and
# leak-checks the Azure VM fleet, including the two-identity crypto pair OSL-Azure-Client-1/-2
# (unit b16).
#
# WHY THIS EXISTS. vmqa-fleet.sh is the ONLY thing standing between "iterate on the crypto pair"
# and "leave a Standard_D2s_v3 running on a fixed Azure-for-Students credit pool". Every refusal
# path in it (unknown VM, malformed snapshot label, VM not deallocated, missing --yes confirm) is
# money-safety logic, and none of it had a single automated test before this file: the prior VMQA
# lane report (docs/reports/vmqa-lane-2026-07-26.md) says of vmqa-fleet.sh only "read paths + all
# refusal paths run live" — proven once by hand against real Azure and never pinned as a test.
#
# This drives the REAL functions out of vmqa-fleet.sh (fleet_rg, expand_target, cmd_snapshot,
# cmd_restore, cmd_stop_all, cmd_leak_check, main) against a stubbed `az` that costs nothing and
# touches no subscription. It deliberately does not reimplement the fleet table or the refusal
# logic — a test that re-derives the rules is a double asserting whatever its author believed,
# which is the failure mode this whole lane exists to catch.
#
# Money-safety property under test, repeated at each refusal case: a refused action must produce
# ZERO recorded `az` invocations. A test that only checks the exit code would pass even if the
# refusal happened one line too late, after a cost-incurring call already fired.
#
# Cloud state touched by this file: NONE. `$AZ_STUB_LOG` and the fixture at
# fixtures/fake-az/az are the only things written or executed.
set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=/dev/null
source "$SCRIPT_DIR/vmqa-fleet.sh"
set +e   # vmqa-fleet.sh's `set -e` would otherwise abort this test at its first deliberate failure.

for fn in fleet_rg expand_target cmd_snapshot cmd_restore cmd_stop_all cmd_leak_check cmd_status main; do
  declare -F "$fn" >/dev/null || {
    echo "FATAL: $fn was not defined by sourcing vmqa-fleet.sh (guard regressed?)." >&2
    exit 70
  }
done

TMP="$(mktemp -d)"
trap 'rm -rf -- "$TMP"' EXIT
export AZ_STUB_LOG="$TMP/az-calls.log"
export PATH="$SCRIPT_DIR/fixtures/fake-az:$PATH"
: >"$AZ_STUB_LOG"

pass_count=0; fail_count=0

reset_log() { : >"$AZ_STUB_LOG"; }
log_call_count() { wc -l <"$AZ_STUB_LOG" | tr -d ' '; }

check_rc() {
  local name="$1" expected_rc="$2"; shift 2
  local rc
  "$@" >"$TMP/out.$$" 2>"$TMP/err.$$"
  rc=$?
  if [ "$rc" -eq "$expected_rc" ]; then
    pass_count=$((pass_count + 1)); printf 'ok   %s\n' "$name"
  else
    fail_count=$((fail_count + 1))
    printf 'FAIL %s: expected rc=%s got rc=%s\n' "$name" "$expected_rc" "$rc"
    printf '     stdout: %s\n' "$(cat "$TMP/out.$$")"
    printf '     stderr: %s\n' "$(cat "$TMP/err.$$")"
  fi
  rm -f -- "$TMP/out.$$" "$TMP/err.$$"
}

check_eq() {
  local name="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    pass_count=$((pass_count + 1)); printf 'ok   %s\n' "$name"
  else
    fail_count=$((fail_count + 1))
    printf 'FAIL %s: expected [%s] got [%s]\n' "$name" "$expected" "$actual"
  fi
}

check_no_az_calls() {
  local name="$1" count
  count="$(log_call_count)"
  if [ "$count" -eq 0 ]; then
    pass_count=$((pass_count + 1)); printf 'ok   %s (0 az calls)\n' "$name"
  else
    fail_count=$((fail_count + 1))
    printf 'FAIL %s: expected 0 az calls, got %s:\n%s\n' "$name" "$count" "$(cat "$AZ_STUB_LOG")"
  fi
}

check_log_contains() {
  local name="$1" needle="$2"
  if grep -qF -- "$needle" "$AZ_STUB_LOG"; then
    pass_count=$((pass_count + 1)); printf 'ok   %s\n' "$name"
  else
    fail_count=$((fail_count + 1))
    printf 'FAIL %s: az log does not contain [%s]:\n%s\n' "$name" "$needle" "$(cat "$AZ_STUB_LOG")"
  fi
}

echo "== fleet_rg / expand_target: the closed table an unknown name must not pass through =="

check_eq "fleet_rg crypto VM 1 -> OSL-TWO-CLIENT-LAB" "OSL-TWO-CLIENT-LAB" "$(fleet_rg OSL-Azure-Client-1)"
check_eq "fleet_rg crypto VM 2 -> OSL-TWO-CLIENT-LAB" "OSL-TWO-CLIENT-LAB" "$(fleet_rg OSL-Azure-Client-2)"
check_rc "fleet_rg unknown name is refused, not passed through" 1 fleet_rg "OSL-Azure-Client-1; rm -rf /"

check_eq "expand_target crypto -> exactly the two-VM crypto pair" \
  "OSL-Azure-Client-1 OSL-Azure-Client-2" "$(expand_target crypto)"
check_eq "expand_target all -> all ten fleet VMs" \
  "OSL-Azure-Client-1 OSL-Azure-Client-2 OSL-Independent-Client-1 OSL-Independent-Client-2 OSL-WhatsApp-Client-1 OSL-WhatsApp-Client-2 OSL-Telegram-QA-1 OSL-Telegram-QA-2 OSL-Signal-Client-1 OSL-Signal-Client-2" \
  "$(expand_target all)"
check_eq "expand_target single known vm passes through unexpanded" \
  "OSL-Azure-Client-1" "$(expand_target OSL-Azure-Client-1)"
check_rc "expand_target unknown alias/vm is refused" 1 bash -c \
  "source '$SCRIPT_DIR/vmqa-fleet.sh'; expand_target does-not-exist"

echo
echo "== cmd_snapshot: every refusal must fire before az is ever called =="

reset_log
check_rc "snapshot: missing args -> refused" 64 cmd_snapshot
check_no_az_calls "snapshot: missing args"

reset_log
check_rc "snapshot: unknown vm -> refused" 64 cmd_snapshot "OSL-Not-A-Real-Vm" "WARM-agent"
check_no_az_calls "snapshot: unknown vm"

reset_log
check_rc "snapshot: label missing WARM-/COLD- prefix -> refused" 64 \
  cmd_snapshot OSL-Azure-Client-1 "iteration-1"
check_no_az_calls "snapshot: bad label prefix"

reset_log
check_rc "snapshot: running VM (not deallocated) -> refused" 3 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_POWER_STATE="VM running" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_snapshot OSL-Azure-Client-1 WARM-agent"
check_log_contains "snapshot: running VM still checks power state once" \
  "az vm get-instance-view -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1"
check_rc "snapshot: running VM never reaches 'az snapshot create'" 1 \
  grep -q "snapshot create" "$AZ_STUB_LOG"

echo
echo "== cmd_snapshot: the crypto pair's real success path — region pin and lineage tag =="

reset_log
check_rc "snapshot: deallocated crypto VM, WARM label -> succeeds" 0 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_POWER_STATE="VM deallocated" AZ_STUB_DISK_LOCATION="centralus" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_snapshot OSL-Azure-Client-1 WARM-agent"
check_log_contains "snapshot: pins --location to the disk's own region (centralus, not a default)" \
  "--location centralus"
check_log_contains "snapshot: WARM- label tags lineage=warm-iteration" \
  "lineage=warm-iteration"
check_log_contains "snapshot: tags the source vm name" "vm=OSL-Azure-Client-1"

reset_log
check_rc "snapshot: COLD label -> succeeds" 0 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_POWER_STATE="VM deallocated" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_snapshot OSL-Azure-Client-2 COLD-release-gate"
check_log_contains "snapshot: COLD- label tags lineage=cold-release-gate" \
  "lineage=cold-release-gate"

echo
echo "== cmd_restore: swaps an OS disk, so the confirm flag is load-bearing =="

reset_log
check_rc "restore: missing --yes-destroy-current-disk -> refused" 64 \
  cmd_restore OSL-Azure-Client-1 "some-snapshot"
check_no_az_calls "restore: missing confirm flag"

reset_log
check_rc "restore: unknown vm -> refused even with confirm flag" 64 \
  cmd_restore "OSL-Not-A-Real-Vm" "some-snapshot" --yes-destroy-current-disk
check_no_az_calls "restore: unknown vm"

reset_log
check_rc "restore: running (not deallocated) vm -> refused" 3 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_POWER_STATE="VM running" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_restore OSL-Azure-Client-1 some-snapshot --yes-destroy-current-disk"
check_rc "restore: refused-running never reaches 'az disk create'" 1 \
  grep -q "disk create" "$AZ_STUB_LOG"

echo
echo "== cmd_stop_all: fleet-wide deallocate requires an explicit --yes =="

reset_log
check_rc "stop-all: missing --yes -> refused" 64 cmd_stop_all
check_no_az_calls "stop-all: missing --yes"

echo
echo "== cmd_leak_check: the subscription-wide credit-leak gate =="

reset_log
check_rc "leak-check: nothing running -> exit 0" 0 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_LEAK_LIST="" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_leak_check"

reset_log
check_rc "leak-check: a running VM anywhere in the subscription -> exit 7" 7 \
  env AZ_STUB_LOG="$AZ_STUB_LOG" AZ_STUB_LEAK_LIST="OSL-Azure-Client-1\tOSL-TWO-CLIENT-LAB\n" \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; cmd_leak_check"

echo
echo "== main dispatch: an unrecognised target must not silently succeed =="
echo "   (regression for the exact defect docs/reports/vmqa-lane-2026-07-26.md logs: a typo'd"
echo "   vmqa-fleet.sh status <typo> once printed an error and exited 0)"

reset_log
check_rc "main status <bogus vm/pair> -> nonzero, not a silent pass" 64 \
  bash -c "source '$SCRIPT_DIR/vmqa-fleet.sh'; main status does-not-exist"
check_no_az_calls "main status <bogus>: no az call for an unresolved target"

echo
printf 'passed=%s failed=%s\n' "$pass_count" "$fail_count"
[ "$fail_count" -eq 0 ] || exit 1
[ "$pass_count" -ge 20 ] || { echo "refusing to report success on fewer than 20 assertions" >&2; exit 1; }
