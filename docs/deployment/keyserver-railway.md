# Deploying the OSL keyserver to Railway

Step-by-step runbook for taking the Phase B-hardened keyserver from
local dogfood to a Railway-hosted instance suitable for closed-beta
two-peer use. Plain HTTP at the application layer; Railway's reverse
proxy provides TLS termination automatically.

---

## Prerequisites

- A Railway account with billing set up (the keyserver is small,
  but Railway free tier may be insufficient for persistent
  volumes).
- The repository pushed to GitHub (Railway pulls from a Git source
  by default).
- `openssl` and `curl` on the local box for token generation +
  smoke testing.

---

## 1. Generate the admin token

The token is a 256-bit random string. Use:

```sh
openssl rand -hex 32
```

Save the output securely — you'll paste it into Railway's env-var
UI in step 4 and into your local `keyserver.json` in step 6.

The keyserver compares incoming tokens in constant time after
SHA-256 hashing, so there's no timing-leak benefit to shorter
tokens — 32 hex bytes (64 chars) is the recommended minimum.

---

## 2. Create the Railway project

In the Railway UI:

1. **New Project → Deploy from GitHub repo**.
2. Select the `discord-privacy-client` repository.
3. **Service → Settings → Source → Root Directory**: set to
   `keyserver`. Railway uses this as the build / run context, so
   `railway.toml`, `package.json`, and `src/` are all relative
   to that path.
4. **Settings → Networking → Generate Domain**. Railway gives the
   service a `*.up.railway.app` URL with auto TLS.

The build is auto-detected as Node.js. The `railway.toml` in
`keyserver/` overrides defaults:

- `buildCommand`: `npm install --omit=dev && npm rebuild better-sqlite3`
  (the rebuild ensures the native sqlite binding matches the
  Railway container's libc / arch).
- `startCommand`: `npm start`.
- `healthcheckPath`: `/v1/healthz`.

---

## 3. Mount a persistent volume

Without a volume, Railway redeploys (every push to the connected
branch) destroy the container's local filesystem and **wipe every
registered identity**. Required setup:

1. Service → Settings → **Volumes → Add Volume**.
2. Mount path: `/data`.
3. Size: 1 GB is plenty for closed-beta scale (the keyserver's
   sqlite file is ~100 KB per registered user with full prekey
   pool).

Then in step 4, set `KEYSERVER_DB=/data/keyserver.db`.

---

## 4. Set environment variables

Service → **Variables**. Add:

| Variable | Value |
| --- | --- |
| `OSL_KEYSERVER_ADMIN_TOKEN` | the token from step 1 |
| `OSL_KEYSERVER_ALLOWED_USERS` | comma-separated user_ids of dogfood testers, e.g. `liam,henry` |
| `HOST` | `0.0.0.0` |
| `KEYSERVER_DB` | `/data/keyserver.db` |

**Do NOT set `PORT`.** Railway sets it automatically — overriding
breaks the platform's reverse-proxy routing.

`SELECTOR_MANIFEST_PATH` is optional; leave unset until selector
manifests are being shipped.

---

## 5. Deploy + verify

Railway auto-deploys on every push to the configured branch. After
the first deploy:

```sh
# Replace with your Railway-issued domain.
KEYSERVER=https://your-service.up.railway.app
TOKEN=<paste the value of OSL_KEYSERVER_ADMIN_TOKEN here>

# (a) Health check — public, should succeed.
curl -s "$KEYSERVER/v1/healthz"
# Expected: {"ok":true}

# (b) Mutation without auth — should 401.
curl -i -X POST "$KEYSERVER/v1/register" \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"liam","ik_x25519_pub":"eA==","ik_ed25519_pub":"eA==","ik_mlkem768_pub":"eA==","ik_x25519_signature":"eA=="}'
# Expected: HTTP/2 401 + {"error":"unauthorized"}

# (c) Mutation with wrong token — should 401.
curl -i -X POST "$KEYSERVER/v1/register" \
  -H 'Authorization: Bearer wrong-token' \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"liam","ik_x25519_pub":"eA==","ik_ed25519_pub":"eA==","ik_mlkem768_pub":"eA==","ik_x25519_signature":"eA=="}'
# Expected: HTTP/2 401

# (d) Mutation with correct token but disallowed user — should 403.
curl -i -X POST "$KEYSERVER/v1/register" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"mallory","ik_x25519_pub":"eA==","ik_ed25519_pub":"eA==","ik_mlkem768_pub":"eA==","ik_x25519_signature":"eA=="}'
# Expected: HTTP/2 403 + {"error":"forbidden: user_id not on allowlist"}
```

If all four match, the keyserver is correctly hardened and reachable.

---

## 6. Update client `keyserver.json`

Each dogfood tester updates their per-machine `keyserver.json`
(at `~/.config/osl/keyserver.json` on Linux/macOS or
`%APPDATA%\osl\keyserver.json` on Windows) to point at the
deployed server:

```json
{
  "base_url": "https://your-service.up.railway.app",
  "user_id": "liam",
  "admin_token": "<the token from step 1>"
}
```

Use `https://` for the deployed case (Railway force-redirects
HTTP→HTTPS at the edge). The `KeyServerClient` ships with rustls
TLS support and accepts both `http://` and `https://`; rustls
ships the Mozilla CA bundle baked in (webpki-roots), so
Railway's standard public-CA cert chain validates without any
client-side configuration.

After updating `keyserver.json`, restart the Tauri app on each
peer's machine. Bootstrap logs should show:

```
OSL bootstrap: KeyServerClient initialised (admin_token=present)
OSL bootstrap: registered with key-server
```

Then the Phase 4 encrypt path works against the deployed instance.

---

## Adding a new dogfood tester

1. Verify the new tester's `user_id` (the string they'll register
   as).
2. In Railway → service → Variables, edit
   `OSL_KEYSERVER_ALLOWED_USERS` to append the new user_id (e.g.
   `liam,henry,carol`).
3. Save. Railway auto-restarts the container; the allowlist is
   reread on startup.
4. Hand the new tester:
   - The Railway service URL.
   - Their assigned `user_id`.
   - The current value of `OSL_KEYSERVER_ADMIN_TOKEN` (over a
     secure channel — DM, password manager share, etc.).
5. They populate their local `keyserver.json` per step 6 and
   start the app.

---

## Rolling the admin token

Run this when the token may have leaked (committed accidentally,
shared on the wrong channel, etc.). All clients must update their
local `keyserver.json` BEFORE the rotation completes; otherwise
they'll hit 401s on every state-mutating request until they do.

1. Generate a new token: `openssl rand -hex 32`.
2. Distribute the new token to all dogfood testers via secure
   channel.
3. Each tester updates their `keyserver.json` and **does not yet
   restart the client**.
4. In Railway → Variables, replace `OSL_KEYSERVER_ADMIN_TOKEN`
   with the new value. Railway auto-restarts.
5. Each tester restarts their client. Bootstrap will succeed
   because the new token in `keyserver.json` matches the new
   server-side value.
6. Verify via the smoke-test in step 5 (using the new token).

If a tester misses the announcement and tries to send while their
client still holds the old token, they'll see "Failed to send" —
not an open failure, just a fail-closed surface. They re-sync via
the `keyserver.json` update + restart.

---

## Monitoring

- **Railway logs**: `OSL bootstrap` lines fire at every startup;
  401 / 403 responses log the attempted user_id (when present) at
  `warn` level. Tail via Railway's Logs tab.
- **Metrics**: Railway's built-in CPU / RAM / network graphs are
  sufficient for closed-beta scale. Per-route latency / error
  rate breakdown deferred until scale demands it.
- **Failed-token alarms**: not yet wired. If alarms become
  necessary, the warn-level log lines are structured (JSON via
  pino) and can feed into Logtail / Datadog / etc.

---

## TLS

Client-side TLS lands in the same scope as Phase B. The
`KeyServerClient` (`crates/keystore/src/client.rs`) uses
`reqwest` 0.12 with the `rustls-tls` feature. That alias enables
`rustls-tls-webpki-roots`: rustls handles TLS, and the Mozilla
CA bundle (via `webpki-roots`) is baked in at compile time.
There's no system cert chain dependency — the client is
self-contained, and Railway's standard `*.up.railway.app` public-
CA cert chain validates out of the box.

No certificate pinning. Pinning is a v1-stable feature; for
closed-beta dogfood we trust the public-CA chain. If a CA-level
adversary becomes part of the threat model before v1-stable
ships, we'd add `reqwest::ClientBuilder::add_root_certificate`
with the specific Railway-leaf or Cloudflare-issuer pin. Not
needed yet.

If you want to use a self-signed cert for a non-Railway
deployment (e.g. on-prem keyserver behind a private CA), today
the client doesn't support it — that's a follow-up that adds
either a pinned-cert config option or an env var pointing at a
custom root cert PEM. Open an issue if you hit this.
