# Offline merchant-wallet kit

This directory is a convenience launcher for the audited
`../generate-offline-merchant-wallets.sh` ceremony. It does not download
software, contact a network, deploy anything, or copy spending material to the
watch-only transfer directory.

## Prepare while online

On a separate staging machine, download these two exact official archives and
copy them to removable media:

- `bitcoin-31.1-x86_64-linux-gnu.tar.gz` from
  `https://bitcoincore.org/bin/bitcoin-core-31.1/`
- `monero-linux-x64-v0.18.5.1.tar.bz2` from
  `https://www.getmonero.org/downloads/`

The launcher hard-codes the official archive SHA-256 values from Bitcoin
Core's `SHA256SUMS` and Monero's signed `downloads/hashes.txt`. For stronger
supply-chain assurance, verify those signed checksum files independently before
moving the archives offline.

Record the current Monero mainnet block height before disconnecting. Two
devices remain the recommended layout: do not put wallet backups on the same
removable device as the watch-only transfer files unless you deliberately use
the single-USB mode described below.

## Run while physically offline

Use a dedicated Linux machine. Disable Wi-Fi, unplug Ethernet, disable swap and
hibernation, and keep the machine offline permanently after it holds wallet
seeds. Mount two different storage devices: one for encrypted backups and one
for the watch-only transfer bundle.

Run as an unprivileged user:

```sh
./run-offline-wallet-ceremony.sh \
  --bitcoin-archive /media/releases/bitcoin-31.1-x86_64-linux-gnu.tar.gz \
  --monero-archive /media/releases/monero-linux-x64-v0.18.5.1.tar.bz2 \
  --monero-restore-height 3720000
```

The launcher first invokes the ceremony's network, swap, and hibernation
preflight. It verifies both full archives, extracts only `bitcoind`,
`bitcoin-cli`, and `monero-wallet-cli` into `/dev/shm`, hashes those three
binaries, and passes the explicit hashes to the ceremony. It then prompts for
new canonical backup and transfer directories and requires their parents to be
on distinct filesystems.

### Explicit single-USB mode

If only one removable device is available, two separate, non-nested directories
on that device can be used by adding `--single-usb`:

```sh
./run-offline-wallet-ceremony.sh --single-usb \
  --bitcoin-archive /media/usb/releases/bitcoin-31.1-x86_64-linux-gnu.tar.gz \
  --monero-archive /media/usb/releases/monero-linux-x64-v0.18.5.1.tar.bz2 \
  --monero-restore-height 3720000
```

The default remains two distinct filesystems. The flag relaxes only that one
check, and only after the operator types the displayed acknowledgement exactly.
The directories must still be new, canonical, separate, and non-nested. All
offline, no-swap, no-hibernation, non-root, archive-hash, tmpfs, encryption, and
receipt checks remain in force. A single device is one failure domain: loss or
damage can remove both copies, and connecting it elsewhere exposes the encrypted
backup files as well as the watch-only bundle.

On WSL, the host may populate `/proc/swaps` even when swap is disabled for the
ceremony. That state is accepted only when the running process is in a cgroup
whose `memory.swap.max` (the cgroup v2 form of `MemorySwapMax`) is exactly `0`.
This does not relax any network/offline check.

### Explicit noninteractive mode

The interactive behavior remains the default. For a launcher that already has
the operator's explicit authorization, add all of the following:

```sh
--noninteractive \
--backup-dir /media/usb/OSL-ENCRYPTED-WALLET-BACKUP \
--transfer-dir /media/usb/OSL-WATCH-ONLY-TRANSFER \
--bitcoin-passphrase-file /dev/shm/osl-bitcoin-passphrase \
--monero-passphrase-file /dev/shm/osl-monero-passphrase \
--i-accept-plaintext-recovery-credentials-on-backup-media
```

Also retain `--single-usb` when the two output directories share a filesystem.
Each passphrase file must be canonical, user-owned, mode 0600, tmpfs-backed,
and contain exactly one line of at least 16 characters. This mode never prints
either passphrase or the Monero seed. After all wallet restore checks, it
fsyncs mode-0600 `OSL-RECOVERY-CREDENTIALS.txt` in the backup directory and
includes it in `BACKUP-SHA256SUMS`. This is plaintext spending material on the
USB and is intentionally gated by the long acceptance flag above.

Success is defined only by the ceremony's `CEREMONY-COMPLETE` receipts. Any
`CEREMONY-INCOMPLETE` directory is sensitive, unusable, and must be quarantined.
Never move the encrypted spending-wallet backup to the VPS or an online PC.

Run the static checks with:

```sh
./test-offline-kit.sh
```
