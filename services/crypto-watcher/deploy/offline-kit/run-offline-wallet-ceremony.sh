#!/usr/bin/env bash
set -euo pipefail
umask 077

# This launcher is intentionally offline-only. It never downloads software and
# extracts only the three executables needed by the wallet ceremony.

if [[ $- == *x* ]]; then
  set +x
  printf 'Refusing to run while shell tracing is enabled.\n' >&2
  exit 1
fi
set +x

readonly BITCOIN_ARCHIVE_NAME='bitcoin-31.1-x86_64-linux-gnu.tar.gz'
readonly BITCOIN_ARCHIVE_SHA256='b80d9c3e04da78fb6f0569685673418cf686fadba9042d926d13fb87ff503f9e'
readonly BITCOIN_BITCOIND_MEMBER='bitcoin-31.1/bin/bitcoind'
readonly BITCOIN_CLI_MEMBER='bitcoin-31.1/bin/bitcoin-cli'
readonly MONERO_ARCHIVE_NAME='monero-linux-x64-v0.18.5.1.tar.bz2'
readonly MONERO_ARCHIVE_SHA256='22a7dda7b0cb699fdd6b7674c3b4a4465b337cc98a54983523b759e1e7cc9958'
readonly MONERO_WALLET_MEMBER='monero-x86_64-linux-gnu-v0.18.5.1/monero-wallet-cli'

usage() {
  local status=${1:-64}
  cat >&2 <<EOF
Usage:
  $0 [--single-usb] [--noninteractive] \\
     --bitcoin-archive /media/path/$BITCOIN_ARCHIVE_NAME \\
     --monero-archive /media/path/$MONERO_ARCHIVE_NAME \\
     --monero-restore-height BLOCK_HEIGHT

Noninteractive mode additionally requires --backup-dir, --transfer-dir, both
--*-passphrase-file paths under /dev/shm, and
--i-accept-plaintext-recovery-credentials-on-backup-media.

Run this only as an unprivileged user on a dedicated, physically disconnected
Linux machine. The script prompts separately for new backup and transfer paths.
EOF
  exit "$status"
}

BITCOIN_ARCHIVE=
MONERO_ARCHIVE=
MONERO_RESTORE_HEIGHT=
SINGLE_USB_MODE=0
NONINTERACTIVE_MODE=0
OFFLINE_BACKUP_DIR=
WATCH_ONLY_TRANSFER_DIR=
BITCOIN_PASSPHRASE_FILE=
MONERO_PASSPHRASE_FILE=
ACCEPT_PLAINTEXT_RECOVERY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --single-usb)
      SINGLE_USB_MODE=1
      shift
      ;;
    --noninteractive)
      NONINTERACTIVE_MODE=1
      shift
      ;;
    --backup-dir)
      [[ $# -ge 2 ]] || usage
      OFFLINE_BACKUP_DIR=$2
      shift 2
      ;;
    --transfer-dir)
      [[ $# -ge 2 ]] || usage
      WATCH_ONLY_TRANSFER_DIR=$2
      shift 2
      ;;
    --bitcoin-passphrase-file)
      [[ $# -ge 2 ]] || usage
      BITCOIN_PASSPHRASE_FILE=$2
      shift 2
      ;;
    --monero-passphrase-file)
      [[ $# -ge 2 ]] || usage
      MONERO_PASSPHRASE_FILE=$2
      shift 2
      ;;
    --i-accept-plaintext-recovery-credentials-on-backup-media)
      ACCEPT_PLAINTEXT_RECOVERY=1
      shift
      ;;
    --bitcoin-archive)
      [[ $# -ge 2 ]] || usage
      BITCOIN_ARCHIVE=$2
      shift 2
      ;;
    --monero-archive)
      [[ $# -ge 2 ]] || usage
      MONERO_ARCHIVE=$2
      shift 2
      ;;
    --monero-restore-height)
      [[ $# -ge 2 ]] || usage
      MONERO_RESTORE_HEIGHT=$2
      shift 2
      ;;
    -h|--help)
      usage 0
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "$BITCOIN_ARCHIVE" && -n "$MONERO_ARCHIVE" && \
   "$MONERO_RESTORE_HEIGHT" =~ ^[0-9]+$ && "$MONERO_RESTORE_HEIGHT" -gt 2000 ]] || usage
if [[ "$NONINTERACTIVE_MODE" -eq 1 ]]; then
  [[ -n "$OFFLINE_BACKUP_DIR" && -n "$WATCH_ONLY_TRANSFER_DIR" && \
     -n "$BITCOIN_PASSPHRASE_FILE" && -n "$MONERO_PASSPHRASE_FILE" && \
     "$ACCEPT_PLAINTEXT_RECOVERY" -eq 1 ]] || usage
else
  [[ -z "$OFFLINE_BACKUP_DIR" && -z "$WATCH_ONLY_TRANSFER_DIR" && \
     -z "$BITCOIN_PASSPHRASE_FILE" && -z "$MONERO_PASSPHRASE_FILE" && \
     "$ACCEPT_PLAINTEXT_RECOVERY" -eq 0 ]] || usage
fi
[[ ${EUID:-$(id -u)} -ne 0 ]] || {
  printf 'Run this launcher as a dedicated, unprivileged offline user, not root.\n' >&2
  exit 1
}

for command in awk cat chmod dirname id mkdir mktemp readlink rm sha256sum stat tar; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
CEREMONY_SCRIPT=$(readlink -f -- "$SCRIPT_DIR/../generate-offline-merchant-wallets.sh")
[[ -f "$CEREMONY_SCRIPT" && ! -L "$CEREMONY_SCRIPT" && -x "$CEREMONY_SCRIPT" ]] || {
  printf 'The audited wallet ceremony is missing or not executable.\n' >&2
  exit 1
}

# The existing ceremony is the single source of truth for the offline, no-swap,
# no-hibernation preflight. Run it before reading or extracting either archive.
"$CEREMONY_SCRIPT" --preflight

canonical_archive() {
  local label=$1 supplied=$2 expected_name=$3 destination=$4 canonical
  [[ -f "$supplied" && ! -L "$supplied" ]] || {
    printf '%s must be a regular archive file, not a symlink: %s\n' "$label" "$supplied" >&2
    exit 1
  }
  canonical=$(readlink -f -- "$supplied")
  [[ -n "$canonical" && ${canonical##*/} == "$expected_name" ]] || {
    printf '%s must have the official filename %s.\n' "$label" "$expected_name" >&2
    exit 1
  }
  printf -v "$destination" '%s' "$canonical"
}

canonical_archive Bitcoin "$BITCOIN_ARCHIVE" "$BITCOIN_ARCHIVE_NAME" BITCOIN_ARCHIVE
canonical_archive Monero "$MONERO_ARCHIVE" "$MONERO_ARCHIVE_NAME" MONERO_ARCHIVE

verify_archive() {
  local label=$1 archive=$2 expected_hash=$3 actual_hash
  actual_hash=$(sha256sum -- "$archive" | awk '{print $1}')
  [[ "$actual_hash" == "$expected_hash" ]] || {
    printf '%s archive SHA-256 does not match the hard-coded official release hash.\n' "$label" >&2
    exit 1
  }
}

verify_archive Bitcoin "$BITCOIN_ARCHIVE" "$BITCOIN_ARCHIVE_SHA256"
verify_archive Monero "$MONERO_ARCHIVE" "$MONERO_ARCHIVE_SHA256"

[[ -d /dev/shm && $(stat -f -c '%T' /dev/shm) == tmpfs ]] || {
  printf 'A tmpfs-backed /dev/shm is required.\n' >&2
  exit 1
}

SCRATCH_DIR=$(mktemp -d /dev/shm/osl-offline-kit.XXXXXX)
cleanup() {
  local status=$?
  set +e
  if [[ -n ${SCRATCH_DIR:-} && "$SCRATCH_DIR" == /dev/shm/osl-offline-kit.* ]]; then
    rm -rf -- "$SCRATCH_DIR"
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -m 0700 -- "$SCRATCH_DIR/bin"

require_exact_member_once() {
  local label=$1 archive=$2 compression=$3 member=$4 count
  if [[ "$compression" == gzip ]]; then
    count=$(tar --list --gzip --file="$archive" | awk -v wanted="$member" '$0 == wanted { count++ } END { print count + 0 }')
  else
    count=$(tar --list --bzip2 --file="$archive" | awk -v wanted="$member" '$0 == wanted { count++ } END { print count + 0 }')
  fi
  [[ "$count" -eq 1 ]] || {
    printf '%s archive does not contain exactly one expected %s member.\n' "$label" "$member" >&2
    exit 1
  }
}

require_exact_member_once Bitcoin "$BITCOIN_ARCHIVE" gzip "$BITCOIN_BITCOIND_MEMBER"
require_exact_member_once Bitcoin "$BITCOIN_ARCHIVE" gzip "$BITCOIN_CLI_MEMBER"
require_exact_member_once Monero "$MONERO_ARCHIVE" bzip2 "$MONERO_WALLET_MEMBER"

# Exact member names plus --strip-components prevent the rest of either archive
# from being unpacked. Ownership and archive permissions are never restored.
tar --extract --gzip --file="$BITCOIN_ARCHIVE" --directory="$SCRATCH_DIR/bin" \
  --strip-components=2 --no-same-owner --no-same-permissions -- \
  "$BITCOIN_BITCOIND_MEMBER" "$BITCOIN_CLI_MEMBER"
tar --extract --bzip2 --file="$MONERO_ARCHIVE" --directory="$SCRATCH_DIR/bin" \
  --strip-components=1 --no-same-owner --no-same-permissions -- \
  "$MONERO_WALLET_MEMBER"

BITCOIND_BIN="$SCRATCH_DIR/bin/bitcoind"
BITCOIN_CLI_BIN="$SCRATCH_DIR/bin/bitcoin-cli"
MONERO_WALLET_CLI_BIN="$SCRATCH_DIR/bin/monero-wallet-cli"
for binary in "$BITCOIND_BIN" "$BITCOIN_CLI_BIN" "$MONERO_WALLET_CLI_BIN"; do
  [[ -f "$binary" && ! -L "$binary" ]] || {
    printf 'Expected extracted executable is missing or unsafe: %s\n' "$binary" >&2
    exit 1
  }
  chmod 0500 -- "$binary"
done

# These executable hashes are pinned to the already verified official archives
# for this invocation and are independently rechecked by the ceremony.
BITCOIND_BIN_SHA256=$(sha256sum -- "$BITCOIND_BIN" | awk '{print $1}')
BITCOIN_CLI_BIN_SHA256=$(sha256sum -- "$BITCOIN_CLI_BIN" | awk '{print $1}')
MONERO_WALLET_CLI_BIN_SHA256=$(sha256sum -- "$MONERO_WALLET_CLI_BIN" | awk '{print $1}')

read_tty() {
  local prompt=$1 destination=$2 value
  [[ -r /dev/tty ]] || {
    printf 'An interactive terminal is required.\n' >&2
    exit 1
  }
  printf '%s' "$prompt" >/dev/tty
  IFS= read -r value </dev/tty
  printf -v "$destination" '%s' "$value"
}

require_new_canonical_directory() {
  local label=$1 supplied=$2 destination=$3 parent canonical_parent canonical_target
  [[ "$supplied" == /* ]] || {
    printf '%s must be an absolute path.\n' "$label" >&2
    exit 1
  }
  [[ ! -e "$supplied" && ! -L "$supplied" ]] || {
    printf '%s must be a new path that does not exist: %s\n' "$label" "$supplied" >&2
    exit 1
  }
  parent=$(dirname -- "$supplied")
  [[ -d "$parent" && ! -L "$parent" ]] || {
    printf '%s parent must be an existing non-symlink directory.\n' "$label" >&2
    exit 1
  }
  canonical_parent=$(readlink -f -- "$parent")
  canonical_target=$(readlink -m -- "$supplied")
  [[ "$canonical_target" == "$supplied" && "$(dirname -- "$canonical_target")" == "$canonical_parent" ]] || {
    printf '%s must already be canonical and contain no symlinked or dot components.\n' "$label" >&2
    exit 1
  }
  case "$canonical_target" in
    /dev|/dev/*|/proc|/proc/*|/run|/run/*|/sys|/sys/*|/tmp|/tmp/*)
      printf '%s must be on durable offline storage.\n' "$label" >&2
      exit 1
      ;;
  esac
  printf -v "$destination" '%s' "$canonical_target"
}

if [[ "$NONINTERACTIVE_MODE" -eq 1 ]]; then
  require_new_canonical_directory 'Backup directory' "$OFFLINE_BACKUP_DIR" OFFLINE_BACKUP_DIR
  require_new_canonical_directory 'Transfer directory' "$WATCH_ONLY_TRANSFER_DIR" WATCH_ONLY_TRANSFER_DIR
else
  read_tty 'New encrypted-backup directory on durable offline media: ' BACKUP_INPUT
  require_new_canonical_directory 'Backup directory' "$BACKUP_INPUT" OFFLINE_BACKUP_DIR
  unset BACKUP_INPUT
  read_tty 'New watch-only transfer directory on separate removable media: ' TRANSFER_INPUT
  require_new_canonical_directory 'Transfer directory' "$TRANSFER_INPUT" WATCH_ONLY_TRANSFER_DIR
  unset TRANSFER_INPUT
fi

[[ "$OFFLINE_BACKUP_DIR" != "$WATCH_ONLY_TRANSFER_DIR" && \
   "$OFFLINE_BACKUP_DIR" != "$WATCH_ONLY_TRANSFER_DIR"/* && \
   "$WATCH_ONLY_TRANSFER_DIR" != "$OFFLINE_BACKUP_DIR"/* ]] || {
  printf 'Backup and transfer directories must be separate and non-nested.\n' >&2
  exit 1
}
if [[ $(stat -c '%d' "$(dirname -- "$OFFLINE_BACKUP_DIR")") == \
      $(stat -c '%d' "$(dirname -- "$WATCH_ONLY_TRANSFER_DIR")") && \
      "$SINGLE_USB_MODE" -ne 1 ]]; then
  printf 'Backup and transfer directories must be on distinct filesystems.\n' >&2
  exit 1
fi

printf '\nArchives verified. Starting the audited offline wallet ceremony.\n'
BITCOIND_BIN="$BITCOIND_BIN" \
BITCOIN_CLI_BIN="$BITCOIN_CLI_BIN" \
MONERO_WALLET_CLI_BIN="$MONERO_WALLET_CLI_BIN" \
BITCOIND_BIN_SHA256="$BITCOIND_BIN_SHA256" \
BITCOIN_CLI_BIN_SHA256="$BITCOIN_CLI_BIN_SHA256" \
MONERO_WALLET_CLI_BIN_SHA256="$MONERO_WALLET_CLI_BIN_SHA256" \
MONERO_RESTORE_HEIGHT="$MONERO_RESTORE_HEIGHT" \
OFFLINE_BACKUP_DIR="$OFFLINE_BACKUP_DIR" \
WATCH_ONLY_TRANSFER_DIR="$WATCH_ONLY_TRANSFER_DIR" \
OSL_SINGLE_USB_MODE="$SINGLE_USB_MODE" \
OSL_NONINTERACTIVE_MODE="$NONINTERACTIVE_MODE" \
OSL_BITCOIN_PASSPHRASE_FILE="$BITCOIN_PASSPHRASE_FILE" \
OSL_MONERO_PASSPHRASE_FILE="$MONERO_PASSPHRASE_FILE" \
OSL_ACCEPT_PLAINTEXT_RECOVERY="$ACCEPT_PLAINTEXT_RECOVERY" \
  "$CEREMONY_SCRIPT"
