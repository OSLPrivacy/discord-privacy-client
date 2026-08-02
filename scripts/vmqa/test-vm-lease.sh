#!/usr/bin/env bash
# TH-4: a second agent must be denied while the first holds a VM lease.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/state"

cat >"$TMP/bin/az" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
state=${FAKE_AZ_STATE:?}
case "$1 $2 $3 $4" in
  'storage blob upload '*) touch "$state/object" ;;
  'storage blob lease acquire') [ ! -e "$state/lease" ] || exit 1; printf 'lease-1\n' >"$state/lease"; printf 'lease-1\n' ;;
  'storage blob lease release') rm -f "$state/lease" ;;
  'storage blob metadata update') for arg in "$@"; do case "$arg" in holder=*) printf '%s\n' "${arg#holder=}" >"$state/holder";; esac; done ;;
  'storage blob metadata show') cat "$state/holder" 2>/dev/null || true ;;
esac
SH
chmod +x "$TMP/bin/az"

lease() { PATH="$TMP/bin:$PATH" FAKE_AZ_STATE="$TMP/state" VMQA_LEASE_HOLDER="$1" "$ROOT/lease.sh" "${@:2}"; }
first=$(lease agent-a acquire vm-alpha)
if lease agent-b acquire vm-alpha >"$TMP/second.out" 2>&1; then
  echo 'second agent acquired an already leased VM' >&2; exit 1
fi
grep -qx 'VM-LEASE-DENIED vm=vm-alpha holder=agent-a' "$TMP/second.out"
lease agent-a release vm-alpha "$first"
second=$(lease agent-b acquire vm-alpha)
[ "$second" = lease-1 ]
lease agent-b release vm-alpha "$second"
