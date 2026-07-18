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

Example Bitcoin wallet initialization (replace the descriptor with an offline
derived xpub descriptor and add the checksum returned by `getdescriptorinfo`):

```sh
bitcoin-cli createwallet "osl-watch" true true "" false true true
bitcoin-cli -rpcwallet=osl-watch importdescriptors \
  '[{"desc":"wpkh([fingerprint/path]xpub.../0/*)#checksum","timestamp":"now","active":true,"internal":false,"range":[0,99999],"next_index":0}]'
```

Create the Monero wallet offline/from trusted software using the public address
and private view key only, then run its RPC locally:

```sh
monero-wallet-rpc --wallet-file /var/lib/osl-crypto/osl-view-only \
  --password-file /etc/osl-crypto/monero-password \
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

The legacy `BTC_CONFIRMATIONS` and `XMR_CONFIRMATIONS` names remain accepted
only during migration. Configure the `CRYPTO_*` names on both the Worker and
watcher so settlement policy cannot silently drift.
| `INVOICE_RETENTION_SECONDS` | `604800` |
| `LISTEN_ADDR` | `127.0.0.1:8789` |

The exact request-secret text must also be installed in the Worker as
`CRYPTO_WATCHER_REQUEST_SECRET`. Only the Ed25519 **public** SPKI value belongs
in the Worker as `CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY`. Never copy the private
settlement key to Cloudflare and never reuse the encrypted-database key.

Create the two local files as the service user and keep secret values out of
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
