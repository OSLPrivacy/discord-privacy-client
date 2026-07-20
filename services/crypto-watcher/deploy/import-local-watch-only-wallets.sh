#!/usr/bin/env bash
set -euo pipefail
umask 077

# Import a completed offline ceremony's public Bitcoin descriptor and Monero
# view material into the current user's local OSL node installation. This
# script never accepts a seed, Bitcoin private descriptor, or Monero spend key.

if [[ $- == *x* ]]; then
  set +x
  printf 'Refusing to handle view material while shell tracing is enabled.\n' >&2
  exit 1
fi
set +x

: "${BTC_PUBLIC_DESCRIPTOR_FILE:?Set BTC_PUBLIC_DESCRIPTOR_FILE}"
: "${BTC_DESCRIPTOR_IS_DEDICATED_UNUSED:?Confirm the BTC descriptor is dedicated and unused}"
: "${MONERO_PRIMARY_ADDRESS_FILE:?Set MONERO_PRIMARY_ADDRESS_FILE}"
: "${MONERO_PRIVATE_VIEW_KEY_FILE:?Set MONERO_PRIVATE_VIEW_KEY_FILE}"
: "${OFFLINE_CEREMONY_RECEIPT_FILE:?Set OFFLINE_CEREMONY_RECEIPT_FILE}"
: "${OFFLINE_CEREMONY_SHA256SUMS_FILE:?Set OFFLINE_CEREMONY_SHA256SUMS_FILE}"
: "${MONERO_RESTORE_HEIGHT:?Set MONERO_RESTORE_HEIGHT}"

[[ "$BTC_DESCRIPTOR_IS_DEDICATED_UNUSED" == true ]] || {
  printf 'Set BTC_DESCRIPTOR_IS_DEDICATED_UNUSED=true only for a dedicated, never-used descriptor.\n' >&2
  exit 1
}
[[ "$MONERO_RESTORE_HEIGHT" =~ ^[0-9]+$ && "$MONERO_RESTORE_HEIGHT" -gt 2000 ]] || {
  printf 'MONERO_RESTORE_HEIGHT must be a positive mainnet height.\n' >&2
  exit 1
}

BTC_CLI="$HOME/.local/opt/osl-crypto/bin/bitcoin-cli"
MONERO_WALLET_CLI="$HOME/.local/opt/osl-crypto/bin/monero-wallet-cli"
BTC_DATADIR="$HOME/.local/share/osl-crypto/bitcoin"
BTC_COOKIE="$BTC_DATADIR/.cookie"
BTC_WALLET=osl-watch
BTC_BACKUP_DIR="$BTC_DATADIR/watch-wallet-backups"
XMR_WALLET_DIR="$HOME/.local/share/osl-crypto/wallets"
XMR_WALLET="$XMR_WALLET_DIR/osl-view-only"
XMR_KEYS="$XMR_WALLET.keys"
XMR_BACKUP_DIR="$HOME/.local/share/osl-crypto/watch-wallet-backups"
XMR_PASSWORD_FILE="$HOME/.config/osl-crypto/secrets/monero-wallet-password"
IMPORT_DIR="$HOME/.config/osl-crypto"
IMPORT_RECEIPT="$IMPORT_DIR/local-watch-only-import.receipt"
LOCK_FILE="$IMPORT_DIR/local-watch-only-import.lock"
MONERO_UNIT=osl-monero-wallet-rpc.service

for command in awk basename chmod curl date dirname flock grep id install jq mkdir \
  mktemp mv readlink rm sed seq sha256sum sleep sort stat systemctl tr; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done
for path in "$BTC_CLI" "$MONERO_WALLET_CLI"; do
  [[ -x "$path" && ! -L "$path" ]] || {
    printf 'Required wallet executable is missing, non-executable, or symlinked: %s\n' "$path" >&2
    exit 1
  }
done

require_mode_0600_file() {
  local path=$1
  [[ -f "$path" && ! -L "$path" && $(stat -c '%a' "$path") == 600 ]] || {
    printf 'Input must be a regular, non-symlink mode-0600 file: %s\n' "$path" >&2
    exit 1
  }
  [[ $(stat -c '%u' "$path") == "$(id -u)" && $(stat -c '%s' "$path") -gt 0 && \
     $(stat -c '%s' "$path") -le 16384 ]] || {
    printf 'Input has unsafe ownership or size: %s\n' "$path" >&2
    exit 1
  }
}

require_private_user_dir() {
  local path=$1
  [[ -d "$path" && ! -L "$path" && $(stat -c '%u' "$path") == "$(id -u)" && \
     $(stat -c '%a' "$path") == 700 ]] || {
    printf 'Directory must be owned by the current user, non-symlink, and mode 0700: %s\n' \
      "$path" >&2
    exit 1
  }
}

create_private_user_dir() {
  local path=$1
  [[ ! -L "$path" && ( ! -e "$path" || -d "$path" ) ]] || {
    printf 'Refusing unsafe directory path: %s\n' "$path" >&2
    exit 1
  }
  mkdir -p -- "$path"
  chmod 0700 "$path"
  require_private_user_dir "$path"
}

for path in "$BTC_PUBLIC_DESCRIPTOR_FILE" "$MONERO_PRIMARY_ADDRESS_FILE" \
  "$MONERO_PRIVATE_VIEW_KEY_FILE" "$OFFLINE_CEREMONY_RECEIPT_FILE" \
  "$OFFLINE_CEREMONY_SHA256SUMS_FILE"; do
  require_mode_0600_file "$path"
done

TRANSFER_DIR=$(readlink -f -- "$(dirname -- "$OFFLINE_CEREMONY_RECEIPT_FILE")")
[[ -d "$TRANSFER_DIR" && ! -L "$TRANSFER_DIR" ]] || {
  printf 'Offline ceremony transfer directory is invalid.\n' >&2
  exit 1
}
[[ ! -e "$TRANSFER_DIR/CEREMONY-INCOMPLETE" && ! -L "$TRANSFER_DIR/CEREMONY-INCOMPLETE" ]] || {
  printf 'Offline ceremony is explicitly marked incomplete.\n' >&2
  exit 1
}
declare -A EXPECTED_BASENAMES=(
  ["$BTC_PUBLIC_DESCRIPTOR_FILE"]=btc-descriptor
  ["$MONERO_PRIMARY_ADDRESS_FILE"]=xmr-address
  ["$MONERO_PRIVATE_VIEW_KEY_FILE"]=xmr-view-key
  ["$OFFLINE_CEREMONY_RECEIPT_FILE"]=CEREMONY-COMPLETE
  ["$OFFLINE_CEREMONY_SHA256SUMS_FILE"]=SHA256SUMS
)
for path in "${!EXPECTED_BASENAMES[@]}"; do
  [[ $(readlink -f -- "$(dirname -- "$path")") == "$TRANSFER_DIR" && \
     $(basename -- "$path") == "${EXPECTED_BASENAMES[$path]}" ]] || {
    printf 'Ceremony input has the wrong directory or filename: %s\n' "$path" >&2
    exit 1
  }
done
MONERO_RESTORE_HEIGHT_FILE="$TRANSFER_DIR/xmr-restore-height"
require_mode_0600_file "$MONERO_RESTORE_HEIGHT_FILE"

mapfile -t RECEIPT_LINES <"$OFFLINE_CEREMONY_RECEIPT_FILE"
[[ ${#RECEIPT_LINES[@]} -eq 3 && \
   ${RECEIPT_LINES[0]} == ceremony_version=1 && \
   ${RECEIPT_LINES[1]} =~ ^backup_manifest_sha256=([0-9a-f]{64})$ && \
   ${RECEIPT_LINES[2]} =~ ^transfer_manifest_sha256=([0-9a-f]{64})$ ]] || {
  printf 'Offline ceremony completion receipt has an invalid schema.\n' >&2
  exit 1
}
RECEIPT_TRANSFER_MANIFEST_SHA256=${RECEIPT_LINES[2]#transfer_manifest_sha256=}
unset RECEIPT_LINES

mapfile -t MANIFEST_NAMES < <(
  awk 'NF == 2 && $1 ~ /^[0-9a-f]{64}$/ { print $2 }' \
    "$OFFLINE_CEREMONY_SHA256SUMS_FILE" | sort
)
EXPECTED_MANIFEST_NAMES=(btc-descriptor xmr-address xmr-restore-height xmr-view-key)
[[ $(awk 'END { print NR }' "$OFFLINE_CEREMONY_SHA256SUMS_FILE") -eq 4 && \
   ${#MANIFEST_NAMES[@]} -eq 4 ]] || {
  printf 'Offline ceremony transfer manifest must contain exactly four hashes.\n' >&2
  exit 1
}
for index in "${!EXPECTED_MANIFEST_NAMES[@]}"; do
  [[ ${MANIFEST_NAMES[$index]} == "${EXPECTED_MANIFEST_NAMES[$index]}" ]] || {
    printf 'Offline ceremony transfer manifest contains an unexpected filename.\n' >&2
    exit 1
  }
done
unset MANIFEST_NAMES EXPECTED_MANIFEST_NAMES
(
  cd "$TRANSFER_DIR"
  sha256sum --strict -c SHA256SUMS >/dev/null
) || {
  printf 'Offline ceremony transfer manifest verification failed.\n' >&2
  exit 1
}
ACTUAL_TRANSFER_MANIFEST_SHA256=$(sha256sum "$OFFLINE_CEREMONY_SHA256SUMS_FILE" | awk '{print $1}')
[[ "$ACTUAL_TRANSFER_MANIFEST_SHA256" == "$RECEIPT_TRANSFER_MANIFEST_SHA256" ]] || {
  printf 'Offline ceremony completion receipt does not bind this transfer manifest.\n' >&2
  exit 1
}
BUNDLED_MONERO_RESTORE_HEIGHT=$(tr -d '\r\n' <"$MONERO_RESTORE_HEIGHT_FILE")
[[ "$BUNDLED_MONERO_RESTORE_HEIGHT" == "$MONERO_RESTORE_HEIGHT" ]] || {
  printf 'MONERO_RESTORE_HEIGHT does not match the completed offline ceremony.\n' >&2
  exit 1
}
unset BUNDLED_MONERO_RESTORE_HEIGHT

BTC_DESCRIPTOR=$(tr -d '\r\n' <"$BTC_PUBLIC_DESCRIPTOR_FILE")
MONERO_ADDRESS=$(tr -d '\r\n' <"$MONERO_PRIMARY_ADDRESS_FILE")
MONERO_VIEW_KEY=$(tr -d '\r\n' <"$MONERO_PRIVATE_VIEW_KEY_FILE")
cleanup() {
  unset BTC_DESCRIPTOR MONERO_ADDRESS MONERO_VIEW_KEY
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

[[ "$BTC_DESCRIPTOR" == wpkh\(* && "$BTC_DESCRIPTOR" == *'#'* && \
   ( "$BTC_DESCRIPTOR" == *'/*)'* || "$BTC_DESCRIPTOR" == *'/*)#'* ) ]] || {
  printf 'Bitcoin input must be a checksummed external ranged wpkh descriptor.\n' >&2
  exit 1
}
if grep -Eqi '(xprv|tprv|prv)' <<<"$BTC_DESCRIPTOR"; then
  printf 'Refusing Bitcoin private descriptor material.\n' >&2
  exit 1
fi
[[ "$MONERO_ADDRESS" =~ ^4[1-9A-HJ-NP-Za-km-z]{94}$ ]] || {
  printf 'Monero primary address is malformed or not mainnet.\n' >&2
  exit 1
}
[[ "$MONERO_VIEW_KEY" =~ ^[0-9a-fA-F]{64}$ ]] || {
  printf 'Monero private view key is malformed.\n' >&2
  exit 1
}
require_mode_0600_file "$XMR_PASSWORD_FILE"
require_private_user_dir "$BTC_DATADIR"

create_private_user_dir "$IMPORT_DIR"
if [[ ! -e "$LOCK_FILE" ]]; then install -m 0600 /dev/null "$LOCK_FILE"; fi
[[ -f "$LOCK_FILE" && ! -L "$LOCK_FILE" && $(stat -c '%a' "$LOCK_FILE") == 600 ]] || {
  printf 'Local import lock has unsafe type or permissions.\n' >&2
  exit 1
}
exec 9<>"$LOCK_FILE"
flock -n 9 || {
  printf 'Another local watch-only import is active.\n' >&2
  exit 1
}

BTC_DESCRIPTOR_SHA256=$(printf '%s' "$BTC_DESCRIPTOR" | sha256sum | awk '{print $1}')
XMR_VIEW_MATERIAL_SHA256=$(
  printf '%s\n%s\n%s\n' "$MONERO_ADDRESS" "$MONERO_VIEW_KEY" "$MONERO_RESTORE_HEIGHT" \
    | sha256sum | awk '{print $1}'
)

# Read-only preflight: nodes must be completely synchronized before mutation.
btc=$(
  "$BTC_CLI" -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" getblockchaininfo
)
jq -e '.chain == "main" and .initialblockdownload == false and .blocks == .headers' \
  >/dev/null <<<"$btc" || {
  printf 'Bitcoin Core is not fully synchronized on mainnet.\n' >&2
  exit 1
}
wallet=$(
  "$BTC_CLI" -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" \
    -rpcwallet="$BTC_WALLET" getwalletinfo
) || {
  printf 'The local osl-watch Bitcoin wallet must exist and be loaded.\n' >&2
  exit 1
}
jq -e '.private_keys_enabled == false and .descriptors == true and .txcount == 0' \
  >/dev/null <<<"$wallet" || {
  printf 'The local Bitcoin wallet is not an empty descriptor-only watch wallet.\n' >&2
  exit 1
}
descriptor_info=$(printf '%s\n' "$BTC_DESCRIPTOR" | "$BTC_CLI" -stdin \
  -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" getdescriptorinfo)
jq -e '.hasprivatekeys == false and .isrange == true' >/dev/null <<<"$descriptor_info" || {
  printf 'Bitcoin Core rejected the descriptor as public and ranged.\n' >&2
  exit 1
}
descriptors=$(
  "$BTC_CLI" -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" \
    -rpcwallet="$BTC_WALLET" listdescriptors false
)
descriptor_count=$(jq '.descriptors | length' <<<"$descriptors")
if [[ "$descriptor_count" -eq 0 ]]; then
  BTC_IMPORT_NEEDED=true
elif [[ "$descriptor_count" -eq 1 ]] && jq -e --arg expected "$BTC_DESCRIPTOR" \
  '.descriptors[0].desc == $expected and .descriptors[0].active == true and
   .descriptors[0].internal == false and .descriptors[0].range == [0, 99999] and
   (.descriptors[0].next | type == "number" and . >= 0 and . <= 100000)' \
  >/dev/null <<<"$descriptors"; then
  BTC_IMPORT_NEEDED=false
else
  printf 'Bitcoin watch wallet contains a different or unexpected descriptor.\n' >&2
  exit 1
fi

xmr=$(curl -fsS --max-time 5 http://127.0.0.1:18081/get_info)
jq -e '.nettype == "mainnet" and .synchronized == true and
  ((.target_height == 0) or (.height >= .target_height))' >/dev/null <<<"$xmr" || {
  printf 'Monero is not fully synchronized on mainnet.\n' >&2
  exit 1
}
xmr_height=$(jq -r '.height' <<<"$xmr")
[[ "$MONERO_RESTORE_HEIGHT" -le "$xmr_height" ]] || {
  printf 'Monero restore height is ahead of the synchronized daemon.\n' >&2
  exit 1
}

for path in "$XMR_WALLET" "$XMR_KEYS" "$IMPORT_RECEIPT"; do
  [[ ! -L "$path" ]] || {
    printf 'Refusing symlinked local wallet artifact: %s\n' "$path" >&2
    exit 1
  }
done
wallet_exists=false; keys_exist=false
[[ ! -e "$XMR_WALLET" ]] || wallet_exists=true
[[ ! -e "$XMR_KEYS" ]] || keys_exist=true
if [[ "$wallet_exists" == false && "$keys_exist" == false && ! -e "$IMPORT_RECEIPT" ]]; then
  XMR_CREATE_NEEDED=true
elif [[ "$wallet_exists" == true && "$keys_exist" == true && \
        -f "$XMR_WALLET" && -f "$XMR_KEYS" && -f "$IMPORT_RECEIPT" && \
        $(stat -c '%a' "$IMPORT_RECEIPT") == 600 ]]; then
  mapfile -t LOCAL_RECEIPT_LINES <"$IMPORT_RECEIPT"
  [[ ${#LOCAL_RECEIPT_LINES[@]} -eq 8 && \
     ${LOCAL_RECEIPT_LINES[0]} == import_version=1 && \
     ${LOCAL_RECEIPT_LINES[1]} == method=local-watch-only && \
     ${LOCAL_RECEIPT_LINES[2]} =~ ^ceremony_transfer_manifest_sha256=([0-9a-f]{64})$ && \
     ${LOCAL_RECEIPT_LINES[3]} =~ ^btc_descriptor_sha256=([0-9a-f]{64})$ && \
     ${LOCAL_RECEIPT_LINES[4]} =~ ^xmr_view_material_sha256=([0-9a-f]{64})$ && \
     ${LOCAL_RECEIPT_LINES[5]} =~ ^monero_primary_address=(4[1-9A-HJ-NP-Za-km-z]{94})$ && \
     ${LOCAL_RECEIPT_LINES[6]} =~ ^monero_restore_height=([0-9]+)$ && \
     ${LOCAL_RECEIPT_LINES[7]} =~ ^imported_at=([0-9]{8}T[0-9]{6}Z-[0-9]+)$ ]] || {
    printf 'Existing local wallet provenance receipt has an invalid schema.\n' >&2
    exit 1
  }
  receipt_transfer=${LOCAL_RECEIPT_LINES[2]#ceremony_transfer_manifest_sha256=}
  receipt_btc_hash=${LOCAL_RECEIPT_LINES[3]#btc_descriptor_sha256=}
  receipt_xmr_hash=${LOCAL_RECEIPT_LINES[4]#xmr_view_material_sha256=}
  receipt_address=${LOCAL_RECEIPT_LINES[5]#monero_primary_address=}
  receipt_height=${LOCAL_RECEIPT_LINES[6]#monero_restore_height=}
  [[ "$receipt_transfer" == "$ACTUAL_TRANSFER_MANIFEST_SHA256" && \
     "$receipt_xmr_hash" == "$XMR_VIEW_MATERIAL_SHA256" && \
     "$receipt_btc_hash" == "$BTC_DESCRIPTOR_SHA256" && \
     "$receipt_address" == "$MONERO_ADDRESS" && \
     "$receipt_height" == "$MONERO_RESTORE_HEIGHT" ]] || {
    printf 'Existing local wallet provenance does not match the supplied ceremony bundle.\n' >&2
    exit 1
  }
  unset LOCAL_RECEIPT_LINES
  XMR_CREATE_NEEDED=false
else
  printf 'Monero wallet state is partial or lacks a matching provenance receipt.\n' >&2
  exit 1
fi

MONERO_ACTIVE_BEFORE=false
systemctl --user is-active --quiet "$MONERO_UNIT" && MONERO_ACTIVE_BEFORE=true
XMR_CREATED=false
RECEIPT_CREATED=false
XMR_BACKUP_WALLET=
XMR_BACKUP_KEYS=
restore_on_failure() {
  local status=$?
  set +e
  if [[ "$status" -ne 0 ]]; then
    systemctl --user stop "$MONERO_UNIT" >/dev/null 2>&1 || true
    if [[ "$XMR_CREATED" == true ]]; then
      rm -f -- "$XMR_WALLET" "$XMR_KEYS"
      [[ -z "$XMR_BACKUP_WALLET" ]] || rm -f -- "$XMR_BACKUP_WALLET"
      [[ -z "$XMR_BACKUP_KEYS" ]] || rm -f -- "$XMR_BACKUP_KEYS"
    fi
    if [[ "$RECEIPT_CREATED" == true ]]; then rm -f -- "$IMPORT_RECEIPT"; fi
    if [[ "$MONERO_ACTIVE_BEFORE" == true ]]; then
      systemctl --user start "$MONERO_UNIT" >/dev/null 2>&1 || true
    fi
  fi
  exit "$status"
}
trap restore_on_failure EXIT

systemctl --user stop "$MONERO_UNIT"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)-$$
if [[ "$BTC_IMPORT_NEEDED" == true ]]; then
  payload=$(jq -cn --arg descriptor "$BTC_DESCRIPTOR" \
    '[{desc:$descriptor,timestamp:"now",active:true,internal:false,range:[0,99999],next_index:0}]')
  result=$(printf '%s\n' "$payload" | "$BTC_CLI" -stdin -datadir="$BTC_DATADIR" \
    -rpccookiefile="$BTC_COOKIE" -rpcwallet="$BTC_WALLET" importdescriptors)
  jq -e 'length == 1 and .[0].success == true' >/dev/null <<<"$result"
fi
post_descriptors=$(
  "$BTC_CLI" -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" \
    -rpcwallet="$BTC_WALLET" listdescriptors false
)
jq -e --arg expected "$BTC_DESCRIPTOR" \
  '.descriptors | length == 1 and .[0].desc == $expected and
   .[0].active == true and .[0].internal == false and
   .[0].range == [0, 99999] and
   (.[0].next | type == "number" and . >= 0 and . <= 100000)' \
  >/dev/null <<<"$post_descriptors"
create_private_user_dir "$BTC_BACKUP_DIR"
BTC_BACKUP="$BTC_BACKUP_DIR/osl-watch-$STAMP.dat"
"$BTC_CLI" -datadir="$BTC_DATADIR" -rpccookiefile="$BTC_COOKIE" \
  -rpcwallet="$BTC_WALLET" backupwallet "$BTC_BACKUP" >/dev/null
chmod 0600 "$BTC_BACKUP"

if [[ "$XMR_CREATE_NEEDED" == true ]]; then
  create_private_user_dir "$XMR_WALLET_DIR"
  create_private_user_dir "$XMR_BACKUP_DIR"
  [[ ! -e "$XMR_WALLET" && ! -e "$XMR_KEYS" ]] || {
    printf 'Monero wallet path changed during import.\n' >&2
    exit 1
  }
  XMR_CREATED=true
  printf '%s\n%s\n' "$MONERO_ADDRESS" "$MONERO_VIEW_KEY" | "$MONERO_WALLET_CLI" \
    --generate-from-view-key "$XMR_WALLET" --password-file "$XMR_PASSWORD_FILE" \
    --restore-height "$MONERO_RESTORE_HEIGHT" --daemon-address 127.0.0.1:18081 \
    --log-file /dev/null --log-level 0 >/dev/null
  [[ -f "$XMR_WALLET" && ! -L "$XMR_WALLET" && -f "$XMR_KEYS" && ! -L "$XMR_KEYS" ]]
  chmod 0600 "$XMR_WALLET" "$XMR_KEYS"
  XMR_BACKUP_WALLET="$XMR_BACKUP_DIR/osl-view-only-$STAMP.wallet"
  XMR_BACKUP_KEYS="$XMR_BACKUP_DIR/osl-view-only-$STAMP.wallet.keys"
  install -m 0600 "$XMR_WALLET" "$XMR_BACKUP_WALLET"
  install -m 0600 "$XMR_KEYS" "$XMR_BACKUP_KEYS"
fi

if [[ "$XMR_CREATE_NEEDED" == true ]]; then
  receipt_tmp=$(mktemp "$IMPORT_DIR/.local-watch-only-import.XXXXXX")
  printf 'import_version=1\nmethod=local-watch-only\nceremony_transfer_manifest_sha256=%s\nbtc_descriptor_sha256=%s\nxmr_view_material_sha256=%s\nmonero_primary_address=%s\nmonero_restore_height=%s\nimported_at=%s\n' \
    "$ACTUAL_TRANSFER_MANIFEST_SHA256" "$BTC_DESCRIPTOR_SHA256" \
    "$XMR_VIEW_MATERIAL_SHA256" "$MONERO_ADDRESS" "$MONERO_RESTORE_HEIGHT" "$STAMP" \
    >"$receipt_tmp"
  chmod 0600 "$receipt_tmp"
  RECEIPT_CREATED=true
  mv -T "$receipt_tmp" "$IMPORT_RECEIPT"
fi

systemctl --user start "$MONERO_UNIT"
address_verified=false
for _attempt in $(seq 1 60); do
  response=$(curl -fsS --max-time 2 http://127.0.0.1:18088/json_rpc \
    -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","id":"osl","method":"get_address","params":{"account_index":0}}') || true
  if jq -e --arg expected "$MONERO_ADDRESS" '.result.address == $expected' \
    >/dev/null <<<"$response"; then
    address_verified=true
    break
  fi
  sleep 1
done
[[ "$address_verified" == true ]] || {
  printf 'Monero Wallet RPC did not return the pinned primary address.\n' >&2
  exit 1
}
if [[ "$MONERO_ACTIVE_BEFORE" == false ]]; then systemctl --user stop "$MONERO_UNIT"; fi

unset BTC_DESCRIPTOR MONERO_ADDRESS MONERO_VIEW_KEY
trap cleanup EXIT
printf '%s\n' \
  'Local public Bitcoin descriptor and Monero view material were imported.' \
  'The exact completed ceremony manifest is bound in the local provenance receipt.' \
  'No spending key, recovery seed, or private Bitcoin descriptor was accepted.'
