#!/usr/bin/env bash
set -euo pipefail
umask 077

# This ceremony creates spending wallets. Run it only on a dedicated machine
# that is physically disconnected from every network and will remain offline.
# The transfer directory receives watch/view material only.

if [[ $- == *x* ]]; then
  set +x
  printf 'Refusing to create wallets while shell tracing is enabled.\n' >&2
  exit 1
fi
set +x

[[ ${EUID:-$(id -u)} -ne 0 ]] || {
  printf 'Run this ceremony as a dedicated, unprivileged offline user, not root.\n' >&2
  exit 1
}

for command in awk cat chmod cp dirname grep id install ip jq mkdir mktemp mv readlink \
  rm sed sha256sum shred sort stat sync tr; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done

assert_offline() {
  local routes active_interfaces
  routes=$({ ip -4 route show table all; ip -6 route show table all; } 2>/dev/null \
    | awk '$1 == "default" { print }')
  [[ -z "$routes" ]] || {
    printf 'Refusing: a default IPv4 or IPv6 route exists.\n' >&2
    return 1
  }

  active_interfaces=$(ip -o link show up 2>/dev/null \
    | awk -F': ' '{ name=$2; sub(/@.*/, "", name); if (name != "lo") print name }')
  [[ -z "$active_interfaces" ]] || {
    printf 'Refusing: active non-loopback interface(s): %s\n' \
      "$(tr '\n' ' ' <<<"$active_interfaces" | sed 's/[[:space:]]*$//')" >&2
    return 1
  }
}

assert_volatile_memory() {
  local resume_device
  [[ -r /proc/swaps && $(awk 'END { print NR }' /proc/swaps) -eq 1 ]] || {
    printf 'Refusing: swap is active or /proc/swaps cannot be verified empty.\n' >&2
    return 1
  }
  if [[ -r /sys/power/resume ]]; then
    resume_device=$(tr -d '[:space:]' </sys/power/resume)
    [[ "$resume_device" == 0:0 ]] || {
      printf 'Refusing: a hibernation resume device is configured.\n' >&2
      return 1
    }
  fi
}

assert_offline_state() {
  assert_offline
  assert_volatile_memory
}

assert_offline_state

if [[ ${1:-} == --preflight ]]; then
  [[ $# -eq 1 ]] || {
    printf 'Usage: %s [--preflight]\n' "$0" >&2
    exit 64
  }
  printf 'Offline network preflight passed. No wallet was created.\n'
  exit 0
fi
[[ $# -eq 0 ]] || {
  printf 'Usage: %s [--preflight]\n' "$0" >&2
  exit 64
}

read_tty() {
  local prompt=$1 destination=$2 value
  [[ -r /dev/tty ]] || {
    printf 'An interactive terminal is required.\n' >&2
    exit 1
  }
  printf '%b' "$prompt" >/dev/tty
  IFS= read -r value </dev/tty
  printf -v "$destination" '%s' "$value"
}

require_acknowledgement() {
  local expected=$1 prompt=$2 supplied
  read_tty "$prompt\nType exactly: $expected\n> " supplied
  [[ "$supplied" == "$expected" ]] || {
    printf 'Acknowledgement did not match; nothing further was done.\n' >&2
    exit 1
  }
  unset supplied
}

require_acknowledgement \
  'I CONFIRM THIS MACHINE IS PHYSICALLY OFFLINE' \
  'Disconnect Ethernet, Wi-Fi, cellular, Bluetooth networking, VPNs, and virtual adapters.'
assert_offline_state

: "${BITCOIND_BIN:?Set BITCOIND_BIN to the pinned offline bitcoind binary}"
: "${BITCOIN_CLI_BIN:?Set BITCOIN_CLI_BIN to the pinned offline bitcoin-cli binary}"
: "${MONERO_WALLET_CLI_BIN:?Set MONERO_WALLET_CLI_BIN to the pinned offline monero-wallet-cli binary}"
: "${BITCOIND_BIN_SHA256:?Set BITCOIND_BIN_SHA256 to its independently verified SHA-256}"
: "${BITCOIN_CLI_BIN_SHA256:?Set BITCOIN_CLI_BIN_SHA256 to its independently verified SHA-256}"
: "${MONERO_WALLET_CLI_BIN_SHA256:?Set MONERO_WALLET_CLI_BIN_SHA256 to its independently verified SHA-256}"
: "${OFFLINE_BACKUP_DIR:?Set OFFLINE_BACKUP_DIR to a new directory on durable offline media}"
: "${WATCH_ONLY_TRANSFER_DIR:?Set WATCH_ONLY_TRANSFER_DIR to a new, separate transfer directory}"
: "${MONERO_RESTORE_HEIGHT:?Set MONERO_RESTORE_HEIGHT to the current mainnet height recorded before disconnecting}"

[[ "$MONERO_RESTORE_HEIGHT" =~ ^[0-9]+$ && "$MONERO_RESTORE_HEIGHT" -gt 2000 ]] || {
  printf 'MONERO_RESTORE_HEIGHT must be a positive mainnet height.\n' >&2
  exit 1
}

require_pinned_binary() {
  local label=$1 path=$2 expected_hash=$3 actual_hash
  [[ "$expected_hash" =~ ^[0-9a-fA-F]{64}$ ]] || {
    printf '%s expected SHA-256 is malformed.\n' "$label" >&2
    exit 1
  }
  [[ -f "$path" && ! -L "$path" && -x "$path" ]] || {
    printf '%s must be an executable regular file, not a symlink: %s\n' "$label" "$path" >&2
    exit 1
  }
  actual_hash=$(sha256sum -- "$path" | awk '{print $1}')
  [[ "${actual_hash,,}" == "${expected_hash,,}" ]] || {
    printf '%s SHA-256 does not match the independently pinned value.\n' "$label" >&2
    exit 1
  }
}

require_pinned_binary bitcoind "$BITCOIND_BIN" "$BITCOIND_BIN_SHA256"
require_pinned_binary bitcoin-cli "$BITCOIN_CLI_BIN" "$BITCOIN_CLI_BIN_SHA256"
require_pinned_binary monero-wallet-cli "$MONERO_WALLET_CLI_BIN" "$MONERO_WALLET_CLI_BIN_SHA256"
assert_offline_state

require_new_directory() {
  local label=$1 path=$2 parent canonical_parent canonical_target
  [[ "$path" == /* ]] || {
    printf '%s must be an absolute path.\n' "$label" >&2
    exit 1
  }
  [[ ! -e "$path" && ! -L "$path" ]] || {
    printf '%s must not already exist: %s\n' "$label" "$path" >&2
    exit 1
  }
  parent=$(dirname -- "$path")
  [[ -d "$parent" && ! -L "$parent" ]] || {
    printf '%s parent must be an existing, non-symlink directory.\n' "$label" >&2
    exit 1
  }
  canonical_parent=$(readlink -f -- "$parent")
  canonical_target=$(readlink -m -- "$path")
  [[ "$canonical_target" == "$path" ]] || {
    printf '%s must be canonical and must not contain symlinked or dot components.\n' "$label" >&2
    exit 1
  }
  [[ "$canonical_parent" != /tmp && "$canonical_parent" != /tmp/* && \
     "$canonical_parent" != /run && "$canonical_parent" != /run/* ]] || {
    printf '%s must be on durable offline storage, not /tmp or /run.\n' "$label" >&2
    exit 1
  }
}

require_new_directory OFFLINE_BACKUP_DIR "$OFFLINE_BACKUP_DIR"
require_new_directory WATCH_ONLY_TRANSFER_DIR "$WATCH_ONLY_TRANSFER_DIR"
[[ "$OFFLINE_BACKUP_DIR" != "$WATCH_ONLY_TRANSFER_DIR" && \
   "$OFFLINE_BACKUP_DIR" != "$WATCH_ONLY_TRANSFER_DIR"/* && \
   "$WATCH_ONLY_TRANSFER_DIR" != "$OFFLINE_BACKUP_DIR"/* ]] || {
  printf 'Backup and transfer directories must be separate and non-nested.\n' >&2
  exit 1
}
BACKUP_PARENT=$(dirname -- "$OFFLINE_BACKUP_DIR")
TRANSFER_PARENT=$(dirname -- "$WATCH_ONLY_TRANSFER_DIR")
[[ $(stat -c '%d' "$BACKUP_PARENT") != $(stat -c '%d' "$TRANSFER_PARENT") ]] || {
  printf 'Backup and watch-only transfer directories must be on distinct filesystems.\n' >&2
  exit 1
}

[[ -d /dev/shm && $(stat -f -c '%T' /dev/shm) == tmpfs ]] || {
  printf 'A tmpfs-backed /dev/shm is required for volatile seed and passphrase handling.\n' >&2
  exit 1
}

SCRATCH_DIR=
BTC_DATA_DIR=
BTC_RPC_PORT=${OSL_OFFLINE_BTC_RPC_PORT:-38432}
[[ "$BTC_RPC_PORT" =~ ^[0-9]+$ && "$BTC_RPC_PORT" -ge 1024 && \
   "$BTC_RPC_PORT" -le 65535 ]] || {
  printf 'OSL_OFFLINE_BTC_RPC_PORT must be an unprivileged TCP port.\n' >&2
  exit 1
}
BTC_WALLET_NAME=osl-btc-merchant
BTC_BACKUP_FILE="$OFFLINE_BACKUP_DIR/bitcoin/osl-btc-merchant.wallet"
XMR_WALLET="$OFFLINE_BACKUP_DIR/monero/osl-xmr-merchant"
XMR_PASS_FILE=
XMR_CREATE_OUT=
XMR_SEED_OUT=
XMR_ADDRESS_OUT=
XMR_VIEW_OUT=
XMR_RESTORE_WALLET=
XMR_RESTORE_ADDRESS_OUT=
XMR_RESTORE_VIEW_OUT=
XMR_RESTORE_SEED_OUT=
XMR_VIEW_ONLY_TEST_WALLET=
XMR_VIEW_ONLY_TEST_CREATE_OUT=
XMR_VIEW_ONLY_TEST_ADDRESS_OUT=
BACKUP_INCOMPLETE_MARKER="$OFFLINE_BACKUP_DIR/CEREMONY-INCOMPLETE"
TRANSFER_INCOMPLETE_MARKER="$WATCH_ONLY_TRANSFER_DIR/CEREMONY-INCOMPLETE"
COMPLETE_RECEIPT_TMP="$OFFLINE_BACKUP_DIR/.CEREMONY-COMPLETE.tmp"
TRANSFER_COMPLETE_RECEIPT_TMP="$WATCH_ONLY_TRANSFER_DIR/.CEREMONY-COMPLETE.tmp"
BTC_STARTED=false
BTC_PASS=
BTC_PASS_CONFIRM=
XMR_PASS=
XMR_PASS_CONFIRM=

btc_cli() {
  "$BITCOIN_CLI_BIN" -datadir="$BTC_DATA_DIR" -rpcport="$BTC_RPC_PORT" "$@"
}

cleanup() {
  local status=$?
  set +e
  if [[ "$BTC_STARTED" == true ]]; then
    btc_cli stop >/dev/null 2>&1 || true
  fi
  unset BTC_PASS BTC_PASS_CONFIRM BTC_PRIVATE_DESCRIPTOR XMR_PASS XMR_PASS_CONFIRM \
    XMR_PRIVATE_VIEW_KEY XMR_MNEMONIC
  for sensitive_file in "$XMR_PASS_FILE" "$XMR_CREATE_OUT" "$XMR_SEED_OUT" \
    "$XMR_ADDRESS_OUT" "$XMR_VIEW_OUT" "$XMR_RESTORE_ADDRESS_OUT" \
    "$XMR_RESTORE_VIEW_OUT" "$XMR_RESTORE_SEED_OUT" \
    "$XMR_VIEW_ONLY_TEST_CREATE_OUT" \
    "$XMR_VIEW_ONLY_TEST_ADDRESS_OUT"; do
    if [[ -n "$sensitive_file" && -e "$sensitive_file" ]]; then
      shred -u -- "$sensitive_file" 2>/dev/null || true
    fi
  done
  if [[ -n "$SCRATCH_DIR" && "$SCRATCH_DIR" == /dev/shm/osl-offline-wallets.* ]]; then
    rm -rf -- "$SCRATCH_DIR"
  fi
  rm -f -- "$COMPLETE_RECEIPT_TMP"
  rm -f -- "$TRANSFER_COMPLETE_RECEIPT_TMP"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -m 0700 -- "$OFFLINE_BACKUP_DIR" "$WATCH_ONLY_TRANSFER_DIR"
mkdir -m 0700 -- "$OFFLINE_BACKUP_DIR/bitcoin" "$OFFLINE_BACKUP_DIR/monero"
printf '%s\n' 'Wallet ceremony incomplete. Do not import or fund this material.' \
  >"$BACKUP_INCOMPLETE_MARKER"
printf '%s\n' 'Wallet ceremony incomplete. Do not import or fund this material.' \
  >"$TRANSFER_INCOMPLETE_MARKER"
chmod 0600 "$BACKUP_INCOMPLETE_MARKER" "$TRANSFER_INCOMPLETE_MARKER"
SCRATCH_DIR=$(mktemp -d /dev/shm/osl-offline-wallets.XXXXXXXX)
chmod 0700 "$SCRATCH_DIR"
BTC_DATA_DIR="$SCRATCH_DIR/bitcoin-datadir"
XMR_PASS_FILE="$SCRATCH_DIR/xmr-password"
XMR_CREATE_OUT="$SCRATCH_DIR/xmr-create.out"
XMR_SEED_OUT="$SCRATCH_DIR/xmr-seed.out"
XMR_ADDRESS_OUT="$SCRATCH_DIR/xmr-address.out"
XMR_VIEW_OUT="$SCRATCH_DIR/xmr-view.out"
XMR_RESTORE_WALLET="$SCRATCH_DIR/xmr-restore-test"
XMR_RESTORE_ADDRESS_OUT="$SCRATCH_DIR/xmr-restore-address.out"
XMR_RESTORE_VIEW_OUT="$SCRATCH_DIR/xmr-restore-view.out"
XMR_RESTORE_SEED_OUT="$SCRATCH_DIR/xmr-restore-seed.out"
XMR_VIEW_ONLY_TEST_WALLET="$SCRATCH_DIR/xmr-view-only-test"
XMR_VIEW_ONLY_TEST_CREATE_OUT="$SCRATCH_DIR/xmr-view-only-create.out"
XMR_VIEW_ONLY_TEST_ADDRESS_OUT="$SCRATCH_DIR/xmr-view-only-address.out"

read_secret_twice() {
  local label=$1 first_name=$2 second_name=$3 first second
  printf '%s passphrase (minimum 16 characters): ' "$label" >/dev/tty
  IFS= read -rs first </dev/tty
  printf '\nRepeat %s passphrase: ' "$label" >/dev/tty
  IFS= read -rs second </dev/tty
  printf '\n' >/dev/tty
  [[ ${#first} -ge 16 && "$first" == "$second" ]] || {
    unset first second
    printf '%s passphrases did not match or were shorter than 16 characters.\n' "$label" >&2
    exit 1
  }
  printf -v "$first_name" '%s' "$first"
  printf -v "$second_name" '%s' "$second"
  unset first second
}

read_secret_twice Bitcoin BTC_PASS BTC_PASS_CONFIRM
read_secret_twice Monero XMR_PASS XMR_PASS_CONFIRM
unset BTC_PASS_CONFIRM XMR_PASS_CONFIRM

assert_offline_state
mkdir -m 0700 -- "$BTC_DATA_DIR"
"$BITCOIND_BIN" \
  -datadir="$BTC_DATA_DIR" -daemonwait -server=1 -listen=0 -dnsseed=0 \
  -discover=0 -upnp=0 -natpmp=0 -connect=0 -networkactive=0 -onlynet=ipv4 \
  -rpcbind=127.0.0.1 -rpcallowip=127.0.0.1 -rpcport="$BTC_RPC_PORT" \
  -fallbackfee=0.0002 -printtoconsole=0
BTC_STARTED=true

printf 'passphrase=%s\n' "$BTC_PASS" | btc_cli -rpcwait -stdin -named createwallet \
  wallet_name="$BTC_WALLET_NAME" disable_private_keys=false blank=false \
  avoid_reuse=false descriptors=true load_on_startup=true \
  external_signer=false >/dev/null

BTC_DESCRIPTOR_JSON=$(btc_cli -rpcwallet="$BTC_WALLET_NAME" listdescriptors false)
BTC_PUBLIC_DESCRIPTOR=$(jq -er \
  '[.descriptors[] | select(.active == true and .internal == false and (.desc | startswith("wpkh(")))] | if length == 1 then .[0].desc else error("expected one external wpkh descriptor") end' \
  <<<"$BTC_DESCRIPTOR_JSON")
[[ "$BTC_PUBLIC_DESCRIPTOR" == *'/*)#'* ]] || {
  printf 'Generated Bitcoin external descriptor is not ranged.\n' >&2
  exit 1
}
if grep -Eqi '(xprv|tprv|prv)' <<<"$BTC_PUBLIC_DESCRIPTOR"; then
  printf 'Refusing to export a Bitcoin private descriptor.\n' >&2
  exit 1
fi
BTC_DESCRIPTOR_INFO=$(printf '%s\n' "$BTC_PUBLIC_DESCRIPTOR" \
  | btc_cli -stdin getdescriptorinfo)
jq -e '.hasprivatekeys == false and .isrange == true and (.checksum | length > 0)' \
  >/dev/null <<<"$BTC_DESCRIPTOR_INFO"

btc_cli -rpcwallet="$BTC_WALLET_NAME" getwalletinfo \
  | jq -e '.private_keys_enabled == true and .descriptors == true and .unlocked_until == 0' \
    >/dev/null
btc_cli -rpcwallet="$BTC_WALLET_NAME" backupwallet "$BTC_BACKUP_FILE" >/dev/null
chmod 0600 "$BTC_BACKUP_FILE"

btc_cli -named restorewallet wallet_name=osl-btc-restore-test \
  backup_file="$BTC_BACKUP_FILE" load_on_startup=false >/dev/null
btc_cli -rpcwallet=osl-btc-restore-test getwalletinfo \
  | jq -e '.private_keys_enabled == true and .descriptors == true and .unlocked_until == 0' \
    >/dev/null
printf '%s\n' "$BTC_PASS" \
  | btc_cli -rpcwallet=osl-btc-restore-test -stdinwalletpassphrase \
    walletpassphrase 30 >/dev/null
BTC_PRIVATE_DESCRIPTOR_JSON=$(
  btc_cli -rpcwallet=osl-btc-restore-test listdescriptors true
)
BTC_PRIVATE_DESCRIPTOR=$(jq -er \
  '[.descriptors[] | select(.active == true and .internal == false and (.desc | startswith("wpkh(")))] | if length == 1 then .[0].desc else error("expected one restored private external wpkh descriptor") end' \
  <<<"$BTC_PRIVATE_DESCRIPTOR_JSON")
unset BTC_PRIVATE_DESCRIPTOR_JSON
[[ "$BTC_PRIVATE_DESCRIPTOR" == *xprv* && "$BTC_PRIVATE_DESCRIPTOR" == *'/*)#'* ]] || {
  printf 'Restored Bitcoin backup did not expose the expected private ranged descriptor.\n' >&2
  exit 1
}
printf '%s\n' "$BTC_PRIVATE_DESCRIPTOR" \
  | btc_cli -stdin getdescriptorinfo \
  | jq -e '.hasprivatekeys == true and .isrange == true and (.checksum | length > 0)' \
    >/dev/null
unset BTC_PRIVATE_DESCRIPTOR
btc_cli -rpcwallet=osl-btc-restore-test walletlock >/dev/null
BTC_RESTORED_DESCRIPTOR_JSON=$(btc_cli -rpcwallet=osl-btc-restore-test listdescriptors false)
BTC_RESTORED_PUBLIC_DESCRIPTOR=$(jq -er \
  '[.descriptors[] | select(.active == true and .internal == false and (.desc | startswith("wpkh(")))] | if length == 1 then .[0].desc else error("expected one restored external wpkh descriptor") end' \
  <<<"$BTC_RESTORED_DESCRIPTOR_JSON")
[[ "$BTC_RESTORED_PUBLIC_DESCRIPTOR" == "$BTC_PUBLIC_DESCRIPTOR" ]] || {
  printf 'Bitcoin backup restore test produced a different public descriptor.\n' >&2
  exit 1
}
btc_cli unloadwallet osl-btc-restore-test >/dev/null
unset BTC_PASS BTC_RESTORED_DESCRIPTOR_JSON BTC_RESTORED_PUBLIC_DESCRIPTOR

assert_offline_state
printf '%s\n' "$XMR_PASS" >"$XMR_PASS_FILE"
chmod 0600 "$XMR_PASS_FILE"
"$MONERO_WALLET_CLI_BIN" --generate-new-wallet "$XMR_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --mnemonic-language English \
  --restore-height "$MONERO_RESTORE_HEIGHT" --log-file /dev/null --command exit \
  >"$XMR_CREATE_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command seed \
  >"$XMR_SEED_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command address \
  >"$XMR_ADDRESS_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command viewkey \
  >"$XMR_VIEW_OUT" 2>&1
mapfile -t XMR_ADDRESS_MATCHES < <(
  sed -nE 's/^[[:space:]]*0[[:space:]]+(4[1-9A-HJ-NP-Za-km-z]{94})[[:space:]]+Primary address[[:space:]]*$/\1/p' \
    "$XMR_ADDRESS_OUT" | sort -u
)
mapfile -t XMR_VIEW_KEY_MATCHES < <(
  sed -nE 's/^[[:space:]]*secret:[[:space:]]*([0-9a-fA-F]{64})[[:space:]]*$/\1/p' \
    "$XMR_VIEW_OUT" | sort -u
)
mapfile -t XMR_MNEMONIC_MATCHES < <(
  awk 'NF == 25 { valid=1; for (i=1; i<=NF; i++) if ($i !~ /^[a-z]+$/) valid=0; if (valid) print }' \
    "$XMR_SEED_OUT" | sort -u
)
[[ ${#XMR_ADDRESS_MATCHES[@]} -eq 1 && ${#XMR_VIEW_KEY_MATCHES[@]} -eq 1 && \
   ${#XMR_MNEMONIC_MATCHES[@]} -eq 1 ]] || {
  printf 'Pinned Monero CLI output was ambiguous; refusing to select wallet material.\n' >&2
  exit 1
}
MONERO_PRIMARY_ADDRESS=${XMR_ADDRESS_MATCHES[0]}
XMR_PRIVATE_VIEW_KEY=${XMR_VIEW_KEY_MATCHES[0]}
XMR_MNEMONIC=${XMR_MNEMONIC_MATCHES[0]}
unset XMR_ADDRESS_MATCHES XMR_VIEW_KEY_MATCHES XMR_MNEMONIC_MATCHES
[[ "$MONERO_PRIMARY_ADDRESS" =~ ^4[1-9A-HJ-NP-Za-km-z]{94}$ ]] || {
  printf 'Could not verify the generated Monero primary address.\n' >&2
  exit 1
}
[[ "$XMR_PRIVATE_VIEW_KEY" =~ ^[0-9a-fA-F]{64}$ ]] || {
  printf 'Could not verify the generated Monero private view key.\n' >&2
  exit 1
}
[[ -f "$XMR_WALLET" && -f "$XMR_WALLET.keys" ]] || {
  printf 'Monero did not leave both encrypted wallet backup files.\n' >&2
  exit 1
}
chmod 0600 "$XMR_WALLET" "$XMR_WALLET.keys"

install -m 0600 "$XMR_WALLET" "$XMR_RESTORE_WALLET"
install -m 0600 "$XMR_WALLET.keys" "$XMR_RESTORE_WALLET.keys"
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_RESTORE_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command address \
  >"$XMR_RESTORE_ADDRESS_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_RESTORE_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command viewkey \
  >"$XMR_RESTORE_VIEW_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_RESTORE_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command seed \
  >"$XMR_RESTORE_SEED_OUT" 2>&1
mapfile -t XMR_RESTORE_ADDRESS_MATCHES < <(
  sed -nE 's/^[[:space:]]*0[[:space:]]+(4[1-9A-HJ-NP-Za-km-z]{94})[[:space:]]+Primary address[[:space:]]*$/\1/p' \
    "$XMR_RESTORE_ADDRESS_OUT" | sort -u
)
mapfile -t XMR_RESTORE_VIEW_MATCHES < <(
  sed -nE 's/^[[:space:]]*secret:[[:space:]]*([0-9a-fA-F]{64})[[:space:]]*$/\1/p' \
    "$XMR_RESTORE_VIEW_OUT" | sort -u
)
mapfile -t XMR_RESTORE_MNEMONIC_MATCHES < <(
  awk 'NF == 25 { valid=1; for (i=1; i<=NF; i++) if ($i !~ /^[a-z]+$/) valid=0; if (valid) print }' \
    "$XMR_RESTORE_SEED_OUT" | sort -u
)
[[ ${#XMR_RESTORE_ADDRESS_MATCHES[@]} -eq 1 && \
   ${#XMR_RESTORE_VIEW_MATCHES[@]} -eq 1 && \
   ${#XMR_RESTORE_MNEMONIC_MATCHES[@]} -eq 1 && \
   ${XMR_RESTORE_ADDRESS_MATCHES[0]} == "$MONERO_PRIMARY_ADDRESS" && \
   ${XMR_RESTORE_VIEW_MATCHES[0]} == "$XMR_PRIVATE_VIEW_KEY" && \
   ${XMR_RESTORE_MNEMONIC_MATCHES[0]} == "$XMR_MNEMONIC" ]] || {
  printf 'Monero backup restore test did not reproduce the generated view material.\n' >&2
  exit 1
}
unset XMR_RESTORE_ADDRESS_MATCHES XMR_RESTORE_VIEW_MATCHES \
  XMR_RESTORE_MNEMONIC_MATCHES

printf '%s\n%s\n' "$MONERO_PRIMARY_ADDRESS" "$XMR_PRIVATE_VIEW_KEY" \
  | "$MONERO_WALLET_CLI_BIN" --generate-from-view-key "$XMR_VIEW_ONLY_TEST_WALLET" \
    --password-file "$XMR_PASS_FILE" --offline \
    --restore-height "$MONERO_RESTORE_HEIGHT" --log-file /dev/null --command exit \
    >"$XMR_VIEW_ONLY_TEST_CREATE_OUT" 2>&1
"$MONERO_WALLET_CLI_BIN" --wallet-file "$XMR_VIEW_ONLY_TEST_WALLET" \
  --password-file "$XMR_PASS_FILE" --offline --log-file /dev/null --command address \
  >"$XMR_VIEW_ONLY_TEST_ADDRESS_OUT" 2>&1
mapfile -t XMR_VIEW_ONLY_ADDRESS_MATCHES < <(
  sed -nE 's/^[[:space:]]*0[[:space:]]+(4[1-9A-HJ-NP-Za-km-z]{94})[[:space:]]+Primary address[[:space:]]*$/\1/p' \
    "$XMR_VIEW_ONLY_TEST_ADDRESS_OUT" | sort -u
)
[[ ${#XMR_VIEW_ONLY_ADDRESS_MATCHES[@]} -eq 1 && \
   ${XMR_VIEW_ONLY_ADDRESS_MATCHES[0]} == "$MONERO_PRIMARY_ADDRESS" ]] || {
  printf 'Monero view key did not reproduce the generated primary address.\n' >&2
  exit 1
}
unset XMR_VIEW_ONLY_ADDRESS_MATCHES XMR_PASS

assert_offline_state
printf '\nMonero recovery seed transcript follows. Keep cameras and networked devices away.\n' >/dev/tty
printf '%s\n' '----- BEGIN OFFLINE MONERO RECOVERY TRANSCRIPT -----' >/dev/tty
printf '%s\n' "$XMR_MNEMONIC" >/dev/tty
printf '%s\n' '----- END OFFLINE MONERO RECOVERY TRANSCRIPT -----' >/dev/tty
require_acknowledgement \
  'I RECORDED THE MONERO SEED OFFLINE' \
  'Write the seed onto durable offline media and verify every word twice.'
unset XMR_MNEMONIC
shred -u -- "$XMR_CREATE_OUT" "$XMR_SEED_OUT" "$XMR_RESTORE_SEED_OUT"

require_acknowledgement \
  'I STORED BOTH WALLET PASSPHRASES OFFLINE' \
  'Store both passphrases separately from the encrypted wallet backup files.'
shred -u -- "$XMR_PASS_FILE"

(
  cd "$OFFLINE_BACKUP_DIR"
  sha256sum bitcoin/osl-btc-merchant.wallet monero/osl-xmr-merchant \
    monero/osl-xmr-merchant.keys >BACKUP-SHA256SUMS
  chmod 0600 BACKUP-SHA256SUMS
)

sync -f "$OFFLINE_BACKUP_DIR"
sync -f "$WATCH_ONLY_TRANSFER_DIR"
assert_offline_state

require_acknowledgement \
  'I COPIED BOTH ENCRYPTED WALLET BACKUPS TO DURABLE OFFLINE MEDIA' \
  'Verify BACKUP-SHA256SUMS on at least one second, physically separate offline copy.'

# BEGIN TRANSFER EXPORTS -- watch/view material only, after backup confirmation.
printf '%s\n' "$BTC_PUBLIC_DESCRIPTOR" \
  | install -m 0600 /dev/stdin "$WATCH_ONLY_TRANSFER_DIR/btc-descriptor"
printf '%s\n' "$MONERO_PRIMARY_ADDRESS" \
  | install -m 0600 /dev/stdin "$WATCH_ONLY_TRANSFER_DIR/xmr-address"
printf '%s\n' "$XMR_PRIVATE_VIEW_KEY" \
  | install -m 0600 /dev/stdin "$WATCH_ONLY_TRANSFER_DIR/xmr-view-key"
printf '%s\n' "$MONERO_RESTORE_HEIGHT" \
  | install -m 0600 /dev/stdin "$WATCH_ONLY_TRANSFER_DIR/xmr-restore-height"
(
  cd "$WATCH_ONLY_TRANSFER_DIR"
  sha256sum btc-descriptor xmr-address xmr-view-key xmr-restore-height \
    >SHA256SUMS
  chmod 0600 SHA256SUMS
)
# END TRANSFER EXPORTS.

unset BTC_PUBLIC_DESCRIPTOR MONERO_PRIMARY_ADDRESS XMR_PRIVATE_VIEW_KEY
BACKUP_MANIFEST_SHA256=$(sha256sum "$OFFLINE_BACKUP_DIR/BACKUP-SHA256SUMS" | awk '{print $1}')
TRANSFER_MANIFEST_SHA256=$(sha256sum "$WATCH_ONLY_TRANSFER_DIR/SHA256SUMS" | awk '{print $1}')
for receipt_tmp in "$COMPLETE_RECEIPT_TMP" "$TRANSFER_COMPLETE_RECEIPT_TMP"; do
  printf 'ceremony_version=1\nbackup_manifest_sha256=%s\ntransfer_manifest_sha256=%s\n' \
    "$BACKUP_MANIFEST_SHA256" "$TRANSFER_MANIFEST_SHA256" >"$receipt_tmp"
  chmod 0600 "$receipt_tmp"
  sync -f "$receipt_tmp"
done
unset BACKUP_MANIFEST_SHA256 TRANSFER_MANIFEST_SHA256
sync -f "$COMPLETE_RECEIPT_TMP"
mv -- "$COMPLETE_RECEIPT_TMP" "$OFFLINE_BACKUP_DIR/CEREMONY-COMPLETE"
mv -- "$TRANSFER_COMPLETE_RECEIPT_TMP" "$WATCH_ONLY_TRANSFER_DIR/CEREMONY-COMPLETE"
sync -f "$OFFLINE_BACKUP_DIR"
sync -f "$WATCH_ONLY_TRANSFER_DIR"
rm -f -- "$BACKUP_INCOMPLETE_MARKER"
rm -f -- "$TRANSFER_INCOMPLETE_MARKER"
sync -f "$OFFLINE_BACKUP_DIR"
sync -f "$WATCH_ONLY_TRANSFER_DIR"

printf '%s\n' \
  'Offline merchant wallets created.' \
  "Encrypted offline backups: $OFFLINE_BACKUP_DIR" \
  "Watch/view-only transfer bundle: $WATCH_ONLY_TRANSFER_DIR" \
  'Do not connect this machine to a network. Power it down and store it securely.'
