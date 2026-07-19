# Private BTC/XMR donation canary client

`crypto-donation-live-canary.mjs` is a local operator tool for the final live
donation checks. It is fixed to `https://keyserver.oslprivacy.com` and has no
wallet RPC, transaction construction, signing, seed, spending, Pro activation,
license, or delivery-acknowledgement capability.

Keep public crypto donations disabled while testing. Create an invoice for an
exact integer number of cents from $1 through $10,000:

```sh
umask 077
node scripts/crypto-donation-live-canary.mjs create \
  --asset btc \
  --amount-cents 100 \
  --state /path/on/encrypted-disk/osl-btc-donation.json
```

Use `--asset xmr` for Monero. The displayed address, native amount,
confirmation count, and expiry are safe to copy into an independent sending
wallet. The mode-`0600` state contains the private claim token. Never copy that
state file into chat, logs, source control, or cloud storage.

After manually sending the exact displayed amount, watch the invoice:

```sh
node scripts/crypto-donation-live-canary.mjs watch \
  --state /path/on/encrypted-disk/osl-btc-donation.json \
  --receipt-out /path/on/encrypted-disk/osl-btc-donation-receipt.json
```

The command accepts only an exact, node-verified `recorded` result bound to the
same invoice, asset, cents, and expiry. It then exclusively creates and flushes
a non-secret mode-`0600` receipt before deleting the claim state. Network loss,
malformed responses, and output failures retain the claim state for a safe
retry. Before its first status request, the client canonicalizes the receipt
path and durably binds that path plus the expected receipt SHA-256 into the
private state. A later invocation cannot redirect the receipt, including after
an interrupted cleanup. An interrupted retry accepts an existing receipt only
when it is byte-for-byte identical to the bound expected receipt.

State updates use a flushed mode-`0600` temporary file in the same directory,
an atomic rename, and a parent-directory flush. Initial file creation and final
state deletion also flush their parent directory, so a successful step remains
durable across a machine crash.

Run the offline test suite with:

```sh
node --test scripts/crypto-donation-live-canary.test.mjs
```

The tests substitute every HTTP, clock, delay, and filesystem operation. They
make no live request and create no invoice or donation.
