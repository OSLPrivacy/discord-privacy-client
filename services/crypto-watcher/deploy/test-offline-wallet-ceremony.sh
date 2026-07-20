#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CEREMONY="$SCRIPT_DIR/generate-offline-merchant-wallets.sh"
IMPORTER="$SCRIPT_DIR/provision-watch-only-wallets.sh"

bash -n "$CEREMONY" "$IMPORTER"

require_literal() {
  local text=$1
  grep -Fq -- "$text" "$CEREMONY" || {
    printf 'Missing ceremony safety invariant: %s\n' "$text" >&2
    exit 1
  }
}

require_literal 'ip -4 route show table all'
require_literal 'ip -6 route show table all'
require_literal 'ip -o link show up'
require_literal '/proc/swaps'
require_literal '/sys/power/resume'
require_literal 'I CONFIRM THIS MACHINE IS PHYSICALLY OFFLINE'
require_literal 'I RECORDED THE MONERO SEED OFFLINE'
require_literal 'I STORED BOTH WALLET PASSPHRASES OFFLINE'
require_literal 'I COPIED BOTH ENCRYPTED WALLET BACKUPS TO DURABLE OFFLINE MEDIA'
require_literal 'BITCOIND_BIN_SHA256'
require_literal 'BITCOIN_CLI_BIN_SHA256'
require_literal 'MONERO_WALLET_CLI_BIN_SHA256'
require_literal 'tmpfs-backed /dev/shm'
require_literal "printf 'passphrase=%s\\n'"
require_literal 'listdescriptors false'
require_literal '.hasprivatekeys == false'
require_literal 'restorewallet wallet_name=osl-btc-restore-test'
require_literal 'listdescriptors true'
require_literal '.hasprivatekeys == true'
require_literal 'XMR_RESTORE_WALLET'
require_literal '--generate-from-view-key'
require_literal 'CEREMONY-INCOMPLETE'
require_literal '.CEREMONY-COMPLETE.tmp'
require_literal 'mktemp mv readlink'
require_literal 'must be on distinct filesystems'
require_literal 'OSL_SINGLE_USB_MODE'
require_literal 'I ACCEPT THAT THIS USB WILL CONTAIN ENCRYPTED SPENDING BACKUPS AND WATCH-ONLY DATA'
require_literal 'must be separate and non-nested'
require_literal '/sys/fs/cgroup${cgroup_path}/memory.swap.max'
require_literal 'the WSL cgroup MemorySwapMax is not zero'
require_literal 'OSL_NONINTERACTIVE_MODE'
require_literal 'OSL-RECOVERY-CREDENTIALS.txt'
require_literal 'sync -f "$RECOVERY_CREDENTIALS_FILE"'
require_literal 'manifest_files+=(OSL-RECOVERY-CREDENTIALS.txt)'

for importer_literal in \
  'OFFLINE_CEREMONY_RECEIPT_FILE' \
  'OFFLINE_CEREMONY_SHA256SUMS_FILE' \
  'Offline ceremony is explicitly marked incomplete.' \
  'sha256sum --strict -c SHA256SUMS' \
  'transfer_manifest_sha256=([0-9a-f]{64})' \
  'MONERO_RESTORE_HEIGHT does not match the completed offline ceremony.'; do
  grep -Fq -- "$importer_literal" "$IMPORTER" || {
    printf 'Online importer is missing ceremony invariant: %s\n' \
      "$importer_literal" >&2
    exit 1
  }
done

for lock_literal in \
  'LOCK_DIR=/run/osl-crypto' \
  'install -d -o root -g root -m 0700 "$LOCK_DIR"' \
  '$(stat -c '\''%u:%g:%a'\'' "$LOCK_DIR") == 0:0:700' \
  '$(stat -c '\''%u:%g:%a'\'' "$LOCK_FILE") == 0:0:600' \
  'exec 9<>"$LOCK_FILE"'; do
  grep -Fq -- "$lock_literal" "$IMPORTER" || {
    printf 'Online importer is missing safe runtime-lock invariant: %s\n' \
      "$lock_literal" >&2
    exit 1
  }
done
if grep -Fq -- '/run/lock/osl-watch-only-provision.lock' "$IMPORTER"; then
  printf 'Online importer still uses the unsafe shared /run/lock path.\n' >&2
  exit 1
fi
require_literal '--offline'
require_literal 'btc-descriptor'
require_literal 'xmr-address'
require_literal 'xmr-view-key'
require_literal 'xmr-restore-height'

transfer_block=$(sed -n \
  '/# BEGIN TRANSFER EXPORTS/,/# END TRANSFER EXPORTS/p' "$CEREMONY")
if grep -Eqi '(xprv|tprv|seed|spend|wallet\.keys|merchant\.wallet)' \
  <<<"$transfer_block"; then
  printf 'Transfer export block appears to contain spending or recovery material.\n' >&2
  exit 1
fi
if grep -Fq -- 'passphrase= avoid_reuse' "$CEREMONY"; then
  printf 'Bitcoin wallet is created unencrypted before its backup passphrase is applied.\n' >&2
  exit 1
fi

backup_ack_line=$(grep -nF \
  'I COPIED BOTH ENCRYPTED WALLET BACKUPS TO DURABLE OFFLINE MEDIA' \
  "$CEREMONY" | cut -d: -f1)
transfer_line=$(grep -nF '# BEGIN TRANSFER EXPORTS' "$CEREMONY" | cut -d: -f1)
[[ "$backup_ack_line" -lt "$transfer_line" ]] || {
  printf 'Watch/view material is exported before durable backup confirmation.\n' >&2
  exit 1
}

mock_address="4$(printf 'A%.0s' {1..94})"
parsed_address=$(printf '0  %s  Primary address\n' "$mock_address" \
  | sed -nE 's/^[[:space:]]*0[[:space:]]+(4[1-9A-HJ-NP-Za-km-z]{94})[[:space:]]+Primary address[[:space:]]*$/\1/p')
[[ "$parsed_address" == "$mock_address" ]] || {
  printf 'Exact labelled Monero primary-address parser rejected its fixture.\n' >&2
  exit 1
}
unlabelled_address=$(printf '%s\n' "$mock_address" \
  | sed -nE 's/^[[:space:]]*0[[:space:]]+(4[1-9A-HJ-NP-Za-km-z]{94})[[:space:]]+Primary address[[:space:]]*$/\1/p')
[[ -z "$unlabelled_address" ]] || {
  printf 'Monero primary-address parser accepted unlabelled output.\n' >&2
  exit 1
}

[[ $(grep -Fc -- 'generate-offline-merchant-wallets.sh' \
  "$SCRIPT_DIR/../README.md") -ge 1 ]] || {
  printf 'README does not route operators through the offline ceremony.\n' >&2
  exit 1
}

TEST_DIR=$(mktemp -d)
cleanup() { rm -rf -- "$TEST_DIR"; }
trap cleanup EXIT
cat >"$TEST_DIR/ip" <<'FAKE_IP'
#!/usr/bin/env bash
set -euo pipefail
case "${FAKE_IP_MODE:-offline}:$*" in
  route:-4\ route\ show\ table\ all)
    printf 'default via 192.0.2.1 dev eth0\n'
    ;;
  iface:-o\ link\ show\ up)
    printf '1: lo: <LOOPBACK,UP> mtu 65536 state UNKNOWN\n'
    printf '2: eth0: <BROADCAST,UP> mtu 1500 state UP\n'
    ;;
  *:-o\ link\ show\ up)
    printf '1: lo: <LOOPBACK,UP> mtu 65536 state UNKNOWN\n'
    ;;
esac
FAKE_IP
chmod 0755 "$TEST_DIR/ip"
cat >"$TEST_DIR/jq" <<'FAKE_JQ'
#!/usr/bin/env bash
exit 0
FAKE_JQ
chmod 0755 "$TEST_DIR/jq"

expect_preflight_failure() {
  local mode=$1 expected=$2 output
  output="$TEST_DIR/$mode.out"
  if PATH="$TEST_DIR:$PATH" FAKE_IP_MODE="$mode" \
    "$CEREMONY" --preflight >"$output" 2>&1; then
    printf 'Preflight unexpectedly accepted mode: %s\n' "$mode" >&2
    exit 1
  fi
  grep -Fq -- "$expected" "$output" || {
    printf 'Preflight did not report expected %s failure.\n' "$mode" >&2
    exit 1
  }
}

expect_preflight_failure route 'a default IPv4 or IPv6 route exists'
expect_preflight_failure iface 'active non-loopback interface(s): eth0'

printf 'Offline wallet ceremony static and behavioral checks passed.\n'
