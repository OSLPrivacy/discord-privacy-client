#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
LAUNCHER="$SCRIPT_DIR/run-offline-wallet-ceremony.sh"

fail() {
  printf 'offline-kit test failed: %s\n' "$1" >&2
  exit 1
}

bash -n "$LAUNCHER"

grep -Fq "BITCOIN_ARCHIVE_SHA256='b80d9c3e04da78fb6f0569685673418cf686fadba9042d926d13fb87ff503f9e'" "$LAUNCHER" \
  || fail 'Bitcoin Core 31.1 archive hash is not pinned'
grep -Fq "MONERO_ARCHIVE_SHA256='22a7dda7b0cb699fdd6b7674c3b4a4465b337cc98a54983523b759e1e7cc9958'" "$LAUNCHER" \
  || fail 'Monero 0.18.5.1 archive hash is not pinned'

grep -Fq "BITCOIN_BITCOIND_MEMBER='bitcoin-31.1/bin/bitcoind'" "$LAUNCHER" \
  || fail 'bitcoind archive member is not exact'
grep -Fq "BITCOIN_CLI_MEMBER='bitcoin-31.1/bin/bitcoin-cli'" "$LAUNCHER" \
  || fail 'bitcoin-cli archive member is not exact'
grep -Fq "MONERO_WALLET_MEMBER='monero-x86_64-linux-gnu-v0.18.5.1/monero-wallet-cli'" "$LAUNCHER" \
  || fail 'monero-wallet-cli archive member is not exact'

if grep -Eq '(^|[[:space:]])(curl|wget|aria2c|ftp|scp|rsync)([[:space:]]|$)' "$LAUNCHER"; then
  fail 'launcher contains a network or download command'
fi
if grep -Eq '(xprv|private spend|mnemonic|seed phrase)' "$LAUNCHER"; then
  fail 'launcher should not handle wallet secrets itself'
fi

preflight_line=$(grep -nF '"$CEREMONY_SCRIPT" --preflight' "$LAUNCHER" | cut -d: -f1)
first_hash_line=$(grep -nF 'verify_archive Bitcoin' "$LAUNCHER" | cut -d: -f1)
first_extract_line=$(grep -nF 'tar --extract --gzip' "$LAUNCHER" | cut -d: -f1)
[[ -n "$preflight_line" && -n "$first_hash_line" && -n "$first_extract_line" ]] \
  || fail 'preflight, verification, or extraction step missing'
[[ "$preflight_line" -lt "$first_hash_line" && "$first_hash_line" -lt "$first_extract_line" ]] \
  || fail 'offline preflight must precede verification and extraction'

grep -Fq '$(stat -c '\''%d'\'' "$(dirname -- "$OFFLINE_BACKUP_DIR")") !=' "$LAUNCHER" \
  || fail 'distinct-filesystem check is missing'
grep -Fq 'BITCOIND_BIN_SHA256="$BITCOIND_BIN_SHA256"' "$LAUNCHER" \
  || fail 'bitcoind hash is not passed explicitly'
grep -Fq 'BITCOIN_CLI_BIN_SHA256="$BITCOIN_CLI_BIN_SHA256"' "$LAUNCHER" \
  || fail 'bitcoin-cli hash is not passed explicitly'
grep -Fq 'MONERO_WALLET_CLI_BIN_SHA256="$MONERO_WALLET_CLI_BIN_SHA256"' "$LAUNCHER" \
  || fail 'monero-wallet-cli hash is not passed explicitly'

printf 'Offline wallet kit static checks passed.\n'
