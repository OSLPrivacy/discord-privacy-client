# F1.4 cutover runbook

Move production keyserver traffic from Railway to
`keyserver.oslprivacy.com` (Cloudflare Workers). Existing clients
keep working via 308 redirect on Railway for 30 days; after that
Railway is shut down and stragglers must update `keyserver.json`.

## 1. DNS

Cloudflare dashboard → DNS for `oslprivacy.com` zone:

- **Type:** AAAA (or A) — actual record doesn't matter, Workers
  binds to any record once routed
- **Name:** `keyserver`
- **Content:** `100::` (placeholder; the Workers route takes over)
- **Proxy:** **proxied** (orange cloud)

TLS auto-issued by Cloudflare. No further config needed.

## 2. Verify the worker route in `wrangler.toml`

F1.4 already uncommented:

```toml
[[routes]]
pattern = "keyserver.oslprivacy.com/*"
zone_name = "oslprivacy.com"
```

Confirm and redeploy:

```sh
cd keyserver-cf
npx wrangler deploy
```

After deploy, this should resolve:

```sh
curl -s https://keyserver.oslprivacy.com/v1/healthz
# → {"ok":true}
```

If 404 or wrong cert, wait 1-2 min for DNS propagation, then
recheck. Workers takes effect within seconds of `wrangler deploy`
returning; the slow step is DNS being globally consistent.

## 3. Switch Railway to redirect mode

On the Railway service:

1. **Service → Variables → New variable:**
   - Name: `OSL_REDIRECT_TARGET`
   - Value: `https://keyserver.oslprivacy.com`
2. **Redeploy** (Railway auto-redeploys on env-var change in most
   plans; otherwise click "Redeploy").

After redeploy, every endpoint except `/v1/healthz` returns
HTTP 308 with `Location: https://keyserver.oslprivacy.com<path>`.
Railway's healthcheck stays green because `/v1/healthz` continues
to return `{"ok":true}` from the Fastify server.

Verify:

```sh
curl -sI https://<railway-url>/v1/pubkeys/x | head -3
# → HTTP/2 308
#   location: https://keyserver.oslprivacy.com/v1/pubkeys/x
```

## 4. Existing clients

The Rust client uses `reqwest::Client` with the default redirect
policy (`Policy::default()` follows up to 10 redirects). 308 PUTs
*should* preserve the body and method per RFC 7538. Confirm
with one round-trip test against your dogfood install before
broadcasting:

```sh
# Point a test install at the OLD Railway URL. Send one OSL
# message. Confirm it goes through.
```

If a client environment doesn't follow 308 for any reason
(unlikely for `reqwest`), users can update `keyserver.json`
manually:

```jsonc
// %APPDATA%\osl\keyserver.json
{
  "base_url": "https://keyserver.oslprivacy.com",
  "user_id": "<your discord snowflake>",
  "admin_token": "<unchanged>"
}
```

## 5. T+30 day shutdown

Schedule a calendar reminder for **30 days after F1.4 deploy**.
On that date:

1. Confirm Cloudflare Workers analytics shows zero 308s being
   served (or below an acceptable error budget).
2. Delete the Railway service.
3. Remove `keyserver/railway.toml` and update
   `docs/deployment/keyserver-railway.md` to mark it historical.

After Railway shutdown, any client still pointed at the old URL
will hard-fail on DNS / TCP — no redirect to fall back on. Users
must update `keyserver.json` to recover.

## 6. F1.4 cutover smoke test

Run this from a fresh OSL install with NO `keyserver.json`:

1. Launch OSL.
2. Configure `keyserver.json`:
   ```json
   { "base_url": "https://keyserver.oslprivacy.com",
     "user_id": "<your snowflake>",
     "admin_token": "<token>" }
   ```
3. Complete the tour (registers identity, generates license, etc.).
4. Send one message to a peer in a whitelisted scope.

If all four steps succeed end-to-end against `keyserver.oslprivacy.com`,
F1.4 is shipped.
