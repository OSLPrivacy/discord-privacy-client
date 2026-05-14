# oslprivacy-keyserver (Cloudflare Workers + D1)

F1.1 port of the existing Railway keyserver (`../keyserver/`). Same
endpoint surface, same wire schemas, same canonical byte encoding —
just running on Workers + D1 instead of Fastify + better-sqlite3.

F1.2 will layer Stripe Checkout + webhook + license issuance on top.
F1.3 adds the crypto manual-payment flow. F1.4 cutover from
`.workers.dev` to `keyserver.oslprivacy.com`.

## Endpoints (F1.1 baseline)

| Method | Path                              | Auth                          |
| ------ | --------------------------------- | ----------------------------- |
| GET    | `/v1/healthz`                     | public                        |
| POST   | `/v1/register`                    | admin token + allowlist       |
| GET    | `/v1/pubkeys/:user_id`            | public                        |
| POST   | `/v1/wrapped-keys`                | admin token                   |
| GET    | `/v1/wrapped-keys/:content_id`    | public                        |
| DELETE | `/v1/wrapped-keys`                | admin token + Ed25519 sig     |
| GET    | `/v1/prekey-bundle/:user_id`      | public                        |
| POST   | `/v1/prekey-bundle/replenish`     | admin token + Ed25519 sig     |
| GET    | `/v1/selector-manifest`           | public                        |

## First-time setup

```sh
npm install

# Create the D1 database; copy database_id into wrangler.toml.
wrangler d1 create osl-keyserver-prod

# Create the KV namespace for rate-limit counters; copy id into wrangler.toml.
wrangler kv namespace create RATE_LIMIT_KV

# Apply the baseline migration locally (creates ./wrangler/state/d1/*).
npm run db:migrate:local

# Run tests against an in-memory D1 + KV via miniflare.
npm test

# Local dev server on http://127.0.0.1:8787.
npm run dev
```

## Production deploy

```sh
# Secrets — admin token and allowlist. Same model as Railway.
wrangler secret put OSL_KEYSERVER_ADMIN_TOKEN
wrangler secret put OSL_KEYSERVER_ALLOWED_USERS
# Optional: selector-manifest JSON envelope.
wrangler secret put SELECTOR_MANIFEST_JSON

# Apply migration to the remote D1.
npm run db:migrate:prod

# Deploy.
wrangler deploy
```

The worker is reachable at `oslprivacy-keyserver.<account>.workers.dev`
until F1.4 wires the custom domain.
