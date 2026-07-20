#!/usr/bin/env bash
set -euo pipefail
umask 077

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
IMPORTER="$SCRIPT_DIR/import-local-watch-only-wallets.sh"
[[ -x "$IMPORTER" ]] || chmod +x "$IMPORTER"

TMP_ROOT=$(mktemp -d)
cleanup() { rm -rf -- "$TMP_ROOT"; }
trap cleanup EXIT

make_fixture() {
  local root=$1
  mkdir -p "$root/home/.local/opt/osl-crypto/bin" \
    "$root/home/.local/share/osl-crypto/bitcoin" \
    "$root/home/.config/osl-crypto/secrets" "$root/bin" "$root/transfer"
  printf 'wallet-password-for-tests\n' >"$root/home/.config/osl-crypto/secrets/monero-wallet-password"
  chmod 0600 "$root/home/.config/osl-crypto/secrets/monero-wallet-password"

  cat >"$root/home/.local/opt/osl-crypto/bin/bitcoin-cli" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
command_name=
for arg in "$@"; do
  case "$arg" in getblockchaininfo|getwalletinfo|getdescriptorinfo|listdescriptors|importdescriptors|backupwallet) command_name=$arg ;; esac
done
state="$HOME/.local/share/osl-crypto/bitcoin/test-descriptor"
case "$command_name" in
  getblockchaininfo) printf '%s\n' '{"chain":"main","initialblockdownload":false,"blocks":10,"headers":10}' ;;
  getwalletinfo) printf '%s\n' '{"private_keys_enabled":false,"descriptors":true,"txcount":0}' ;;
  getdescriptorinfo) read -r _descriptor; printf '%s\n' '{"hasprivatekeys":false,"isrange":true}' ;;
  listdescriptors)
    if [[ -f "$state" ]]; then
      jq -cn --arg d "$(<"$state")" --argjson next "${BTC_TEST_NEXT_INDEX:-0}" \
        '{descriptors:[{desc:$d,active:true,internal:false,range:[0,99999],next:$next}]}'
    else
      printf '%s\n' '{"descriptors":[]}'
    fi
    ;;
  importdescriptors)
    payload=$(cat)
    jq -r '.[0].desc' <<<"$payload" >"$state"
    printf '%s\n' '[{"success":true}]'
    ;;
  backupwallet)
    target=${!#}
    printf 'watch-only-backup\n' >"$target"
    ;;
  *) printf 'unexpected bitcoin-cli invocation\n' >&2; exit 1 ;;
esac
FAKE
  chmod 0700 "$root/home/.local/opt/osl-crypto/bin/bitcoin-cli"

  cat >"$root/home/.local/opt/osl-crypto/bin/monero-wallet-cli" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
wallet=
while [[ $# -gt 0 ]]; do
  if [[ $1 == --generate-from-view-key ]]; then wallet=$2; shift 2; else shift; fi
done
IFS= read -r address
IFS= read -r _view_key
printf '%s\n' "$address" >"$HOME/test-xmr-address"
printf 'view-only-wallet\n' >"$wallet"
printf 'view-only-keys\n' >"$wallet.keys"
FAKE
  chmod 0700 "$root/home/.local/opt/osl-crypto/bin/monero-wallet-cli"

  cat >"$root/bin/systemctl" <<'FAKE'
#!/usr/bin/env bash
if [[ " $* " == *' is-active '* ]]; then exit 3; fi
exit 0
FAKE
  cat >"$root/bin/sleep" <<'FAKE'
#!/usr/bin/env bash
exit 0
FAKE
  cat >"$root/bin/curl" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *'/get_info'* ]]; then
  printf '%s\n' '{"nettype":"mainnet","synchronized":true,"height":5000,"target_height":5000}'
else
  if [[ ${FAIL_WALLET_RPC:-0} == 1 ]]; then printf '%s\n' '{"error":"not ready"}'; exit 0; fi
  jq -cn --arg a "$(<"$HOME/test-xmr-address")" '{result:{address:$a}}'
fi
FAKE
  chmod 0700 "$root/bin/systemctl" "$root/bin/curl" "$root/bin/sleep"

  descriptor='wpkh([01234567/84h/0h/0h]xpub661MyMwAqRbcFakePublicOnlyKey/0/*)#abcd1234'
  address="4$(printf 'A%.0s' $(seq 1 94))"
  printf '%s\n' "$descriptor" >"$root/transfer/btc-descriptor"
  printf '%s\n' "$address" >"$root/transfer/xmr-address"
  printf '%064d\n' 0 >"$root/transfer/xmr-view-key"
  printf '3000\n' >"$root/transfer/xmr-restore-height"
  chmod 0600 "$root/transfer/"*
  (
    cd "$root/transfer"
    sha256sum btc-descriptor xmr-address xmr-view-key xmr-restore-height >SHA256SUMS
    chmod 0600 SHA256SUMS
  )
  transfer_hash=$(sha256sum "$root/transfer/SHA256SUMS" | awk '{print $1}')
  printf 'ceremony_version=1\nbackup_manifest_sha256=%064d\ntransfer_manifest_sha256=%s\n' \
    0 "$transfer_hash" >"$root/transfer/CEREMONY-COMPLETE"
  chmod 0600 "$root/transfer/CEREMONY-COMPLETE"
}

run_import() {
  local root=$1
  HOME="$root/home" PATH="$root/bin:$PATH" \
  BTC_PUBLIC_DESCRIPTOR_FILE="$root/transfer/btc-descriptor" \
  BTC_DESCRIPTOR_IS_DEDICATED_UNUSED=true \
  MONERO_PRIMARY_ADDRESS_FILE="$root/transfer/xmr-address" \
  MONERO_PRIVATE_VIEW_KEY_FILE="$root/transfer/xmr-view-key" \
  OFFLINE_CEREMONY_RECEIPT_FILE="$root/transfer/CEREMONY-COMPLETE" \
  OFFLINE_CEREMONY_SHA256SUMS_FILE="$root/transfer/SHA256SUMS" \
  MONERO_RESTORE_HEIGHT=3000 "$IMPORTER"
}

SUCCESS="$TMP_ROOT/success"
make_fixture "$SUCCESS"
output=$(run_import "$SUCCESS")
[[ "$output" != *xpub* && "$output" != *"$(<"$SUCCESS/transfer/xmr-view-key")"* ]]
receipt="$SUCCESS/home/.config/osl-crypto/local-watch-only-import.receipt"
[[ -f "$receipt" && $(stat -c '%a' "$receipt") == 600 ]]
grep -qx 'method=local-watch-only' "$receipt"
[[ -f "$SUCCESS/home/.local/share/osl-crypto/wallets/osl-view-only" ]]
[[ -f "$SUCCESS/home/.local/share/osl-crypto/wallets/osl-view-only.keys" ]]
[[ -f "$SUCCESS/home/.local/share/osl-crypto/bitcoin/test-descriptor" ]]

# Exact replay is idempotent and remains bound to the same ceremony manifest.
receipt_hash_before=$(sha256sum "$receipt" | awk '{print $1}')
BTC_TEST_NEXT_INDEX=2 run_import "$SUCCESS" >/dev/null
[[ $(sha256sum "$receipt" | awk '{print $1}') == "$receipt_hash_before" ]]
printf 'unexpected=field\n' >>"$receipt"
if run_import "$SUCCESS" >"$SUCCESS/replay-out" 2>"$SUCCESS/replay-err"; then
  printf 'invalid provenance receipt unexpectedly accepted\n' >&2
  exit 1
fi
grep -q 'provenance receipt has an invalid schema' "$SUCCESS/replay-err"

TAMPER="$TMP_ROOT/tamper"
make_fixture "$TAMPER"
printf 'tampered\n' >>"$TAMPER/transfer/xmr-address"
if run_import "$TAMPER" >"$TAMPER/out" 2>"$TAMPER/err"; then
  printf 'tampered manifest unexpectedly imported\n' >&2
  exit 1
fi
grep -q 'manifest verification failed' "$TAMPER/err"
[[ ! -e "$TAMPER/home/.local/share/osl-crypto/bitcoin/test-descriptor" ]]

INCOMPLETE="$TMP_ROOT/incomplete"
make_fixture "$INCOMPLETE"
: >"$INCOMPLETE/transfer/CEREMONY-INCOMPLETE"
if run_import "$INCOMPLETE" >"$INCOMPLETE/out" 2>"$INCOMPLETE/err"; then
  printf 'incomplete ceremony unexpectedly imported\n' >&2
  exit 1
fi
grep -q 'explicitly marked incomplete' "$INCOMPLETE/err"

PRIVATE="$TMP_ROOT/private"
make_fixture "$PRIVATE"
printf 'wpkh(tprvSecret/0/*)#abcd1234\n' >"$PRIVATE/transfer/btc-descriptor"
(
  cd "$PRIVATE/transfer"
  sha256sum btc-descriptor xmr-address xmr-view-key xmr-restore-height >SHA256SUMS
  transfer_hash=$(sha256sum SHA256SUMS | awk '{print $1}')
  printf 'ceremony_version=1\nbackup_manifest_sha256=%064d\ntransfer_manifest_sha256=%s\n' \
    0 "$transfer_hash" >CEREMONY-COMPLETE
  chmod 0600 SHA256SUMS CEREMONY-COMPLETE btc-descriptor
)
if run_import "$PRIVATE" >"$PRIVATE/out" 2>"$PRIVATE/err"; then
  printf 'private descriptor unexpectedly imported\n' >&2
  exit 1
fi
grep -q 'Refusing Bitcoin private descriptor material' "$PRIVATE/err"

ROLLBACK="$TMP_ROOT/rollback"
make_fixture "$ROLLBACK"
if FAIL_WALLET_RPC=1 run_import "$ROLLBACK" >"$ROLLBACK/out" 2>"$ROLLBACK/err"; then
  printf 'unverified Monero RPC unexpectedly completed import\n' >&2
  exit 1
fi
grep -q 'did not return the pinned primary address' "$ROLLBACK/err"
[[ ! -e "$ROLLBACK/home/.local/share/osl-crypto/wallets/osl-view-only" ]]
[[ ! -e "$ROLLBACK/home/.local/share/osl-crypto/wallets/osl-view-only.keys" ]]
[[ ! -e "$ROLLBACK/home/.config/osl-crypto/local-watch-only-import.receipt" ]]
if find "$ROLLBACK/home/.local/share/osl-crypto/watch-wallet-backups" -type f \
    -print -quit 2>/dev/null | grep -q .; then
  printf 'failed import left a Monero backup artifact\n' >&2
  exit 1
fi
# The exact public Bitcoin import is deliberately resumable and non-secret.
[[ -f "$ROLLBACK/home/.local/share/osl-crypto/bitcoin/test-descriptor" ]]

printf 'local watch-only importer tests passed\n'
