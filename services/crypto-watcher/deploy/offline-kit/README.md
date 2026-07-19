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

Record the current Monero mainnet block height before disconnecting. Do not put
wallet backups on the same removable device as the watch-only transfer files.

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

Success is defined only by the ceremony's `CEREMONY-COMPLETE` receipts. Any
`CEREMONY-INCOMPLETE` directory is sensitive, unusable, and must be quarantined.
Never move the encrypted spending-wallet backup to the VPS or an online PC.

Run the static checks with:

```sh
./test-offline-kit.sh
```
