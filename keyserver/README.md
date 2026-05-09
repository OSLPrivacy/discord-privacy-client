# discord-privacy-keyserver (prototype)

Minimum prototype key server for `discord-privacy-client`'s v1 alpha
build order.

> **INSECURE BY DESIGN.** Plain HTTP. No auth on any endpoint. No
> signature verification. Sqlite stored in plain on disk. v1 alpha
> prototype scaffold only — both endpoints are dev devices, both
> clients are the developer's. v1 stable replaces this with the
> authenticated, TLS-only, OAuth-gated, rate-limited service in
> `../docs/design/key-server-api.md`.

## Endpoints

| Method | Path                            | Notes                          |
| ---    | ---                              | ---                            |
| GET    | `/v1/healthz`                    | liveness                       |
| POST   | `/v1/register`                   | upsert identity-key record     |
| GET    | `/v1/pubkeys/:user_id`           | look up identity keys          |
| POST   | `/v1/wrapped-keys`               | upload one wrapped-share blob  |
| GET    | `/v1/wrapped-keys/:content_id`   | fetch + (single_use) consume   |

`single_use=true` rows are deleted in the same transaction that
returns them. Rows past `expires_at` return 410 Gone and are
lazy-deleted on read.

## Run

```sh
npm install
npm test
PORT=3000 KEYSERVER_DB=./keyserver.db npm start
```

## Deferred

Per `../docs/design/build-order.md` and `../docs/design/key-server-api.md`:
prekey-bundle / replenish, burn (DELETE /v1/wrapped-keys), session
rotation, anonymous-credential token issuance, Discord OAuth gate,
TLS, rate limiting, threshold-share fan-out across 5 jurisdictions.
