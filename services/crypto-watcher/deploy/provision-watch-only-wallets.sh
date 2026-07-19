#!/usr/bin/env bash
set -euo pipefail
umask 077

# Imports only public Bitcoin descriptor material and Monero view material.
# It never creates, accepts, or transfers a Bitcoin xprv, Monero spend key, or
# recovery seed. Spending wallets must be generated and backed up offline.

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
: "${OSL_PAYMENTS_VPS:?Set OSL_PAYMENTS_VPS to user@host}"
: "${OSL_PAYMENTS_HOST_KEY_SHA256:?Set OSL_PAYMENTS_HOST_KEY_SHA256}"

SSH_KEY=${OSL_PAYMENTS_SSH_KEY:-"$HOME/.ssh/osl_payments_contabo_ed25519"}
VPS=$OSL_PAYMENTS_VPS
if [[ "$VPS" =~ ^([a-z_][a-z0-9_-]*)@([A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?)$ ]]; then
  VPS_USER=${BASH_REMATCH[1]}
  VPS_HOST=${BASH_REMATCH[2]}
else
  printf 'OSL_PAYMENTS_VPS must be a plain user@hostname or user@IPv4 target.\n' >&2
  exit 1
fi
[[ "$VPS_USER" != -* && "$VPS_HOST" != -* && "$VPS_HOST" != *..* ]] || {
  printf 'OSL_PAYMENTS_VPS contains an invalid user or host.\n' >&2
  exit 1
}
[[ "$OSL_PAYMENTS_HOST_KEY_SHA256" =~ ^SHA256:[A-Za-z0-9+/]{43}=?$ ]] || {
  printf 'Invalid pinned SSH host-key fingerprint.\n' >&2
  exit 1
}
[[ "$MONERO_RESTORE_HEIGHT" =~ ^[0-9]+$ && "$MONERO_RESTORE_HEIGHT" -gt 2000 ]] || {
  printf 'MONERO_RESTORE_HEIGHT must be a positive mainnet height.\n' >&2
  exit 1
}
[[ "$BTC_DESCRIPTOR_IS_DEDICATED_UNUSED" == true ]] || {
  printf 'Set BTC_DESCRIPTOR_IS_DEDICATED_UNUSED=true only for a dedicated, never-used descriptor.\n' >&2
  exit 1
}

for command in ssh ssh-keygen stat grep sed date tr base64 awk basename dirname \
  mktemp readlink rm sha256sum sort; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done

require_secret_file() {
  local path=$1
  [[ -f "$path" && ! -L "$path" && $(stat -c '%a' "$path") == 600 ]] || {
    printf 'Input must be a regular, non-symlink mode-0600 file: %s\n' "$path" >&2
    exit 1
  }
  [[ $(stat -c '%s' "$path") -le 16384 ]] || {
    printf 'Input is unexpectedly large: %s\n' "$path" >&2
    exit 1
  }
}

for path in "$BTC_PUBLIC_DESCRIPTOR_FILE" "$MONERO_PRIMARY_ADDRESS_FILE" \
  "$MONERO_PRIVATE_VIEW_KEY_FILE" "$OFFLINE_CEREMONY_RECEIPT_FILE" \
  "$OFFLINE_CEREMONY_SHA256SUMS_FILE" "$SSH_KEY"; do
  require_secret_file "$path"
done

TRANSFER_DIR=$(readlink -f -- "$(dirname -- "$OFFLINE_CEREMONY_RECEIPT_FILE")")
[[ -d "$TRANSFER_DIR" && ! -L "$TRANSFER_DIR" ]] || {
  printf 'Offline ceremony transfer directory is invalid.\n' >&2
  exit 1
}
[[ ! -e "$TRANSFER_DIR/CEREMONY-INCOMPLETE" && \
   ! -L "$TRANSFER_DIR/CEREMONY-INCOMPLETE" ]] || {
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
require_secret_file "$MONERO_RESTORE_HEIGHT_FILE"

mapfile -t CEREMONY_RECEIPT_LINES <"$OFFLINE_CEREMONY_RECEIPT_FILE"
[[ ${#CEREMONY_RECEIPT_LINES[@]} -eq 3 && \
   ${CEREMONY_RECEIPT_LINES[0]} == ceremony_version=1 && \
   ${CEREMONY_RECEIPT_LINES[1]} =~ ^backup_manifest_sha256=([0-9a-f]{64})$ && \
   ${CEREMONY_RECEIPT_LINES[2]} =~ ^transfer_manifest_sha256=([0-9a-f]{64})$ ]] || {
  printf 'Offline ceremony completion receipt has an invalid schema.\n' >&2
  exit 1
}
RECEIPT_TRANSFER_MANIFEST_SHA256=${CEREMONY_RECEIPT_LINES[2]#transfer_manifest_sha256=}
unset CEREMONY_RECEIPT_LINES

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
ACTUAL_TRANSFER_MANIFEST_SHA256=$(
  sha256sum "$OFFLINE_CEREMONY_SHA256SUMS_FILE" | awk '{print $1}'
)
[[ "$ACTUAL_TRANSFER_MANIFEST_SHA256" == "$RECEIPT_TRANSFER_MANIFEST_SHA256" ]] || {
  printf 'Offline ceremony completion receipt does not bind this transfer manifest.\n' >&2
  exit 1
}
unset ACTUAL_TRANSFER_MANIFEST_SHA256 RECEIPT_TRANSFER_MANIFEST_SHA256
BUNDLED_MONERO_RESTORE_HEIGHT=$(tr -d '\r\n' <"$MONERO_RESTORE_HEIGHT_FILE")
[[ "$BUNDLED_MONERO_RESTORE_HEIGHT" == "$MONERO_RESTORE_HEIGHT" ]] || {
  printf 'MONERO_RESTORE_HEIGHT does not match the completed offline ceremony.\n' >&2
  exit 1
}
unset BUNDLED_MONERO_RESTORE_HEIGHT

BTC_DESCRIPTOR=$(tr -d '\r\n' <"$BTC_PUBLIC_DESCRIPTOR_FILE")
MONERO_ADDRESS=$(tr -d '\r\n' <"$MONERO_PRIMARY_ADDRESS_FILE")
MONERO_VIEW_KEY=$(tr -d '\r\n' <"$MONERO_PRIVATE_VIEW_KEY_FILE")
PINNED_KNOWN_HOSTS=$(mktemp)
chmod 0600 "$PINNED_KNOWN_HOSTS"
cleanup() {
  unset BTC_DESCRIPTOR MONERO_ADDRESS MONERO_VIEW_KEY
  rm -f -- "${PINNED_KNOWN_HOSTS:-}"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

[[ "$BTC_DESCRIPTOR" == wpkh\(* && "$BTC_DESCRIPTOR" == *'#'* ]] || {
  printf 'Bitcoin input must be a checksummed external ranged wpkh descriptor.\n' >&2
  exit 1
}
[[ "$BTC_DESCRIPTOR" == *'/*)'* || "$BTC_DESCRIPTOR" == *'/*)#'* ]] || {
  printf 'Bitcoin descriptor must be ranged.\n' >&2
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

pin_found=false
while IFS= read -r known_host_line; do
  [[ -n "$known_host_line" && "$known_host_line" != \#* ]] || continue
  candidate_fingerprint=$(printf '%s\n' "$known_host_line" \
    | ssh-keygen -lf - -E sha256 2>/dev/null \
    | awk 'NR == 1 { print $2 }' || true)
  if [[ "$candidate_fingerprint" == "$OSL_PAYMENTS_HOST_KEY_SHA256" ]]; then
    printf '%s\n' "$known_host_line" >>"$PINNED_KNOWN_HOSTS"
    pin_found=true
  fi
done < <(ssh-keygen -F "$VPS_HOST" 2>/dev/null || true)
[[ "$pin_found" == true && -s "$PINNED_KNOWN_HOSTS" ]] || {
  printf 'The exact pinned VPS SSH host key is missing or changed.\n' >&2
  exit 1
}

SSH=(
  ssh
  -i "$SSH_KEY"
  -o BatchMode=yes
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$PINNED_KNOWN_HOSTS"
  -o GlobalKnownHostsFile=/dev/null
  -o ServerAliveInterval=30
  -o ServerAliveCountMax=6
  "$VPS"
)

# Send the private view key only inside the protected SSH stdin stream. Base64
# keeps the generated shell assignments unambiguous; it is not encryption.
{
  printf '%s\n' 'set +x'
  printf "BTC_DESCRIPTOR_B64='%s'\n" "$(printf '%s' "$BTC_DESCRIPTOR" | base64 -w0)"
  printf "MONERO_ADDRESS_B64='%s'\n" "$(printf '%s' "$MONERO_ADDRESS" | base64 -w0)"
  printf "MONERO_VIEW_KEY_B64='%s'\n" "$(printf '%s' "$MONERO_VIEW_KEY" | base64 -w0)"
  printf "MONERO_RESTORE_HEIGHT='%s'\n" "$MONERO_RESTORE_HEIGHT"
  cat <<'REMOTE'
set -euo pipefail
umask 077

for command in awk base64 chown chmod cmp curl date flock install jq mktemp mv \
  rm sed seq sha256sum sleep stat sudo systemctl; do
  command -v "$command" >/dev/null || {
    printf 'Missing required VPS command: %s\n' "$command" >&2
    exit 1
  }
done

BTC_DESCRIPTOR=$(printf '%s' "$BTC_DESCRIPTOR_B64" | base64 -d)
MONERO_ADDRESS=$(printf '%s' "$MONERO_ADDRESS_B64" | base64 -d)
MONERO_VIEW_KEY=$(printf '%s' "$MONERO_VIEW_KEY_B64" | base64 -d)
unset BTC_DESCRIPTOR_B64 MONERO_ADDRESS_B64 MONERO_VIEW_KEY_B64
BTC_DESCRIPTOR_SHA256=$(printf '%s' "$BTC_DESCRIPTOR" | sha256sum | awk '{print $1}')
XMR_VIEW_MATERIAL_SHA256=$(
  printf '%s\n%s\n%s\n' "$MONERO_ADDRESS" "$MONERO_VIEW_KEY" "$MONERO_RESTORE_HEIGHT" \
    | sha256sum | awk '{print $1}'
)

LOCK_DIR=/run/osl-crypto
LOCK_FILE="$LOCK_DIR/watch-only-provision.lock"
if [[ -L "$LOCK_DIR" || ( -e "$LOCK_DIR" && ! -d "$LOCK_DIR" ) ]]; then
  printf 'Runtime lock directory is a symlink or not a directory.\n' >&2
  exit 1
fi
install -d -o root -g root -m 0700 "$LOCK_DIR"
[[ -d "$LOCK_DIR" && ! -L "$LOCK_DIR" && \
   $(stat -c '%u:%g:%a' "$LOCK_DIR") == 0:0:700 ]] || {
  printf 'Runtime lock directory has unsafe ownership or permissions.\n' >&2
  exit 1
}
if [[ -L "$LOCK_FILE" || ( -e "$LOCK_FILE" && ! -f "$LOCK_FILE" ) ]]; then
  printf 'Runtime lock file is a symlink or not a regular file.\n' >&2
  exit 1
fi
if [[ ! -e "$LOCK_FILE" ]]; then
  install -o root -g root -m 0600 /dev/null "$LOCK_FILE"
fi
[[ -f "$LOCK_FILE" && ! -L "$LOCK_FILE" && \
   $(stat -c '%u:%g:%a' "$LOCK_FILE") == 0:0:600 ]] || {
  printf 'Runtime lock file has unsafe ownership or permissions.\n' >&2
  exit 1
}
exec 9<>"$LOCK_FILE"
flock -n 9 || {
  printf 'Another watch-only provisioning ceremony is active.\n' >&2
  exit 1
}

WATCHER_UNIT=osl-crypto-watcher.service
MONERO_UNIT=monero-wallet-rpc.service
XMR_WALLET=/var/lib/osl-crypto/wallets/osl-view-only
XMR_KEYS=/var/lib/osl-crypto/wallets/osl-view-only.keys
XMR_RECEIPT=/etc/osl-crypto/monero-view-only-creation.receipt
WATCHER_ENV=/etc/osl-crypto/watcher.env
INVOICE_DB=/var/lib/osl-crypto/invoices.sqlite3
MUTATION_STARTED=false
XMR_CREATED=false
RECEIPT_CREATED=false
XMR_BACKUP_WALLET=
XMR_BACKUP_KEYS=
ENV_TMP=

unit_active() {
  if systemctl is-active --quiet "$1"; then printf 'true'; else printf 'false'; fi
}
unit_enabled_state() {
  systemctl is-enabled "$1" 2>/dev/null || true
}
restore_unit() {
  local unit=$1 enabled_state=$2 was_active=$3
  systemctl unmask --runtime "$unit" >/dev/null 2>&1 || true
  case "$enabled_state" in
    enabled) systemctl enable "$unit" >/dev/null 2>&1 || true ;;
    enabled-runtime) systemctl enable --runtime "$unit" >/dev/null 2>&1 || true ;;
    disabled) systemctl disable "$unit" >/dev/null 2>&1 || true ;;
    masked) systemctl mask "$unit" >/dev/null 2>&1 || true ;;
    masked-runtime) systemctl mask --runtime "$unit" >/dev/null 2>&1 || true ;;
  esac
  if [[ "$was_active" == true ]]; then
    systemctl start "$unit" >/dev/null 2>&1 || true
  else
    systemctl stop "$unit" >/dev/null 2>&1 || true
  fi
}
remote_cleanup() {
  local status=$?
  set +e
  unset BTC_DESCRIPTOR MONERO_ADDRESS MONERO_VIEW_KEY
  [[ -z "$ENV_TMP" ]] || rm -f -- "$ENV_TMP"
  if [[ "$status" -ne 0 && "$MUTATION_STARTED" == true ]]; then
    systemctl stop "$WATCHER_UNIT" "$MONERO_UNIT" >/dev/null 2>&1 || true
    if [[ "$XMR_CREATED" == true ]]; then
      rm -f -- "$XMR_WALLET" "$XMR_KEYS"
      [[ -z "$XMR_BACKUP_WALLET" ]] || rm -f -- "$XMR_BACKUP_WALLET"
      [[ -z "$XMR_BACKUP_KEYS" ]] || rm -f -- "$XMR_BACKUP_KEYS"
    fi
    if [[ "$RECEIPT_CREATED" == true ]]; then
      rm -f -- "$XMR_RECEIPT"
    fi
    restore_unit "$MONERO_UNIT" "$MONERO_ENABLED_BEFORE" "$MONERO_ACTIVE_BEFORE"
    restore_unit "$WATCHER_UNIT" "$WATCHER_ENABLED_BEFORE" "$WATCHER_ACTIVE_BEFORE"
  fi
  exit "$status"
}
trap remote_cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# Read-only preflight. Nothing below this comment changes service or wallet
# state until every prerequisite and existing artifact has been validated.
for path in \
  /etc/osl-crypto/monero-wallet-password \
  /etc/osl-crypto/watcher-request-secret \
  /etc/osl-crypto/watcher-db-key \
  /etc/osl-crypto/watcher-settlement-key.pem; do
  [[ -f "$path" && ! -L "$path" && $(stat -c '%a' "$path") == 600 &&
     $(stat -c '%s' "$path") -gt 0 && $(stat -c '%s' "$path") -le 16384 ]] || {
    printf 'Required credential must be regular, non-symlink, and mode 0600: %s\n' "$path" >&2
    exit 1
  }
done
for path in "$INVOICE_DB" "$XMR_WALLET" "$XMR_KEYS" "$XMR_RECEIPT" "$WATCHER_ENV"; do
  [[ ! -L "$path" ]] || {
    printf 'Refusing symlinked runtime artifact: %s\n' "$path" >&2
    exit 1
  }
done
[[ ! -e "$INVOICE_DB" ]] || {
  printf 'Refusing to provision over an existing invoice database.\n' >&2
  exit 1
}

btc=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
  -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie getblockchaininfo)
jq -e '.chain == "main" and .initialblockdownload == false and .blocks == .headers' \
  >/dev/null <<<"$btc"
wallet=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
  -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
  -rpcwallet=osl-watch getwalletinfo)
jq -e '.private_keys_enabled == false and .descriptors == true and .txcount == 0' \
  >/dev/null <<<"$wallet"
descriptor_info=$(printf '%s\n' "$BTC_DESCRIPTOR" \
  | sudo -n -u bitcoin /usr/local/bin/bitcoin-cli -stdin \
    -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie getdescriptorinfo)
jq -e '.hasprivatekeys == false and .isrange == true' >/dev/null <<<"$descriptor_info"

descriptors=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
  -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
  -rpcwallet=osl-watch listdescriptors false)
descriptor_count=$(jq '.descriptors | length' <<<"$descriptors")
if [[ "$descriptor_count" -eq 0 ]]; then
  BTC_IMPORT_NEEDED=true
elif [[ "$descriptor_count" -eq 1 ]] && jq -e --arg expected "$BTC_DESCRIPTOR" \
  '.descriptors[0].desc == $expected and .descriptors[0].active == true and
   .descriptors[0].internal == false and .descriptors[0].range == [0, 99999] and
   .descriptors[0].next == 0' >/dev/null <<<"$descriptors"; then
  BTC_IMPORT_NEEDED=false
else
  printf 'Bitcoin watch wallet contains a different or unexpected descriptor.\n' >&2
  exit 1
fi

xmr=$(curl -fsS --max-time 5 http://127.0.0.1:18081/get_info)
jq -e '.nettype == "mainnet" and .synchronized == true and
  ((.target_height == 0) or (.height >= .target_height))' >/dev/null <<<"$xmr"
xmr_height=$(jq -r '.height' <<<"$xmr")
[[ "$MONERO_RESTORE_HEIGHT" -le "$xmr_height" ]] || {
  printf 'Monero restore height is ahead of the synchronized daemon.\n' >&2
  exit 1
}

wallet_exists=false
keys_exist=false
[[ ! -e "$XMR_WALLET" ]] || wallet_exists=true
[[ ! -e "$XMR_KEYS" ]] || keys_exist=true
if [[ "$wallet_exists" == false && "$keys_exist" == false && ! -e "$XMR_RECEIPT" ]]; then
  XMR_CREATE_NEEDED=true
elif [[ "$wallet_exists" == true && "$keys_exist" == true &&
        -f "$XMR_WALLET" && -f "$XMR_KEYS" && -f "$XMR_RECEIPT" &&
        $(stat -c '%a' "$XMR_RECEIPT") == 600 ]]; then
  receipt_hash=$(sed -n 's/^view_material_sha256=//p' "$XMR_RECEIPT")
  receipt_address=$(sed -n 's/^primary_address=//p' "$XMR_RECEIPT")
  receipt_btc_hash=$(sed -n 's/^btc_descriptor_sha256=//p' "$XMR_RECEIPT")
  [[ "$receipt_hash" == "$XMR_VIEW_MATERIAL_SHA256" &&
     "$receipt_address" == "$MONERO_ADDRESS" &&
     "$receipt_btc_hash" == "$BTC_DESCRIPTOR_SHA256" ]] || {
    printf 'Existing Monero wallet provenance does not match the supplied view material.\n' >&2
    exit 1
  }
  XMR_CREATE_NEEDED=false
else
  printf 'Monero wallet state is partial or lacks a matching creation receipt.\n' >&2
  exit 1
fi

expected_env() {
  printf '%s\n' \
    'BITCOIN_RPC_URL=http://127.0.0.1:8332/' \
    'CRYPTO_BTC_ENABLED=true' \
    'BITCOIN_COOKIE_FILE=/run/bitcoind/.cookie' \
    'BITCOIN_WATCH_WALLET=osl-watch' \
    'MONERO_WALLET_RPC_URL=http://127.0.0.1:18088/' \
    'CRYPTO_XMR_ENABLED=true' \
    'MONERO_ACCOUNT_INDEX=0'
  printf 'MONERO_PRIMARY_ADDRESS=%s\n' "$MONERO_ADDRESS"
  printf '%s\n' \
    'CRYPTO_SETTLEMENT_CALLBACK_URL=https://keyserver.oslprivacy.com/v1/internal/crypto/settle' \
    'CRYPTO_WATCHER_REQUEST_SECRET_FILE=/etc/osl-crypto/watcher-request-secret' \
    'CRYPTO_WATCHER_SETTLEMENT_SIGNING_KEY_FILE=/etc/osl-crypto/watcher-settlement-key.pem' \
    'CRYPTO_WATCHER_DB_KEY_FILE=/etc/osl-crypto/watcher-db-key' \
    'CRYPTO_WATCHER_DB=/var/lib/osl-crypto/invoices.sqlite3' \
    'CRYPTO_BTC_CONFIRMATIONS=2' \
    'CRYPTO_XMR_CONFIRMATIONS=10' \
    'INVOICE_RETENTION_SECONDS=604800' \
    'LISTEN_ADDR=127.0.0.1:8789'
}
if [[ -e "$WATCHER_ENV" ]] && ! cmp -s "$WATCHER_ENV" <(expected_env); then
  printf 'Existing watcher.env differs; refusing to overwrite operator configuration.\n' >&2
  exit 1
fi

WATCHER_ACTIVE_BEFORE=$(unit_active "$WATCHER_UNIT")
MONERO_ACTIVE_BEFORE=$(unit_active "$MONERO_UNIT")
WATCHER_ENABLED_BEFORE=$(unit_enabled_state "$WATCHER_UNIT")
MONERO_ENABLED_BEFORE=$(unit_enabled_state "$MONERO_UNIT")

# Mutating phase. Failure from this point restores the prior service state and
# removes only XMR artifacts created by this invocation. An exact BTC import is
# deliberately resumable because descriptor imports cannot be rolled back.
MUTATION_STARTED=true
systemctl stop "$WATCHER_UNIT" "$MONERO_UNIT"
systemctl mask --runtime "$WATCHER_UNIT" "$MONERO_UNIT" >/dev/null

STAMP=$(date -u +%Y%m%dT%H%M%SZ)-$$
if [[ "$BTC_IMPORT_NEEDED" == true ]]; then
  payload=$(jq -cn --arg descriptor "$BTC_DESCRIPTOR" \
    '[{desc:$descriptor,timestamp:"now",active:true,internal:false,range:[0,99999],next_index:0}]')
  result=$(printf '%s\n' "$payload" | sudo -n -u bitcoin \
    /usr/local/bin/bitcoin-cli -stdin -datadir=/var/lib/bitcoind \
    -rpccookiefile=/run/bitcoind/.cookie -rpcwallet=osl-watch importdescriptors)
  jq -e 'length == 1 and .[0].success == true' >/dev/null <<<"$result"
fi
post_descriptors=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
  -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
  -rpcwallet=osl-watch listdescriptors false)
jq -e --arg expected "$BTC_DESCRIPTOR" \
  '.descriptors | length == 1 and .[0].desc == $expected and
   .[0].active == true and .[0].internal == false and
   .[0].range == [0, 99999] and .[0].next == 0' >/dev/null <<<"$post_descriptors"
install -d -o bitcoin -g bitcoin -m 0700 /var/lib/bitcoind/watch-wallet-backups
BTC_BACKUP=/var/lib/bitcoind/watch-wallet-backups/osl-watch-$STAMP.dat
sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
  -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
  -rpcwallet=osl-watch backupwallet "$BTC_BACKUP" >/dev/null
chmod 0600 "$BTC_BACKUP"

if [[ "$XMR_CREATE_NEEDED" == true ]]; then
  install -d -o osl-crypto -g osl-crypto -m 0700 /var/lib/osl-crypto/wallets
  [[ ! -e "$XMR_WALLET" && ! -L "$XMR_WALLET" &&
     ! -e "$XMR_KEYS" && ! -L "$XMR_KEYS" ]] || {
    printf 'Monero wallet path changed during provisioning.\n' >&2
    exit 1
  }
  XMR_CREATED=true
  printf '%s\n%s\n' "$MONERO_ADDRESS" "$MONERO_VIEW_KEY" | sudo -n -u osl-crypto \
    /usr/local/bin/monero-wallet-cli \
      --generate-from-view-key "$XMR_WALLET" \
      --password-file /etc/osl-crypto/monero-wallet-password \
      --restore-height "$MONERO_RESTORE_HEIGHT" \
      --daemon-address 127.0.0.1:18081 --log-file /dev/null --log-level 0 \
      >/dev/null
  [[ -f "$XMR_WALLET" && ! -L "$XMR_WALLET" && -f "$XMR_KEYS" && ! -L "$XMR_KEYS" ]]
  install -d -o osl-crypto -g osl-crypto -m 0700 \
    /var/lib/osl-crypto/watch-wallet-backups
  XMR_BACKUP_WALLET=/var/lib/osl-crypto/watch-wallet-backups/osl-view-only-$STAMP.wallet
  XMR_BACKUP_KEYS=/var/lib/osl-crypto/watch-wallet-backups/osl-view-only-$STAMP.wallet.keys
  install -o osl-crypto -g osl-crypto -m 0600 "$XMR_WALLET" "$XMR_BACKUP_WALLET"
  install -o osl-crypto -g osl-crypto -m 0600 "$XMR_KEYS" "$XMR_BACKUP_KEYS"
  receipt_tmp=$(mktemp /etc/osl-crypto/.monero-view-only-creation.XXXXXX)
  printf 'method=generate-from-view-key\nprimary_address=%s\nrestore_height=%s\nview_material_sha256=%s\nbtc_descriptor_sha256=%s\ncreated_at=%s\n' \
    "$MONERO_ADDRESS" "$MONERO_RESTORE_HEIGHT" "$XMR_VIEW_MATERIAL_SHA256" \
    "$BTC_DESCRIPTOR_SHA256" "$STAMP" >"$receipt_tmp"
  chown root:osl-crypto "$receipt_tmp"
  chmod 0600 "$receipt_tmp"
  RECEIPT_CREATED=true
  mv -T "$receipt_tmp" "$XMR_RECEIPT"
fi

systemctl unmask --runtime "$MONERO_UNIT"
systemctl enable --now "$MONERO_UNIT"
for attempt in $(seq 1 60); do
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
[[ "${address_verified:-false}" == true ]] || {
  printf 'Monero Wallet RPC did not return the pinned primary address.\n' >&2
  exit 1
}

systemctl disable "$WATCHER_UNIT" >/dev/null
ENV_TMP=$(mktemp /etc/osl-crypto/.watcher.env.XXXXXX)
expected_env >"$ENV_TMP"
chown root:osl-crypto "$ENV_TMP"
chmod 0640 "$ENV_TMP"
mv -T "$ENV_TMP" "$WATCHER_ENV"
ENV_TMP=
unset BTC_DESCRIPTOR MONERO_ADDRESS MONERO_VIEW_KEY
REMOTE
} | "${SSH[@]}" 'sudo -n /bin/bash -s'

printf '%s\n' \
  'BTC public descriptor and XMR view material were imported.' \
  'Bitcoin Core confirmed a public-only ranged descriptor.' \
  'Monero Wallet RPC returned the pinned primary address; retain the creation receipt.' \
  'The public watcher remains disabled pending payment and replay canaries.'
