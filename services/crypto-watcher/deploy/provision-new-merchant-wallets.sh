#!/usr/bin/env bash
set -euo pipefail
umask 077

if [[ $- == *x* ]]; then
  set +x
  printf 'Refusing to handle wallet secrets while shell tracing is enabled.\n' >&2
  exit 1
fi
set +x
[[ -t 0 && -t 1 ]] || {
  printf 'Run this only in a clean, interactive local terminal.\n' >&2
  exit 1
}

# Creates dedicated merchant spending wallets on this PC and transfers only
# watch/view material to the isolated OSL payment VPS. Run interactively in a
# trusted terminal. Never run through CI, a log collector, or a remote shell.

CRYPTO_DATA_HOME=${OSL_CRYPTO_DATA_HOME:-${XDG_DATA_HOME:-"$HOME/.local/share"}/osl-crypto}
CRYPTO_BIN_HOME=${OSL_CRYPTO_BIN_HOME:-"$HOME/.local/opt/osl-crypto/bin"}
BITCOIN_CLI=${BITCOIN_CLI:-"$CRYPTO_BIN_HOME/bitcoin-cli"}
BITCOIN_DATA=${BITCOIN_DATA:-"$CRYPTO_DATA_HOME/bitcoin"}
BITCOIN_WALLET=${BITCOIN_WALLET:-osl-merchant-spend-v1}
MONERO_WALLET_CLI=${MONERO_WALLET_CLI:-"$CRYPTO_BIN_HOME/monero-wallet-cli"}
MONERO_WALLET=${MONERO_WALLET:-"$CRYPTO_DATA_HOME/wallets/osl-merchant-spend-v1"}
BACKUP_DIR=${BACKUP_DIR:-"$CRYPTO_DATA_HOME/merchant-backups"}
RECEIPT_DIR=${RECEIPT_DIR:-"$CRYPTO_DATA_HOME/merchant-receipts"}
SSH_KEY=${OSL_PAYMENTS_SSH_KEY:-"$HOME/.ssh/osl_payments_ed25519"}
: "${OSL_PAYMENTS_VPS:?Set OSL_PAYMENTS_VPS to the dedicated user@host target}"
: "${OSL_PAYMENTS_HOST_KEY_SHA256:?Set OSL_PAYMENTS_HOST_KEY_SHA256 to the pinned SHA256 fingerprint}"
VPS=$OSL_PAYMENTS_VPS
VPS_HOST_KEY_SHA256=$OSL_PAYMENTS_HOST_KEY_SHA256
VPS_HOST=${VPS#*@}
[[ "$VPS" == *@* && "$VPS_HOST" =~ ^[A-Za-z0-9.-]+$ ]] || {
  printf 'OSL_PAYMENTS_VPS must be a plain user@hostname or user@IPv4 target.\n' >&2
  exit 1
}
[[ "$VPS_HOST_KEY_SHA256" =~ ^SHA256:[A-Za-z0-9+/]{43}=?$ ]] || {
  printf 'OSL_PAYMENTS_HOST_KEY_SHA256 is not a SHA256 host-key fingerprint.\n' >&2
  exit 1
}

for command in jq curl ssh install mktemp sha256sum stat ssh-keygen grep \
  head awk date sort uniq cut wc; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done
"$BITCOIN_CLI" -datadir="$BITCOIN_DATA" getblockchaininfo \
  | jq -e '.chain == "main"' >/dev/null || {
    printf 'The local Bitcoin wallet backend is not on mainnet.\n' >&2
    exit 1
  }
for binary in "$BITCOIN_CLI" "$MONERO_WALLET_CLI" "$SSH_KEY"; do
  [[ -e "$binary" ]] || {
    printf 'Missing required file: %s\n' "$binary" >&2
    exit 1
  }
done
[[ -f "$SSH_KEY" && ! -L "$SSH_KEY" && $(stat -c '%a' "$SSH_KEY") == 600 ]] || {
  printf 'SSH key must be a regular, non-symlink mode-0600 file.\n' >&2
  exit 1
}
ssh-keygen -F "$VPS_HOST" 2>/dev/null \
  | ssh-keygen -lf - -E sha256 2>/dev/null \
  | grep -Fq "$VPS_HOST_KEY_SHA256" || {
    printf 'The pinned VPS SSH host key is missing or has changed.\n' >&2
    exit 1
  }
"$BITCOIN_CLI" -version | head -1 | grep -Fq 'v31.1.0' || {
  printf 'Expected the verified Bitcoin Core 31.1 client.\n' >&2
  exit 1
}
"$MONERO_WALLET_CLI" --version 2>&1 | head -1 \
  | grep -Fq 'v0.18.5.1-release' || {
    printf 'Expected the verified Monero 0.18.5.1 client.\n' >&2
    exit 1
  }

mkdir -p "$BACKUP_DIR" "$RECEIPT_DIR" "$(dirname "$MONERO_WALLET")"
chmod 700 "$BACKUP_DIR" "$RECEIPT_DIR" "$(dirname "$MONERO_WALLET")"

printf '%s\n' \
  'OSL new merchant-wallet provisioning' \
  'Spending authority stays on this PC.' \
  'The VPS receives only Bitcoin watch-only and Monero view-only material.' \
  '' \
  'Before funding either wallet, you must make offline backups and complete' \
  'a separate restore rehearsal. The VPS is not a spending-wallet backup.' \
  ''
read -r -p "Type CREATE to continue: " CONFIRM
[[ "$CONFIRM" == CREATE ]] || {
  printf 'Cancelled.\n'
  exit 1
}
unset CONFIRM

# Fail closed before creating either local spending wallet. This initial-only
# provisioner refuses any invoice database, prior active receive descriptor,
# unsynchronized chain, or active wallet/watcher process.
MONERO_CREATION_HEIGHT=$(ssh -i "$SSH_KEY" -o BatchMode=yes \
  -o StrictHostKeyChecking=yes "$VPS" '
    set -euo pipefail
    sudo -n systemctl stop osl-crypto-watcher.service
    sudo -n systemctl stop monero-wallet-rpc.service
    sudo -n systemctl mask --runtime osl-crypto-watcher.service monero-wallet-rpc.service \
      >/dev/null
    ! sudo -n systemctl is-active --quiet osl-crypto-watcher.service
    ! sudo -n systemctl is-active --quiet monero-wallet-rpc.service
    test "$(sudo -n systemctl is-enabled osl-crypto-watcher.service)" = masked-runtime
    test "$(sudo -n systemctl is-enabled monero-wallet-rpc.service)" = masked-runtime
    ! sudo -n test -e /var/lib/osl-crypto/invoices.sqlite3
    test "$(sudo -n stat -c "%a %U %G %F" /etc/osl-crypto/monero-wallet-password)" = \
      "600 osl-crypto osl-crypto regular file"
    test "$(sudo -n stat -c "%a %U %G %F" /var/lib/osl-crypto/wallets)" = \
      "700 osl-crypto osl-crypto directory"
    sudo -n test ! -L /etc/osl-crypto/monero-wallet-password
    sudo -n test ! -e /var/lib/osl-crypto/wallets/osl-view-only
    sudo -n test ! -e /var/lib/osl-crypto/wallets/osl-view-only.keys
    listeners=$(sudo -n ss -H -ltn)
    ! grep -Eq ":(8789|18088)[[:space:]]" <<<"$listeners"

    btc_chain=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
      -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
      getblockchaininfo)
    jq -e ".chain == \"main\" and .initialblockdownload == false and .blocks == .headers" \
      >/dev/null <<<"$btc_chain"
    btc_wallet=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
      -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
      -rpcwallet=osl-watch getwalletinfo)
    jq -e ".private_keys_enabled == false and .descriptors == true and .txcount == 0" \
      >/dev/null <<<"$btc_wallet"
    btc_descriptors=$(sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
      -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
      -rpcwallet=osl-watch listdescriptors false)
    jq -e ".descriptors | length == 0" >/dev/null <<<"$btc_descriptors"

    monero_info=$(curl -fsS --max-time 5 http://127.0.0.1:18081/get_info)
    jq -e ".nettype == \"mainnet\" and .synchronized == true and .height > 2000 and
      ((.target_height == 0) or (.height >= .target_height))" \
      >/dev/null <<<"$monero_info"
    jq -er ".height" <<<"$monero_info"
  ')
[[ "$MONERO_CREATION_HEIGHT" =~ ^[0-9]+$ && "$MONERO_CREATION_HEIGHT" -gt 2000 ]] || {
  printf 'VPS preflight did not prove fully synchronized mainnet nodes.\n' >&2
  exit 1
}
MONERO_RESTORE_HEIGHT=$((MONERO_CREATION_HEIGHT - 1000))

if "$BITCOIN_CLI" -datadir="$BITCOIN_DATA" listwalletdir \
  | jq -e --arg name "$BITCOIN_WALLET" '.wallets[]? | select(.name == $name)' >/dev/null; then
  printf 'Refusing to overwrite Bitcoin wallet %s.\n' "$BITCOIN_WALLET" >&2
  exit 1
fi
if [[ -e "$MONERO_WALLET" || -e "$MONERO_WALLET.keys" ]]; then
  printf 'Refusing to overwrite Monero wallet %s.\n' "$MONERO_WALLET" >&2
  exit 1
fi

read -r -s -p "New Bitcoin wallet passphrase (20+ characters): " BTC_PASS
printf '\n'
read -r -s -p "Repeat Bitcoin wallet passphrase: " BTC_PASS_CONFIRM
printf '\n'
[[ "$BTC_PASS" == "$BTC_PASS_CONFIRM" && ${#BTC_PASS} -ge 20 ]] || {
  unset BTC_PASS BTC_PASS_CONFIRM
  printf 'Bitcoin passphrases differ or are shorter than 20 characters.\n' >&2
  exit 1
}
unset BTC_PASS_CONFIRM

printf 'passphrase=%s\n' "$BTC_PASS" | "$BITCOIN_CLI" \
  -datadir="$BITCOIN_DATA" -stdin -named createwallet \
  "wallet_name=$BITCOIN_WALLET" \
  disable_private_keys=false blank=false avoid_reuse=true \
  descriptors=true load_on_startup=true >/dev/null
unset BTC_PASS

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BTC_BACKUP="$BACKUP_DIR/$BITCOIN_WALLET-$STAMP.dat"
"$BITCOIN_CLI" -datadir="$BITCOIN_DATA" -rpcwallet="$BITCOIN_WALLET" \
  backupwallet "$BTC_BACKUP" >/dev/null
chmod 600 "$BTC_BACKUP"

BTC_DESCRIPTORS=$("$BITCOIN_CLI" -datadir="$BITCOIN_DATA" \
  -rpcwallet="$BITCOIN_WALLET" listdescriptors false)
BTC_DESCRIPTOR=$(jq -er '
  [.descriptors[]
   | select(.active == true and .internal == false)
   | select(.desc | startswith("wpkh("))
   | .desc] as $matches
  | if ($matches | length) == 1 then $matches[0] else error("expected one external wpkh descriptor") end
' <<<"$BTC_DESCRIPTORS")
unset BTC_DESCRIPTORS

[[ "$BTC_DESCRIPTOR" != *prv* && "$BTC_DESCRIPTOR" != *PRV* ]] || {
  unset BTC_DESCRIPTOR
  printf 'Refusing a Bitcoin descriptor containing private key material.\n' >&2
  exit 1
}
BTC_DESCRIPTOR_INFO=$("$BITCOIN_CLI" -datadir="$BITCOIN_DATA" \
  getdescriptorinfo "$BTC_DESCRIPTOR")
jq -e '.isrange == true and .hasprivatekeys == false' \
  <<<"$BTC_DESCRIPTOR_INFO" >/dev/null || {
    unset BTC_DESCRIPTOR BTC_DESCRIPTOR_INFO
    printf 'Bitcoin public descriptor failed range/private-key checks.\n' >&2
    exit 1
  }
unset BTC_DESCRIPTOR_INFO

BTC_FIRST_LOCAL=$(printf '%s\n[0,0]\n' "$BTC_DESCRIPTOR" \
  | "$BITCOIN_CLI" -datadir="$BITCOIN_DATA" -stdin deriveaddresses \
  | jq -er 'if length == 1 then .[0] else error("expected one address") end')
BTC_IMPORT=$(jq -cn --arg descriptor "$BTC_DESCRIPTOR" '
  [{desc:$descriptor,timestamp:"now",active:true,internal:false,
    range:[0,99999],next_index:0}]
')
BTC_IMPORT_RESULT=$(printf '%s\n' "$BTC_IMPORT" | ssh \
  -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n -u bitcoin /usr/local/bin/bitcoin-cli -stdin \
    -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
    -rpcwallet=osl-watch importdescriptors')
unset BTC_IMPORT
jq -e 'length == 1 and .[0].success == true' <<<"$BTC_IMPORT_RESULT" >/dev/null || {
  unset BTC_DESCRIPTOR BTC_IMPORT_RESULT
  printf 'VPS rejected the Bitcoin watch-only descriptor.\n' >&2
  exit 1
}
unset BTC_IMPORT_RESULT

BTC_REMOTE_INFO=$(ssh -i "$SSH_KEY" -o BatchMode=yes \
  -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
    -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
    -rpcwallet=osl-watch getwalletinfo')
jq -e '.private_keys_enabled == false and .descriptors == true' \
  <<<"$BTC_REMOTE_INFO" >/dev/null || {
    unset BTC_DESCRIPTOR BTC_REMOTE_INFO
    printf 'VPS Bitcoin wallet is not provably watch-only.\n' >&2
    exit 1
  }
unset BTC_REMOTE_INFO
BTC_FIRST_REMOTE=$(printf '%s\n[0,0]\n' "$BTC_DESCRIPTOR" | ssh \
  -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n -u bitcoin /usr/local/bin/bitcoin-cli -stdin \
    -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
    deriveaddresses' | jq -er \
  'if length == 1 then .[0] else error("expected one address") end')
[[ "$BTC_FIRST_LOCAL" == "$BTC_FIRST_REMOTE" ]] || {
  unset BTC_DESCRIPTOR BTC_FIRST_LOCAL BTC_FIRST_REMOTE
  printf 'Bitcoin index-0 address mismatch; checkout remains disabled.\n' >&2
  exit 1
}
ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  "sudo -n install -d -o bitcoin -g bitcoin -m 0700 /var/lib/bitcoind/watch-wallet-backups;
   sudo -n -u bitcoin /usr/local/bin/bitcoin-cli \
     -datadir=/var/lib/bitcoind -rpccookiefile=/run/bitcoind/.cookie \
     -rpcwallet=osl-watch backupwallet \
     /var/lib/bitcoind/watch-wallet-backups/osl-watch-$STAMP.dat >/dev/null;
   sudo -n chmod 0600 /var/lib/bitcoind/watch-wallet-backups/osl-watch-$STAMP.dat"

BTC_RECEIPT="$RECEIPT_DIR/bitcoin-public-$STAMP.json"
jq -n --arg created_at "$STAMP" --arg wallet "$BITCOIN_WALLET" \
  --arg descriptor "$BTC_DESCRIPTOR" --arg first_address "$BTC_FIRST_LOCAL" \
  --arg backup_sha256 "$(sha256sum "$BTC_BACKUP" | awk '{print $1}')" \
  '{created_at:$created_at,network:"main",wallet:$wallet,
    transferred_material:"external watch-only wpkh descriptor",
    descriptor:$descriptor,first_address:$first_address,
    encrypted_backup_sha256:$backup_sha256}' >"$BTC_RECEIPT"
chmod 600 "$BTC_RECEIPT"
unset BTC_DESCRIPTOR BTC_FIRST_LOCAL BTC_FIRST_REMOTE

read -r -s -p "New Monero wallet password (20+ characters): " XMR_PASS
printf '\n'
read -r -s -p "Repeat Monero wallet password: " XMR_PASS_CONFIRM
printf '\n'
[[ "$XMR_PASS" == "$XMR_PASS_CONFIRM" && ${#XMR_PASS} -ge 20 ]] || {
  unset XMR_PASS XMR_PASS_CONFIRM
  printf 'Monero passwords differ or are shorter than 20 characters.\n' >&2
  exit 1
}
unset XMR_PASS_CONFIRM

XMR_PASS_FILE=$(mktemp /dev/shm/osl-xmr-password.XXXXXX)
cleanup() {
  unset XMR_PASS MONERO_VIEW_KEY
  [[ -z "${XMR_PASS_FILE:-}" ]] || rm -f -- "$XMR_PASS_FILE"
}
trap cleanup EXIT HUP INT TERM
printf '%s\n' "$XMR_PASS" >"$XMR_PASS_FILE"
unset XMR_PASS
chmod 600 "$XMR_PASS_FILE"

printf 'exit\n' | "$MONERO_WALLET_CLI" \
  --generate-new-wallet "$MONERO_WALLET" \
  --password-file "$XMR_PASS_FILE" \
  --mnemonic-language English \
  --restore-height "$MONERO_RESTORE_HEIGHT" \
  --daemon-address 127.0.0.1:18081 \
  --log-file /dev/null --log-level 0 >/dev/null
chmod 600 "$MONERO_WALLET" "$MONERO_WALLET.keys"

printf '\nYour Monero recovery words will appear below.\n'
printf 'Record them offline. Never paste them into chat, Cloudflare, or the VPS.\n\n'
printf 'Stop screen recording and close any terminal logging before continuing.\n\n'
"$MONERO_WALLET_CLI" --wallet-file "$MONERO_WALLET" \
  --password-file "$XMR_PASS_FILE" --daemon-address 127.0.0.1:18081 \
  --log-file /dev/null --log-level 0 --command seed
printf '\n'
read -r -p "After recording the words offline, type RECORDED: " SEED_CONFIRM
[[ "$SEED_CONFIRM" == RECORDED ]] || {
  printf 'Stopped before exporting Monero view material. Wallet remains local.\n' >&2
  exit 1
}
unset SEED_CONFIRM
# Clear the visible screen and scrollback in terminals that support ECMA-48.
printf '\033[3J\033[H\033[2J'

XMR_ADDRESS_OUTPUT=$("$MONERO_WALLET_CLI" --wallet-file "$MONERO_WALLET" \
  --password-file "$XMR_PASS_FILE" --daemon-address 127.0.0.1:18081 \
  --log-file /dev/null --log-level 0 --command address 2>/dev/null)
MONERO_ADDRESS=$(grep -Eo '4[1-9A-HJ-NP-Za-km-z]{94}' \
  <<<"$XMR_ADDRESS_OUTPUT" | head -1)
unset XMR_ADDRESS_OUTPUT
[[ ${#MONERO_ADDRESS} -eq 95 ]] || {
  printf 'Could not safely parse the Monero primary address.\n' >&2
  exit 1
}

XMR_VIEW_OUTPUT=$("$MONERO_WALLET_CLI" --wallet-file "$MONERO_WALLET" \
  --password-file "$XMR_PASS_FILE" --daemon-address 127.0.0.1:18081 \
  --log-file /dev/null --log-level 0 --command viewkey 2>/dev/null)
MONERO_VIEW_KEY=$(sed -nE 's/.*[Ss]ecret[^:]*:[[:space:]]*([0-9a-fA-F]{64}).*/\1/p' \
  <<<"$XMR_VIEW_OUTPUT" | head -1)
unset XMR_VIEW_OUTPUT
[[ "$MONERO_VIEW_KEY" =~ ^[0-9a-fA-F]{64}$ ]] || {
  unset MONERO_VIEW_KEY
  printf 'Could not safely parse the Monero private view key.\n' >&2
  exit 1
}

ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  'set -euo pipefail;
   sudo -n systemctl stop osl-crypto-watcher.service;
   sudo -n systemctl stop monero-wallet-rpc.service;
   sudo -n systemctl mask --runtime osl-crypto-watcher.service monero-wallet-rpc.service \
     >/dev/null;
   ! sudo -n systemctl is-active --quiet osl-crypto-watcher.service;
   ! sudo -n systemctl is-active --quiet monero-wallet-rpc.service;
   sudo -n test ! -e /var/lib/osl-crypto/wallets/osl-view-only;
   sudo -n test ! -e /var/lib/osl-crypto/wallets/osl-view-only.keys'
printf '%s\n%s\n' "$MONERO_ADDRESS" "$MONERO_VIEW_KEY" | ssh \
  -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  "sudo -n -u osl-crypto /usr/local/bin/monero-wallet-cli \
    --generate-from-view-key /var/lib/osl-crypto/wallets/osl-view-only \
    --password-file /etc/osl-crypto/monero-wallet-password \
    --restore-height $MONERO_RESTORE_HEIGHT \
    --daemon-address 127.0.0.1:18081 --log-file /dev/null --log-level 0" \
  >/dev/null
unset MONERO_VIEW_KEY

ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  "sudo -n install -d -o osl-crypto -g osl-crypto -m 0700 \
     /var/lib/osl-crypto/watch-wallet-backups;
   sudo -n install -o osl-crypto -g osl-crypto -m 0600 \
     /var/lib/osl-crypto/wallets/osl-view-only \
     /var/lib/osl-crypto/watch-wallet-backups/osl-view-only-$STAMP.wallet;
   sudo -n install -o osl-crypto -g osl-crypto -m 0600 \
     /var/lib/osl-crypto/wallets/osl-view-only.keys \
     /var/lib/osl-crypto/watch-wallet-backups/osl-view-only-$STAMP.wallet.keys"

XMR_BACKUP_BASE="$BACKUP_DIR/osl-merchant-spend-v1-$STAMP"
install -m 600 "$MONERO_WALLET" "$XMR_BACKUP_BASE.wallet"
install -m 600 "$MONERO_WALLET.keys" "$XMR_BACKUP_BASE.wallet.keys"

MONERO_RECEIPT="$RECEIPT_DIR/monero-public-$STAMP.json"
jq -n --arg created_at "$STAMP" --arg address "$MONERO_ADDRESS" \
  --argjson restore_height "$MONERO_RESTORE_HEIGHT" \
  --arg wallet_keys_sha256 "$(sha256sum "$MONERO_WALLET.keys" | awk '{print $1}')" \
  '{created_at:$created_at,network:"main",method:"generate-from-view-key",
    primary_address:$address,restore_height:$restore_height,
    local_encrypted_wallet_keys_sha256:$wallet_keys_sha256,
    spend_authority_transferred:false}' >"$MONERO_RECEIPT"
chmod 600 "$MONERO_RECEIPT"

WATCHER_ENV=$(jq -nr --arg address "$MONERO_ADDRESS" '
  ["BITCOIN_RPC_URL=http://127.0.0.1:8332/",
   "BITCOIN_COOKIE_FILE=/run/bitcoind/.cookie",
   "BITCOIN_WATCH_WALLET=osl-watch",
   "MONERO_WALLET_RPC_URL=http://127.0.0.1:18088/",
   "MONERO_ACCOUNT_INDEX=0",
   "MONERO_PRIMARY_ADDRESS=" + $address,
   "CRYPTO_SETTLEMENT_CALLBACK_URL=https://keyserver.oslprivacy.com/v1/internal/crypto/settle",
   "CRYPTO_WATCHER_DB=/var/lib/osl-crypto/invoices.sqlite3",
   "CRYPTO_BTC_CONFIRMATIONS=2",
   "CRYPTO_XMR_CONFIRMATIONS=10",
   "INVOICE_RETENTION_SECONDS=604800",
   "LISTEN_ADDR=127.0.0.1:8789"] | join("\n") + "\n"
')
printf '%s' "$WATCHER_ENV" | ssh -i "$SSH_KEY" -o BatchMode=yes \
  -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n install -o root -g osl-crypto -m 0640 /dev/stdin \
    /etc/osl-crypto/watcher.env.new'
unset WATCHER_ENV

printf 'method=generate-from-view-key\nprimary_address=%s\nrestore_height=%s\ncreated_at=%s\n' \
  "$MONERO_ADDRESS" "$MONERO_RESTORE_HEIGHT" "$STAMP" | ssh \
  -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n install -o root -g osl-crypto -m 0600 /dev/stdin \
    /etc/osl-crypto/monero-view-only-creation.receipt'

ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" \
  'sudo -n systemctl daemon-reload;
   sudo -n systemctl unmask --runtime monero-wallet-rpc.service;
   sudo -n systemctl enable --now monero-wallet-rpc.service;
   for attempt in $(seq 1 60); do
     curl -fsS --max-time 2 http://127.0.0.1:18088/json_rpc \
       -H "content-type: application/json" \
       --data "{\"jsonrpc\":\"2.0\",\"id\":\"osl\",\"method\":\"get_address\",\"params\":{\"account_index\":0}}" \
       && exit 0;
     sleep 1;
   done;
   exit 1' >"$RECEIPT_DIR/monero-rpc-address-$STAMP.json"
chmod 600 "$RECEIPT_DIR/monero-rpc-address-$STAMP.json"
jq -e --arg expected "$MONERO_ADDRESS" '.result.address == $expected' \
  "$RECEIPT_DIR/monero-rpc-address-$STAMP.json" >/dev/null || {
    printf 'Monero view-only RPC did not return the pinned primary address.\n' >&2
    exit 1
  }

# Commit the staged non-secret configuration only after the view wallet has
# returned the exact pinned primary address. The same-filesystem rename is
# atomic; an existing configuration is retained as a rollback copy.
ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=yes "$VPS" '
  set -euo pipefail
  test "$(sudo -n stat -c "%a %U %G %F" /etc/osl-crypto/watcher.env.new)" = \
    "640 root osl-crypto regular file"
  sudo -n grep -Ec "^[A-Z0-9_]+=.*$" /etc/osl-crypto/watcher.env.new \
    | grep -Fxq 13
  ! sudo -n cut -d= -f1 /etc/osl-crypto/watcher.env.new \
    | sort | uniq -d | grep -q .
  sudo -n grep -Fxq "BITCOIN_COOKIE_FILE=/run/bitcoind/.cookie" \
    /etc/osl-crypto/watcher.env.new
  sudo -n grep -Fxq "CRYPTO_BTC_CONFIRMATIONS=2" \
    /etc/osl-crypto/watcher.env.new
  sudo -n grep -Fxq "CRYPTO_XMR_CONFIRMATIONS=10" \
    /etc/osl-crypto/watcher.env.new
  if sudo -n test -e /etc/osl-crypto/watcher.env; then
    sudo -n cp --preserve=mode,ownership,timestamps \
      /etc/osl-crypto/watcher.env /etc/osl-crypto/watcher.env.previous
  fi
  sudo -n mv -f /etc/osl-crypto/watcher.env.new /etc/osl-crypto/watcher.env
  sudo -n systemctl disable osl-crypto-watcher.service >/dev/null
  ! sudo -n systemctl is-active --quiet osl-crypto-watcher.service
'

printf '\nWallet boundary provisioned successfully.\n'
printf 'Local encrypted backups: %s\n' "$BACKUP_DIR"
printf 'Local public receipts: %s\n' "$RECEIPT_DIR"
printf '%s\n' \
  'The watcher and public crypto checkout remain disabled.' \
  'Next gates: offline backup, restore rehearsal, full node sync, address' \
  'matching, tiny BTC/XMR payment canaries, replay tests, then activation.'
