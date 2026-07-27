#!/usr/bin/env bash
# VM lifecycle, snapshots and leak prevention for the OSL QA fleet.
#
# Azure for Students is a FIXED CREDIT POOL, not a billing account. A forgotten running D2s_v3 is
# the only way this workflow costs real money, so leak prevention is the primary design constraint
# here rather than an afterthought: `leak-check` reports on the whole subscription, not just the
# table below, because a VM someone else created can drain the same pool.
set -euo pipefail


# Closed table. An unknown name is refused rather than passed through to az, so a typo cannot
# start or snapshot something nobody meant to touch.
fleet_rg() {
  case "$1" in
    OSL-Azure-Client-1|OSL-Azure-Client-2)             echo "OSL-TWO-CLIENT-LAB" ;;
    OSL-Independent-Client-1|OSL-Independent-Client-2) echo "OSL-TWO-CLIENT-LAB-INDEPENDENT" ;;
    OSL-WhatsApp-Client-1|OSL-WhatsApp-Client-2)       echo "OSL-WHATSAPP-TWO-CLIENT-LAB" ;;
    OSL-Telegram-QA-1|OSL-Telegram-QA-2)               echo "OSL-TELEGRAM-QA" ;;
    OSL-Signal-Client-1|OSL-Signal-Client-2)           echo "OSL-SIGNAL-QA-SCUS" ;;
    *) return 1 ;;
  esac
}

ALL_VMS=(
  OSL-Azure-Client-1 OSL-Azure-Client-2
  OSL-Independent-Client-1 OSL-Independent-Client-2
  OSL-WhatsApp-Client-1 OSL-WhatsApp-Client-2
  OSL-Telegram-QA-1 OSL-Telegram-QA-2
  OSL-Signal-Client-1 OSL-Signal-Client-2
)

# Pair aliases. Expanding here keeps the closed table as the single source of truth.
expand_target() {
  case "$1" in
    crypto)      echo "OSL-Azure-Client-1 OSL-Azure-Client-2" ;;
    scrub)       echo "OSL-Independent-Client-1 OSL-Independent-Client-2" ;;
    whatsapp)    echo "OSL-WhatsApp-Client-1 OSL-WhatsApp-Client-2" ;;
    telegram)    echo "OSL-Telegram-QA-1 OSL-Telegram-QA-2" ;;
    signal)      echo "OSL-Signal-Client-1 OSL-Signal-Client-2" ;;
    all)         echo "${ALL_VMS[*]}" ;;
    *)
      if fleet_rg "$1" >/dev/null 2>&1; then echo "$1"
      else echo "unknown VM or pair: $1" >&2; return 1; fi ;;
  esac
}

power_state() {
  az vm get-instance-view -g "$(fleet_rg "$1")" -n "$1" \
     --query "instanceView.statuses[?starts_with(code,'PowerState/')].displayStatus | [0]" -o tsv 2>/dev/null
}

agent_ready() {
  az vm get-instance-view -g "$(fleet_rg "$1")" -n "$1" \
     --query "instanceView.vmAgent.statuses[0].displayStatus" -o tsv 2>/dev/null
}

cmd_status() {
  local target="${1:-all}" vm rg vms
  vms="$(expand_target "$target")" || return 64
  printf '%-28s %-34s %-16s %s\n' NAME RESOURCE-GROUP POWER SIZE
  for vm in $vms; do
    rg="$(fleet_rg "$vm")"
    printf '%-28s %-34s %-16s %s\n' "$vm" "$rg" \
      "$(power_state "$vm")" \
      "$(az vm show -g "$rg" -n "$vm" --query hardwareProfile.vmSize -o tsv 2>/dev/null)"
  done
}

cmd_start() {
  local vm start elapsed ready vms
  [ $# -ge 1 ] || { echo "start needs a vm or pair" >&2; return 64; }
  vms="$(expand_target "$1")" || return 64
  for vm in $vms; do
    echo "starting $vm ..."
    az vm start -g "$(fleet_rg "$vm")" -n "$vm" --no-wait
  done
  for vm in $vms; do
    start=$(date +%s)
    while :; do
      elapsed=$(( $(date +%s) - start ))
      # Both conditions matter: 'VM running' only means the hypervisor started it. The guest agent
      # being Ready is what makes run-command and the logon task actually usable.
      if [ "$(power_state "$vm")" = "VM running" ]; then
        ready="$(agent_ready "$vm")"
        [ "$ready" = "Ready" ] && { echo "$vm running, agent Ready after ${elapsed}s"; break; }
      fi
      [ "$elapsed" -gt 300 ] && { echo "TIMEOUT: $vm not ready after ${elapsed}s" >&2; return 4; }
      sleep 5
    done
  done
}

cmd_stop() {
  local vm start elapsed vms
  [ $# -ge 1 ] || { echo "stop needs a vm or pair" >&2; return 64; }
  vms="$(expand_target "$1")" || return 64
  for vm in $vms; do
    # deallocate, NOT `az vm stop`: a stopped-but-allocated VM still bills compute, which defeats
    # the entire point of turning it off on a fixed credit pool.
    echo "deallocating $vm ..."
    az vm deallocate -g "$(fleet_rg "$vm")" -n "$vm" --no-wait
  done
  for vm in $vms; do
    start=$(date +%s)
    while :; do
      elapsed=$(( $(date +%s) - start ))
      [ "$(power_state "$vm")" = "VM deallocated" ] && { echo "$vm deallocated after ${elapsed}s"; break; }
      [ "$elapsed" -gt 600 ] && { echo "TIMEOUT: $vm still not deallocated after ${elapsed}s" >&2; return 4; }
      sleep 5
    done
  done
}

cmd_leak_check() {
  # Subscription-wide on purpose. Scoping this to the closed table would miss exactly the VM
  # nobody remembers creating, which is the one that drains the pool.
  local running
  running="$(az vm list -d --query "[?powerState=='VM running'].{n:name,rg:resourceGroup}" -o tsv 2>/dev/null || true)"
  if [ -z "$running" ]; then echo "leak-check: nothing running in the subscription"; return 0; fi
  echo "LEAK: these VMs are still running and are burning credit:" >&2
  echo "$running" >&2
  return 7
}

cmd_cleanup_receipt() {
  local vm="${1:-}" run_id="${2:-}" exe_sha="${3:-}" identity_sha="${4:-}" out_dir="${5:-}"
  local rg subscription_id subscription_sha captured raw_instance raw_census
  local instance_sha census_sha running_count
  [ $# -eq 5 ] || {
    echo "cleanup-receipt needs <vm> <run-id> <exe-sha256> <build-identity-sha256> <out-dir>" >&2
    return 64
  }
  fleet_rg "$vm" >/dev/null || { echo "unknown vm: $vm" >&2; return 64; }
  [[ "$run_id" =~ ^[A-Za-z0-9._-]{1,96}$ ]] || { echo "unsafe run id: $run_id" >&2; return 64; }
  [[ "$exe_sha" =~ ^[0-9a-f]{64}$ ]] || { echo "invalid executable sha256" >&2; return 64; }
  [[ "$identity_sha" =~ ^[0-9a-f]{64}$ ]] || { echo "invalid build identity sha256" >&2; return 64; }
  command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; return 69; }
  command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; return 69; }
  [ -f "$out_dir/build-identity.json" ] || {
    echo "build identity is missing from report directory: $out_dir/build-identity.json" >&2
    return 66
  }
  [ "$(sha256sum "$out_dir/build-identity.json" | awk '{print $1}')" = "$identity_sha" ] || {
    echo "build identity sha256 does not match retained report bytes" >&2
    return 9
  }
  python3 "$(dirname -- "${BASH_SOURCE[0]}")/vmqa-contract.py" \
    validate-build --build-identity "$out_dir/build-identity.json" || return 9
  rg="$(fleet_rg "$vm")"
  subscription_id="$(az account show --query id -o tsv)"
  [ -n "$subscription_id" ] || { echo "Azure subscription identity is unavailable" >&2; return 5; }
  subscription_sha="$(printf '%s' "$subscription_id" | sha256sum | awk '{print $1}')"
  captured="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  raw_instance="$(mktemp)"
  raw_census="$(mktemp)"
  if ! az vm get-instance-view -g "$rg" -n "$vm" -o json >"$raw_instance"; then
    rm -f -- "$raw_instance" "$raw_census"
    return 5
  fi
  if ! az vm list -d -o json >"$raw_census"; then
    rm -f -- "$raw_instance" "$raw_census"
    return 5
  fi
  mkdir -p -- "$out_dir"
  jq --arg captured "$captured" --arg subscription "$subscription_sha" '
    {
      schemaVersion:2,
      capturedUtc:$captured,
      subscriptionIdSha256:$subscription,
      vm:{
        id:.id,
        name:.name,
        resourceGroup:.resourceGroup,
        location:.location,
        powerState:(
          [.instanceView.statuses[]?
           | select((.code // "") | startswith("PowerState/"))
           | .displayStatus] | first // "unknown"
        ),
        agentStatus:(.instanceView.vmAgent.statuses[0].displayStatus // "unknown"),
        provisioningState:(
          [.instanceView.statuses[]?
           | select((.code // "") | startswith("ProvisioningState/"))
           | .displayStatus] | first // "unknown"
        )
      }
    }
  ' "$raw_instance" >"$out_dir/azure-instance-view.json"
  jq --arg captured "$captured" --arg subscription "$subscription_sha" '
    {
      schemaVersion:2,
      capturedUtc:$captured,
      subscriptionIdSha256:$subscription,
      vms:(
        [.[] | {
          id:.id,
          name:.name,
          resourceGroup:.resourceGroup,
          powerState:(.powerState // "unknown")
        }] | sort_by(.id)
      )
    }
  ' "$raw_census" >"$out_dir/azure-subscription-census.json"
  rm -f -- "$raw_instance" "$raw_census"
  instance_sha="$(sha256sum "$out_dir/azure-instance-view.json" | awk '{print $1}')"
  census_sha="$(sha256sum "$out_dir/azure-subscription-census.json" | awk '{print $1}')"
  running_count="$(jq '[.vms[] | select(.powerState=="VM running")] | length' \
    "$out_dir/azure-subscription-census.json")"
  jq -n \
    --arg runId "$run_id" \
    --arg exe "$exe_sha" \
    --arg identity "$identity_sha" \
    --arg vm "$vm" \
    --arg rg "$rg" \
    --arg instanceSha "$instance_sha" \
    --arg censusSha "$census_sha" \
    --argjson runningCount "$running_count" \
    --arg captured "$captured" '
      {
        schemaVersion:2,
        runId:$runId,
        exeSha256:$exe,
        buildIdentitySha256:$identity,
        targetVm:$vm,
        targetResourceGroup:$rg,
        instanceViewFile:"azure-instance-view.json",
        instanceViewSha256:$instanceSha,
        censusFile:"azure-subscription-census.json",
        censusSha256:$censusSha,
        deallocated:true,
        runningCount:$runningCount,
        capturedUtc:$captured
      }
    ' >"$out_dir/azure-cleanup-receipt.json"
  if ! python3 "$(dirname -- "${BASH_SOURCE[0]}")/vmqa-contract.py" \
      verify-cleanup --directory "$out_dir"; then
    echo "REFUSED: Azure cleanup JSON did not prove deallocation and zero subscription leaks" >&2
    return 9
  fi
  printf 'cleanup receipt: %s\n' "$out_dir/azure-cleanup-receipt.json"
}

cmd_stop_all() {
  [ "${1:-}" = "--yes" ] || { echo "stop-all requires --yes" >&2; return 64; }
  local vm any=0
  for vm in "${ALL_VMS[@]}"; do
    if [ "$(power_state "$vm")" = "VM running" ]; then
      echo "will deallocate $vm"; any=1
      az vm deallocate -g "$(fleet_rg "$vm")" -n "$vm" --no-wait
    fi
  done
  [ "$any" = 0 ] && echo "stop-all: nothing was running"
  return 0
}

os_disk_id()  { az vm show -g "$(fleet_rg "$1")" -n "$1" --query storageProfile.osDisk.managedDisk.id -o tsv; }
# Read the location from the DISK, never from a fleet-wide constant. Azure region policy refuses a
# snapshot that inherits a default region instead of the source disk's, and the correct value
# differs per fleet: the crypto pair is centralus while the Independent pair is northcentralus.
disk_location() { az disk show --ids "$1" --query location -o tsv; }

cmd_snapshot() {
  local vm="${1:-}" label="${2:-}" rg disk loc name state lineage
  [ -n "$vm" ] && [ -n "$label" ] || { echo "snapshot needs <vm> <label>" >&2; return 64; }
  fleet_rg "$vm" >/dev/null || { echo "unknown vm: $vm" >&2; return 64; }

  # Lineage is enforced in the name, not in a comment. A snapshot whose name does not say which
  # lineage it belongs to is untrusted: a WARM image cannot satisfy a release gate, because it has
  # already been taught everything the gate is trying to discover.
  case "$label" in
    WARM-*|COLD-*) ;;
    *) echo "REFUSED: label must start with WARM- or COLD-. Got '$label'." >&2
       echo "  WARM = Discord signed in / identity present, for fast iteration." >&2
       echo "  COLD = clean box, what the release gate requires." >&2
       return 64 ;;
  esac

  state="$(power_state "$vm")"
  [ "$state" = "VM deallocated" ] || {
    echo "REFUSED: $vm is '$state'. Snapshot a deallocated VM only — a snapshot of a running" >&2
    echo "         Windows box is crash-consistent at best." >&2; return 3; }

  rg="$(fleet_rg "$vm")"; disk="$(os_disk_id "$vm")"; loc="$(disk_location "$disk")"
  name="${vm}-${label}-$(date -u +%Y%m%d%H%M)"
  case "$label" in WARM-*) lineage=warm-iteration ;; *) lineage=cold-release-gate ;; esac

  echo "creating snapshot $name in $rg at location $loc (from disk's own region)"
  # Standard_LRS: a snapshot is cold storage read a handful of times; premium buys nothing here.
  az snapshot create -g "$rg" -n "$name" --source "$disk" --location "$loc" \
    --sku Standard_LRS --tags "lineage=$lineage" "vm=$vm" -o none
  echo "$name"
}

cmd_snapshots() {
  local filter="${1:-}"
  if [ -n "$filter" ]; then
    az snapshot list --query "sort_by([?starts_with(name,'$filter')],&timeCreated)[].{name:name,created:timeCreated,gb:diskSizeGb,lineage:tags.lineage}" -o table
  else
    az snapshot list --query "sort_by([],&timeCreated)[].{name:name,created:timeCreated,gb:diskSizeGb,lineage:tags.lineage}" -o table
  fi
}

cmd_restore() {
  local vm="${1:-}" snap="${2:-}" confirm="${3:-}" rg loc snapid newdisk olddisk state
  [ -n "$vm" ] && [ -n "$snap" ] || { echo "restore needs <vm> <snapshot-name>" >&2; return 64; }
  [ "$confirm" = "--yes-destroy-current-disk" ] || {
    echo "REFUSED: restore swaps the OS disk. Pass --yes-destroy-current-disk to confirm." >&2; return 64; }
  fleet_rg "$vm" >/dev/null || { echo "unknown vm: $vm" >&2; return 64; }

  state="$(power_state "$vm")"
  [ "$state" = "VM deallocated" ] || { echo "REFUSED: $vm is '$state'; deallocate first." >&2; return 3; }

  rg="$(fleet_rg "$vm")"
  snapid="$(az snapshot show -g "$rg" -n "$snap" --query id -o tsv)"
  loc="$(az snapshot show -g "$rg" -n "$snap" --query location -o tsv)"
  olddisk="$(os_disk_id "$vm")"
  newdisk="${vm}-osdisk-$(date -u +%Y%m%d%H%M)"

  echo "restoring $vm from $snap"
  echo "  creating disk $newdisk in $loc"
  az disk create -g "$rg" -n "$newdisk" --source "$snapid" --location "$loc" -o none
  echo "  swapping OS disk"
  az vm update -g "$rg" -n "$vm" --os-disk "$(az disk show -g "$rg" -n "$newdisk" --query id -o tsv)" -o none

  echo "restore complete."
  echo "OLD DISK NOT DELETED: $olddisk"
  echo "  Delete it yourself once the restore is proven. Doing it automatically here would be"
  echo "  unrecoverable, and a bad restore is exactly when you still want the old disk."
}

usage() {
  cat >&2 <<'USAGE'
vmqa-fleet.sh — VM lifecycle, snapshots and leak prevention

  status [vm|pair|all]
  start  <vm|pair>                 waits for running AND guest agent Ready
  stop   <vm|pair>                 deallocate (not 'stop' — stop still bills compute)
  leak-check                       subscription-wide; exit 7 if anything is running
  cleanup-receipt <vm> <run-id> <exe-sha> <build-identity-sha> <out-dir>
                                   retain hash-bound instance/deallocation/census JSON
  stop-all --yes                   deallocate every fleet VM currently running

  snapshot  <vm> <WARM-…|COLD-…>   requires deallocated; pins --location to the disk's own region
  snapshots [name-prefix]
  restore   <vm> <snapshot> --yes-destroy-current-disk

Pairs: crypto scrub whatsapp telegram signal all
Lineages: WARM = iteration (Discord/identity present).  COLD = clean box, release gate.
USAGE
  exit 64
}

case "${1:-}" in
  status)     shift; cmd_status "${1:-all}" ;;
  start)      shift; cmd_start "$@" ;;
  stop)       shift; cmd_stop "$@" ;;
  leak-check) shift; cmd_leak_check ;;
  cleanup-receipt) shift; cmd_cleanup_receipt "$@" ;;
  stop-all)   shift; cmd_stop_all "$@" ;;
  snapshot)   shift; cmd_snapshot "$@" ;;
  snapshots)  shift; cmd_snapshots "$@" ;;
  restore)    shift; cmd_restore "$@" ;;
  *) usage ;;
esac
