#!/usr/bin/env bash
# Server-side mutual exclusion for a VMQA VM.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ACTION=${1:?usage: lease.sh acquire <vm> | release <vm> <lease-id>}
VM=${2:?usage: lease.sh acquire <vm> | release <vm> <lease-id>}
SA=${VMQA_LEASE_SA:-${VMQA_SA:-osltestartifactsa7d5}}
CONTAINER=${VMQA_LEASE_CONTAINER:-${VMQA_CONTAINER:-vmqa}}

case "$VM" in
  ''|.|..|*/*) echo "invalid VM name: $VM" >&2; exit 64 ;;
esac

BLOB="leases/${VM}.lock"
az_args=(--account-name "$SA" --auth-mode login --container-name "$CONTAINER" --blob-name "$BLOB")

case "$ACTION" in
  acquire)
    empty=$(mktemp)
    trap 'rm -f -- "$empty"' EXIT
    az storage blob upload --account-name "$SA" --auth-mode login --container-name "$CONTAINER" \
      --name "$BLOB" --file "$empty" --overwrite false --no-progress -o none >/dev/null 2>&1 || true
    if ! lease_id=$(az storage blob lease acquire "${az_args[@]}" --lease-duration -1 -o tsv 2>/dev/null); then
      holder=$(az storage blob metadata show "${az_args[@]}" --query holder -o tsv 2>/dev/null || true)
      printf 'VM-LEASE-DENIED vm=%s holder=%s\n' "$VM" "${holder:-unknown}" >&2
      exit 1
    fi
    [ -n "$lease_id" ] || { echo "VM-LEASE-ERROR vm=$VM Azure returned no lease id" >&2; exit 1; }
    holder=${VMQA_LEASE_HOLDER:-"${USER:-unknown}@$(hostname)"}
    if ! az storage blob metadata update "${az_args[@]}" --lease-id "$lease_id" \
      --metadata holder="$holder" -o none >/dev/null; then
      az storage blob lease release "${az_args[@]}" --lease-id "$lease_id" -o none >/dev/null 2>&1 || true
      echo "VM-LEASE-ERROR vm=$VM could not record holder" >&2
      exit 1
    fi
    printf '%s\n' "$lease_id"
    ;;
  release)
    lease_id=${3:?usage: lease.sh release <vm> <lease-id>}
    az storage blob lease release "${az_args[@]}" --lease-id "$lease_id" -o none
    ;;
  *) echo "usage: lease.sh acquire <vm> | release <vm> <lease-id>" >&2; exit 64 ;;
esac
