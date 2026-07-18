# oslprivacy-keyserver (Cloudflare Workers + D1)

F1.1 port of the existing Railway keyserver (`../keyserver/`). Same
endpoint surface, same wire schemas, same canonical byte encoding —
just running on Workers + D1 instead of Fastify + better-sqlite3.

F1.2 adds a one-time $5 Stripe Checkout + webhook + lifetime Pro issuance.
OSL does not submit or persist an email or create a reusable Stripe Customer
profile for this public checkout. Crypto invoices are
anonymous and automatically verified by the isolated watch-only node service
in `../services/crypto-watcher`; customers do not submit an email or txid.
F1.4 cutover from
`.workers.dev` to `keyserver.oslprivacy.com`.

## Endpoints (F1.1 baseline)

| Method | Path                              | Auth                          |
| ------ | --------------------------------- | ----------------------------- |
| GET    | `/v1/healthz`                     | public                        |
| POST   | `/v1/register`                    | Ed25519 self/rotation proof*  |
| GET    | `/v1/pubkeys/:user_id`            | public                        |
| POST   | `/v1/wrapped-keys`                | fresh registered-sender signature |
| GET    | `/v1/wrapped-keys/:content_id`    | fresh intended-recipient signature |
| DELETE | `/v1/wrapped-keys`                | registered-sender signature   |
| GET    | `/v1/prekey-bundle/:user_id`      | registered-requester signature |
| POST   | `/v1/prekey-bundle/replenish`     | registered-owner signature    |
| GET    | `/v1/selector-manifest`           | public                        |
| POST   | `/v1/crypto/quote`                | public, rate-limited          |
| POST   | `/v1/crypto/status`               | anonymous claim token         |
| POST   | `/v1/internal/crypto/settle`      | timestamped watcher Ed25519 signature |
| POST   | `/v1/checkout-session`            | public, live Stripe only      |
| POST   | `/v1/checkout/claim`              | browser claim capability      |
| POST   | `/v1/stripe/webhook`              | Stripe signature, live only   |
| POST   | `/v1/internal/comp/batches`       | dual operator bearer secrets  |
| DELETE | `/v1/internal/comp/batches/:id`   | dual operator bearer secrets  |
| GET    | `/v1/download/windows`            | public tracked redirect       |
| POST   | `/v1/telegram/webhook`            | Telegram secret and operator allowlist |

\* The self-signature proves key possession, not ownership of the
claimed Discord user ID. Discord OAuth binding remains a release gate.

## First-time setup

```sh
npm install

# Create the D1 database; copy database_id into wrangler.toml.
wrangler d1 create osl-keyserver-prod

# Apply the baseline migration locally (creates ./wrangler/state/d1/*).
npm run db:migrate:local

# Run tests against an isolated D1 and native rate-limit bindings.
npm test

# Local dev server on http://127.0.0.1:8787.
npm run dev
```

## Production deploy

```sh
# Public client mutations are authorized by registered-identity signatures;
# never embed a shared bearer in the desktop app.
wrangler secret put OSL_KEYSERVER_ADMIN_TOKEN
wrangler secret put LICENSE_HMAC_SECRET
wrangler secret put OSL_COMP_ADMIN_TOKEN
wrangler secret put COMP_AUDIT_HMAC_SECRET
wrangler secret put STRIPE_SECRET_KEY
wrangler secret put STRIPE_WEBHOOK_SECRET
wrangler secret put STRIPE_PRICE_ID_PRO
wrangler secret put TELEGRAM_BOT_TOKEN
wrangler secret put TELEGRAM_WEBHOOK_SECRET
wrangler secret put TELEGRAM_OPERATOR_CHAT_IDS
# Optional: selector-manifest JSON envelope.
wrangler secret put SELECTOR_MANIFEST_JSON

# Apply migration to the remote D1.
npm run db:migrate:prod

# Deploy.
wrangler deploy
```

Stripe and crypto delivery are two-phase. The browser-bound claim may fetch
the same ciphertext again after network loss, but no retry mints a new code.
After successful local decryption and local persistence, the browser sends
`acknowledge_delivery: true`; the Worker then tombstones its ciphertext.

Owner-approved comp batches require both operator secrets, contain 1 to 25
expiring codes, and return plaintext only inside one hybrid-encrypted response
to an ephemeral operator key. D1 retains only credential hashes and keyed audit
commitments. Use `scripts/mint-beta-keys.ts`; it writes the locally decrypted
batch to a new mode-0600 file and never prints codes. QA deployments must set
`DEPLOYMENT_ENV=qa` plus an independent `QA_LICENSE_HMAC_SECRET`; their `OSLQ-`
codes are structurally rejected by production validation.

`TELEGRAM_OPERATOR_CHAT_IDS` is a comma-separated allowlist of private chat
IDs and, optionally, one negative group chat ID, for example
`1122334455,5566778899,-1001234567890`. Empty/malformed entries and multiple
group IDs disable Telegram reporting. Duplicate IDs are ignored. During
migration, deployments that have only the legacy `TELEGRAM_ADMIN_CHAT_ID`
continue to work; once the new allowlist is set and verified, delete the legacy
secret.

The worker is reachable at `oslprivacy-keyserver.<account>.workers.dev`
until F1.4 wires the custom domain.
