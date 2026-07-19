# OSL crypto watcher

An isolated, self-hosted payment observer for anonymous BTC and XMR invoices.
It creates a unique receiving address for every invoice, observes payments
through locally bound wallet RPCs, and signs an idempotent settlement callback
to the Cloudflare keyserver with a watcher-only Ed25519 key. It has no wallet
send, transfer, transaction-signing, refund, or withdrawal code.

## Wallet boundary

- Bitcoin Core must use a descriptor wallet created with
  `disable_private_keys=true`, `blank=true`, and `descriptors=true`. Import only
  a checksummed external ranged descriptor derived from an xpub. The service
  verifies `private_keys_enabled=false` before listening.
- `monero-wallet-rpc` must open a view-only wallet and bind only to
  `127.0.0.1`. The watcher calls only
  `get_version`, `get_address`, `create_address`, and `get_transfers`.
- Pin the wallet's public primary address in `MONERO_PRIMARY_ADDRESS`. Startup
  fails before listening if account 0 does not return that exact address. This
  detects the wrong wallet file without requesting, storing, or logging any
  private view or spend key.
- **Remaining go-live gate:** Monero Wallet RPC does not expose a documented,
  non-secret `view_only` boolean. Its `query_key` method returns private keys,
  while probing `sign` or transfer methods would actively use spend authority.
  The watcher therefore does not pretend to prove view-only status through the
  current RPC. Provision the wallet from a public address plus private view key
  only, retain the creation receipt, and independently inspect the wallet before
  enabling public XMR checkout. Primary-address pinning detects the wrong file,
  but it is not proof that the file lacks a spend key.
- Keep the Bitcoin xprv/seed and Monero private spend key offline. They are not
  deployment secrets and must never be copied to this host.

Generate and back up both spending wallets offline. The supported ceremony is
`deploy/generate-offline-merchant-wallets.sh`. It refuses root, shell tracing,
default IPv4/IPv6 routes, and every UP interface except loopback. It also
refuses swap or a configured hibernation resume device, requires independently
pinned hashes for the Bitcoin and Monero wallet binaries, and requires exact
interactive confirmations for the recovery backups. Never run it on this
workstation or on the VPS.

For a guided, offline-only launcher that verifies exact Bitcoin Core 31.1 and
Monero 0.18.5.1 release archives, extracts only the required wallet binaries to
`/dev/shm`, and prompts for separate backup media, see
`deploy/offline-kit/README.md`. The kit is only a wrapper around this same
ceremony and contains no download or deployment behavior.

On a dedicated, physically disconnected offline machine, first record the
current Monero mainnet height and independently verify the binary hashes. Then
run:

```sh
BITCOIND_BIN=/offline/bin/bitcoind \
BITCOIN_CLI_BIN=/offline/bin/bitcoin-cli \
MONERO_WALLET_CLI_BIN=/offline/bin/monero-wallet-cli \
BITCOIND_BIN_SHA256=... \
BITCOIN_CLI_BIN_SHA256=... \
MONERO_WALLET_CLI_BIN_SHA256=... \
MONERO_RESTORE_HEIGHT=... \
OFFLINE_BACKUP_DIR=/offline-backup/osl-merchant-wallets \
WATCH_ONLY_TRANSFER_DIR=/removable-transfer/osl-watch-material \
  ./deploy/generate-offline-merchant-wallets.sh
```

The ceremony leaves encrypted Bitcoin and Monero wallets only in the offline
backup directory. Passphrases and the temporary Monero recovery transcript are
handled only in a verified `/dev/shm` tmpfs. It shows the seed only on the
offline terminal, requires the operator to confirm the seed and passphrases
were stored durably, and then removes the volatile transcript. The separate
transfer directory contains only:

- `btc-descriptor`: checksummed external ranged public descriptor;
- `xmr-address`: public Monero primary address;
- `xmr-view-key`: Monero private view key, which cannot spend funds;
- `xmr-restore-height`; and
- `SHA256SUMS` for transfer verification; and
- `CEREMONY-COMPLETE`, an atomic receipt binding both backup and transfer
  manifests.

The Bitcoin wallet is encrypted at creation, not after keys have already been
written. Before reporting success, the ceremony restores the encrypted Bitcoin
backup under a temporary wallet name and reopens a temporary copy of the Monero
backup, then proves that both reproduce the exact exported public/view
material and mnemonic seed. The backup and transfer paths must be on distinct
filesystems. No watch/view files are written until the operator confirms the
second durable offline backup.

Both output directories begin with `CEREMONY-INCOMPLETE`. Any interruption or
failed check leaves that marker in place. Do not import, fund, merge, or reuse a
partial ceremony. Quarantine it as sensitive material and rerun from the start
with two new empty directories. Success atomically creates
`CEREMONY-COMPLETE` beside the encrypted backups, containing hashes of both
manifests, then removes the incomplete markers. Verify that receipt before
moving the transfer media to an online administrative machine.

Do not add a BTC xprv, Monero spend key, recovery seed, wallet file, password,
or unrelated secret to the transfer media. Power down and securely store the
offline machine after making and verifying a second physically separate backup.

The watcher begins allocating the dedicated BTC descriptor at index zero and
imports it with the current timestamp. Move only the ceremony's mode-0600
watch/view files to the online administrative workstation, verify `SHA256SUMS`,
and run:

```sh
BTC_PUBLIC_DESCRIPTOR_FILE=/secure-transfer/btc-descriptor \
BTC_DESCRIPTOR_IS_DEDICATED_UNUSED=true \
MONERO_PRIMARY_ADDRESS_FILE=/secure-transfer/xmr-address \
MONERO_PRIVATE_VIEW_KEY_FILE=/secure-transfer/xmr-view-key \
OFFLINE_CEREMONY_RECEIPT_FILE=/secure-transfer/CEREMONY-COMPLETE \
OFFLINE_CEREMONY_SHA256SUMS_FILE=/secure-transfer/SHA256SUMS \
MONERO_RESTORE_HEIGHT=... \
OSL_PAYMENTS_VPS=osladmin@payments-host \
OSL_PAYMENTS_HOST_KEY_SHA256=SHA256:... \
  ./deploy/provision-watch-only-wallets.sh
```

The legacy `provision-new-merchant-wallets.sh` now refuses to run because
creating spending keys on an internet-connected workstation violates this
service's trust boundary. The import ceremony pins the exact SSH host key,
holds a remote provisioning lock, validates both synchronized nodes before
stopping services, and records hashes of the imported material. If BTC import
succeeds but a later stage fails, a rerun may continue only when Bitcoin Core
contains that exact descriptor. Newly created partial XMR files are removed and
the prior service state is restored on failure.

The online importer refuses a transfer directory that still contains
`CEREMONY-INCOMPLETE`. Before reading any descriptor or view key, it requires
mode-0600, non-symlink inputs with the exact ceremony filenames, validates the
three-line completion-receipt schema, checks the manifest hash bound into that
receipt, runs `sha256sum -c` over the exact four watch/view files, and confirms
the bundled Monero restore height matches the requested value.

After the import-only ceremony, the VPS runs its view-only RPC locally:

```sh
monero-wallet-rpc --wallet-file /var/lib/osl-crypto/wallets/osl-view-only \
  --password-file /etc/osl-crypto/monero-wallet-password \
  --daemon-address 127.0.0.1:18081 --rpc-bind-ip 127.0.0.1 \
  --rpc-bind-port 18088 --disable-rpc-login
```

`--disable-rpc-login` is acceptable here only because the watcher enforces a
loopback RPC URL and the process is isolated on the same host. Never expose
either wallet RPC through a tunnel, public interface, container port, or proxy.
Current Monero releases deny `create_address` and `get_transfers` in
`--restricted-rpc` mode, so that flag is incompatible with unique-address
invoice observation. Safety instead comes from the loopback-only RPC, the
operator-pinned view-only wallet, and the complete absence of transfer/signing
calls in this service.

## Required configuration

The systemd process reads these environment variables from a root-owned file:

| Variable | Example / meaning |
|---|---|
| `CRYPTO_BTC_ENABLED` | Exact `true` enables BTC; absent or `false` keeps it unavailable |
| `BITCOIN_RPC_URL` | `http://127.0.0.1:8332/` |
| `BITCOIN_COOKIE_FILE` | `/run/bitcoind/.cookie` |
| `BITCOIN_WATCH_WALLET` | `osl-watch` |
| `CRYPTO_XMR_ENABLED` | Exact `true` enables XMR; absent or `false` keeps it unavailable |
| `MONERO_WALLET_RPC_URL` | `http://127.0.0.1:18088/` |
| `MONERO_ACCOUNT_INDEX` | `0` |
| `MONERO_PRIMARY_ADDRESS` | Public 95-character primary address of the offline-created view-only wallet |
| `CRYPTO_SETTLEMENT_CALLBACK_URL` | `https://keyserver.oslprivacy.com/v1/internal/crypto/settle` |
| `CRYPTO_WATCHER_REQUEST_SECRET_FILE` | Path to a mode-0600 file containing the HMAC secret used only for Worker invoice requests |
| `CRYPTO_WATCHER_SETTLEMENT_SIGNING_KEY_FILE` | Path to a mode-0600 Ed25519 PKCS#8 PEM private key used only for settlement proofs |
| `CRYPTO_WATCHER_DB_KEY_FILE` | Preferred: path to a separate mode-0600 file containing `openssl rand -base64 32` |
| `CRYPTO_WATCHER_REQUEST_SECRET` | Compatibility fallback only; do not put it in `watcher.env` |
| `CRYPTO_WATCHER_DB_KEY_B64` | Compatibility fallback only; do not put it in `watcher.env` |
| `CRYPTO_WATCHER_DB` | `/var/lib/osl-crypto/invoices.sqlite3` |
| `CRYPTO_BTC_CONFIRMATIONS` | `2` |
| `CRYPTO_XMR_CONFIRMATIONS` | `10` |
| `INVOICE_RETENTION_SECONDS` | `604800` |
| `LISTEN_ADDR` | `127.0.0.1:8789` |

The legacy `BTC_CONFIRMATIONS` and `XMR_CONFIRMATIONS` names remain accepted
only during migration. Configure the `CRYPTO_*` names on both the Worker and
watcher so settlement policy cannot silently drift.

The exact request-secret text must also be installed in the Worker as
`CRYPTO_WATCHER_REQUEST_SECRET`. Only the Ed25519 **public** SPKI value belongs
in the Worker as `CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY`. Never copy the private
settlement key to Cloudflare and never reuse the encrypted-database key.

Create the three local files as the service user and keep secret values out of
the environment file:

```sh
install -o osl-crypto -g osl-crypto -m 0600 /dev/null /etc/osl-crypto/watcher-request-secret
install -o osl-crypto -g osl-crypto -m 0600 /dev/null /etc/osl-crypto/watcher-db-key
openssl genpkey -algorithm ED25519 -out /etc/osl-crypto/watcher-settlement-key.pem
chmod 0600 /etc/osl-crypto/watcher-settlement-key.pem
```

Export only the public verification key for Worker configuration:

```sh
openssl pkey -in /etc/osl-crypto/watcher-settlement-key.pem \
  -pubout -outform DER | base64 -w0
```

Write each generated value directly into its corresponding file without
printing it in logs or shell history. The watcher rejects symlinks, files over
16 KiB, empty files, and any group/other permission bits.

The provisioning script writes only the credential **paths** to `watcher.env`;
it never copies their contents into that environment file. A successful message
means Bitcoin Core accepted the public ranged descriptor and Monero Wallet RPC
returned the pinned primary address. Retain and independently inspect
`/etc/osl-crypto/monero-view-only-creation.receipt` before enabling public XMR
checkout; address matching alone is not proof against a replaced wallet binary
or a previously compromised host.

Expose only `127.0.0.1:8789` through a dedicated Cloudflare Tunnel hostname.
The public invoice endpoint still requires a timestamped, method-and-path-bound
HMAC. Configure the Worker's `CRYPTO_WATCHER_URL` to that HTTPS hostname.

## Storage and retention

SQLite stores SHA-256 invoice/address indexes plus one XChaCha20-Poly1305
encrypted payload. It stores no email, submitted transaction id, IP address,
social account, or spending key. Pending and settled records are deleted after
the configured retention window. Node wallet history inherently remains in the
operator's own watch-only wallets until their wallet files are rotated; that is
separate from OSL application storage and must be covered by the retention
policy.

## Test

```sh
cargo test --manifest-path services/crypto-watcher/Cargo.toml
cargo clippy --manifest-path services/crypto-watcher/Cargo.toml --all-targets -- -D warnings
```
