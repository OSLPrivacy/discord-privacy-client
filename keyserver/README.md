# discord-privacy-keyserver

Key server for `discord-privacy-client`. Holds identity public keys,
prekey bundles, and wrapped key blobs. Plain HTTP at the application
layer (TLS terminated by the hosting platform — Cloudflare/Railway/
etc. — and trusted by the Rust client via rustls + webpki-roots).

## Status

Phase B (closed-beta deployable). The server is **production-ish for
small dogfood scale**: pre-shared admin token gates state-mutating
routes, user-id allowlist gates registration, light rate limiting on
mutations. Local-dev mode preserved: when no env vars are set, the
behaviour is identical to the v0.0.1 prototype (open mutations, no
rate limit).

What this is **not**:

- Audited. The crypto layer the client relies on (PQXDH, ratchet,
  AEAD, stego) lives in the Rust `crates/` tree and has its own
  separate review burden; the keyserver itself is small but not
  formally reviewed.
- TLS-terminating. Deploy behind a TLS-terminating reverse proxy
  (Railway / Cloudflare). Plain HTTP is for the application
  protocol only.
- Discord-OAuth-gated. Closed-beta uses the trusted-token model;
  the OAuth gate is v1-stable scope per
  `../docs/design/auth-flow.md`.
- Ed25519-signing-on-register-protected. The `ik_x25519_signature`
  field is still mocked client-side (the prototype identity-key
  self-sig is `b64("PROTOTYPE_NO_SIG")`). Burn / replenish DO
  verify Ed25519 signatures.

## Endpoints

| Method | Path                            | Auth                          |
| ---    | ---                             | ---                           |
| GET    | `/v1/healthz`                   | public                        |
| POST   | `/v1/register`                  | admin token + allowlist       |
| GET    | `/v1/pubkeys/:user_id`          | public                        |
| POST   | `/v1/wrapped-keys`              | admin token                   |
| GET    | `/v1/wrapped-keys/:content_id`  | public                        |
| DELETE | `/v1/wrapped-keys`              | admin token + Ed25519 sig     |
| GET    | `/v1/prekey-bundle/:user_id`    | public                        |
| POST   | `/v1/prekey-bundle/replenish`   | admin token + Ed25519 sig     |
| GET    | `/v1/selector-manifest`         | public                        |

GETs serving public keys are intentionally unauthenticated — the
recipient public-key lookup IS the public-side of the design.
Authenticated routes carry `Authorization: Bearer <token>` matching
`OSL_KEYSERVER_ADMIN_TOKEN`. Token comparison is constant-time
(SHA-256 + `crypto.timingSafeEqual`).

## Run (local dev, no auth)

```sh
npm install
npm test
PORT=3000 KEYSERVER_DB=./keyserver.db npm start
```

Server starts in dev mode and logs a loud warning that admin auth is
disabled. Suitable for `127.0.0.1` testing only.

## Run (production / Railway / hardened local)

```sh
npm install
export OSL_KEYSERVER_ADMIN_TOKEN="$(openssl rand -hex 32)"
export OSL_KEYSERVER_ALLOWED_USERS="liam,henry"
export HOST="0.0.0.0"           # loopback only by default; opt out for Railway
export KEYSERVER_DB="/data/keyserver.db"
PORT=3000 npm start
```

Smoke check (replace `$TOKEN` with the value of
`OSL_KEYSERVER_ADMIN_TOKEN`):

```sh
# Public — should succeed without auth:
curl -s http://localhost:3000/v1/healthz
# Mutation without token — should 401:
curl -i -X POST http://localhost:3000/v1/register \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"liam","ik_x25519_pub":"x","ik_ed25519_pub":"e","ik_mlkem768_pub":"m","ik_x25519_signature":"s"}'
# Mutation with disallowed user_id — should 403:
curl -i -X POST http://localhost:3000/v1/register \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"user_id":"mallory","ik_x25519_pub":"eA==","ik_ed25519_pub":"eA==","ik_mlkem768_pub":"eA==","ik_x25519_signature":"eA=="}'
```

For the full Railway deploy runbook, see
`../docs/deployment/keyserver-railway.md`.

## Env vars

| Variable                       | Default        | Notes                                                                 |
| ---                            | ---            | ---                                                                   |
| `PORT`                         | `3000`         | Railway sets this automatically.                                      |
| `HOST`                         | `127.0.0.1`    | Set `0.0.0.0` (or `::`) for Railway / any container hosting.          |
| `KEYSERVER_DB`                 | `./keyserver.db` | Persistent volume path on Railway (e.g. `/data/keyserver.db`).      |
| `OSL_KEYSERVER_ADMIN_TOKEN`    | _(unset)_      | Pre-shared bearer token. Unset = dev mode (no auth, loud warning).    |
| `OSL_KEYSERVER_ALLOWED_USERS`  | _(unset)_      | Comma-separated allowlist for /v1/register. Unset = no allowlist.     |
| `SELECTOR_MANIFEST_PATH`       | _(unset)_      | Optional path to a SignedManifest envelope JSON.                      |

## Rate limiting

Rate limit is auto-enabled when `OSL_KEYSERVER_ADMIN_TOKEN` is set.
Default: 10 requests / minute / IP per mutation route. GETs are
never rate-limited. Adjust by editing the `rateLimit` object in
`src/server.js` (or surface via env vars when scale demands it).

429 responses include the standard `Retry-After` header.

## Deferred (still unimplemented)

Per `../docs/design/build-order.md` and
`../docs/design/key-server-api.md`: session rotation, anonymous
credential token issuance, Discord OAuth gate, real
`ik_x25519_signature` verification on register, threshold-share
fan-out across 5 jurisdictions.
