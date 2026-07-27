#!/usr/bin/env bash
set -u

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/release/create-cold-snapshot.sh --vm <name> --resource-group <rg> --evidence <file> [--execute]
  scripts/release/create-cold-snapshot.sh --self-test
USAGE
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

quote_command() {
  local arg quoted
  local parts=()

  for arg in "$@"; do
    printf -v quoted '%q' "$arg"
    parts+=("$quoted")
  done

  (IFS=' '; printf '%s\n' "${parts[*]}")
}

require_value() {
  local flag=$1
  local value=${2-}

  if [ -z "$value" ]; then
    usage
    die "$flag requires a value"
  fi
}

snapshot_name_for_vm() {
  local vm=$1
  printf '%s-COLD-releasegate-%s\n' "$vm" "$(date -u +%Y%m%d)"
}

build_snapshot_command() {
  local vm=$1
  local rg=$2
  local evidence=$3

  [ -s "$evidence" ] || die "--evidence must name an existing non-empty file"

  local evidence_hash
  if ! evidence_hash=$(sha256sum "$evidence" | awk '{print $1}'); then
    die "could not compute sha256 for evidence file"
  fi

  local power_state
  if ! power_state=$(az vm show -d --resource-group "$rg" --name "$vm" --query powerState -o tsv); then
    die "could not query VM power state"
  fi
  [ "$power_state" = "VM deallocated" ] || die "source VM must be VM deallocated, got: $power_state"

  local disk_id
  if ! disk_id=$(az vm show --resource-group "$rg" --name "$vm" --query storageProfile.osDisk.managedDisk.id -o tsv); then
    die "could not query source OS disk"
  fi
  if [ -z "$disk_id" ] || [ "$disk_id" = "None" ] || [ "$disk_id" = "null" ]; then
    die "could not determine source OS disk"
  fi

  local disk_location
  if ! disk_location=$(az disk show --ids "$disk_id" --query location -o tsv); then
    die "could not query source disk location"
  fi
  if [ -z "$disk_location" ] || [ "$disk_location" = "None" ] || [ "$disk_location" = "null" ]; then
    die "could not determine source disk location"
  fi

  local snapshot_name
  snapshot_name=$(snapshot_name_for_vm "$vm")
  if az snapshot show --resource-group "$rg" --name "$snapshot_name" --query name -o tsv >/dev/null 2>&1; then
    die "snapshot already exists: $snapshot_name"
  fi

  SNAPSHOT_COMMAND=(
    az snapshot create
    --resource-group "$rg"
    --name "$snapshot_name"
    --source "$disk_id"
    --location "$disk_location"
    --incremental true
    --tags
    lineage=release-cold
    owner=release-lane
    "evidence-sha256=$evidence_hash"
    "purpose=Release-gate cold image, no OSL identity or service login"
  )
}

create_stub_az() {
  local stub=$1

  cat >"$stub" <<'STUB'
#!/usr/bin/env bash
set -u

has_arg() {
  local wanted=$1
  shift
  local arg
  for arg in "$@"; do
    [ "$arg" = "$wanted" ] && return 0
  done
  return 1
}

printf 'az %s\n' "$*" >>"${STUB_AZ_LOG:?}"

if [ "$#" -ge 2 ] && [ "$1" = "snapshot" ] && [ "$2" = "create" ]; then
  printf 'MUTATING az snapshot create\n' >>"$STUB_AZ_LOG"
  printf 'unexpected mutating call\n' >&2
  exit 99
fi

if [ "$#" -ge 2 ] && [ "$1" = "vm" ] && [ "$2" = "show" ]; then
  if has_arg -d "$@"; then
    printf '%s\n' "${STUB_POWER_STATE:-VM deallocated}"
  else
    printf '%s\n' "${STUB_DISK_ID:-/subscriptions/test/resourceGroups/rg/providers/Microsoft.Compute/disks/osdisk}"
  fi
  exit 0
fi

if [ "$#" -ge 2 ] && [ "$1" = "disk" ] && [ "$2" = "show" ]; then
  printf '%s\n' "${STUB_DISK_LOCATION-northcentralus}"
  exit 0
fi

if [ "$#" -ge 2 ] && [ "$1" = "snapshot" ] && [ "$2" = "show" ]; then
  if [ "${STUB_SNAPSHOT_EXISTS:-0}" = "1" ]; then
    printf '%s\n' "${STUB_SNAPSHOT_NAME:-existing-snapshot}"
    exit 0
  fi
  printf 'not found\n' >&2
  exit 3
fi

printf 'unsupported az stub call: %s\n' "$*" >&2
exit 64
STUB
  chmod +x "$stub"
}

self_test() {
  local tmpdir
  tmpdir=$(mktemp -d)
  SELF_TEST_TMPDIR=$tmpdir
  trap 'rm -rf "$SELF_TEST_TMPDIR"' EXIT

  local script_path
  case $0 in
    /*) script_path=$0 ;;
    *) script_path=$PWD/$0 ;;
  esac

  create_stub_az "$tmpdir/az"

  local evidence="$tmpdir/evidence.txt"
  local empty_evidence="$tmpdir/empty.txt"
  local missing_evidence="$tmpdir/missing.txt"
  printf 'inspected clean while running\n' >"$evidence"
  : >"$empty_evidence"

  local passed=0
  local failed=0

  pass() {
    passed=$((passed + 1))
  }

  fail() {
    failed=$((failed + 1))
    printf 'FAIL: %s\n' "$1" >&2
  }

  local out="$tmpdir/out.txt"
  local err="$tmpdir/err.txt"
  local log="$tmpdir/az.log"

  : >"$log"
  if STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$evidence" >"$out" 2>"$err"; then
    pass
  else
    fail "valid dry-run should pass"
  fi

  if grep -Fq 'az snapshot create' "$out"; then
    pass
  else
    fail "dry-run should print az snapshot create"
  fi

  if grep -Fq -- '--location northcentralus' "$out"; then
    pass
  else
    fail "dry-run should use source disk location"
  fi

  if grep -Fq 'lineage=release-cold' "$out"; then
    pass
  else
    fail "dry-run should include cold lineage tag"
  fi

  if grep -Fq -- '--incremental true' "$out"; then
    pass
  else
    fail "dry-run should request incremental snapshot"
  fi
  if grep -Fq 'MUTATING' "$log"; then
    fail "dry-run should not call mutating az"
  else
    pass
  fi

  if STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg >"$out" 2>"$err"; then
    fail "missing evidence should refuse"
  else
    pass
  fi

  if STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$empty_evidence" >"$out" 2>"$err"; then
    fail "empty evidence should refuse"
  else
    pass
  fi

  if STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$missing_evidence" >"$out" 2>"$err"; then
    fail "nonexistent evidence should refuse"
  else
    pass
  fi

  if STUB_POWER_STATE='VM running' STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$evidence" >"$out" 2>"$err"; then
    fail "running VM should refuse"
  else
    pass
  fi

  if STUB_SNAPSHOT_EXISTS=1 STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$evidence" >"$out" 2>"$err"; then
    fail "existing snapshot should refuse"
  else
    pass
  fi

  if STUB_DISK_LOCATION='' STUB_AZ_LOG="$log" PATH="$tmpdir:$PATH" bash "$script_path" --vm rcvm --resource-group rcrg --evidence "$evidence" >"$out" 2>"$err"; then
    fail "missing disk location should refuse"
  else
    pass
  fi

  printf '%s passed, %s failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}

main() {
  if [ "$#" -eq 1 ] && [ "$1" = "--self-test" ]; then
    self_test
    exit $?
  fi

  local vm=''
  local rg=''
  local evidence=''
  local execute=0

  while [ "$#" -gt 0 ]; do
    case $1 in
      --vm)
        require_value "$1" "${2-}"
        vm=$2
        shift 2
        ;;
      --resource-group)
        require_value "$1" "${2-}"
        rg=$2
        shift 2
        ;;
      --evidence)
        require_value "$1" "${2-}"
        evidence=$2
        shift 2
        ;;
      --execute)
        execute=1
        shift
        ;;
      --self-test)
        usage
        die "--self-test cannot be combined with other arguments"
        ;;
      *)
        usage
        die "unknown argument: $1"
        ;;
    esac
  done

  [ -n "$vm" ] || die "--vm is required"
  [ -n "$rg" ] || die "--resource-group is required"
  [ -n "$evidence" ] || die "--evidence is required"

  local SNAPSHOT_COMMAND=()
  build_snapshot_command "$vm" "$rg" "$evidence"

  if [ "$execute" -eq 1 ]; then
    "${SNAPSHOT_COMMAND[@]}"
  else
    quote_command "${SNAPSHOT_COMMAND[@]}"
  fi
}

main "$@"
