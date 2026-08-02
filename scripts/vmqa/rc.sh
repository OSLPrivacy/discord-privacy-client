#!/usr/bin/env bash
# rc.sh <vm> <local-ps1-or->: run a PowerShell script remotely under a VM lease.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
VM=${1:?usage: rc.sh <vm> <local-ps1-or->}
SRC=${2:--}
LEASE_ID=$("$SCRIPT_DIR/lease.sh" acquire "$VM")
tmp=""
release() {
  "$SCRIPT_DIR/lease.sh" release "$VM" "$LEASE_ID" >/dev/null 2>&1 || \
    echo "VM-LEASE-RELEASE-ERROR vm=$VM" >&2
  [ -z "$tmp" ] || rm -f -- "$tmp"
}
trap release EXIT

if [ "$SRC" = "-" ]; then
  tmp=$(mktemp --suffix=.ps1)
  cat >"$tmp"
  SRC=$tmp
fi
[ -f "$SRC" ] || { echo "PowerShell source not found: $SRC" >&2; exit 66; }

for _ in $(seq 1 40); do
  if out=$(az vm run-command invoke -g "${VMQA_RG:-osl-two-client-lab}" -n "$VM" \
      --command-id RunPowerShellScript --scripts "@$SRC" --query 'value[0].message' -o tsv 2>&1); then
    printf '%s\n' "$out"
    exit 0
  fi
  if [[ "$out" == *'Run command extension execution is in progress'* ]]; then sleep 15; continue; fi
  printf '%s\n' "$out" >&2
  exit 1
done
echo "RC-TIMEOUT: $VM stayed busy" >&2
exit 1
