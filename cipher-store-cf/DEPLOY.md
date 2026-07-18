# cipher-store-cf deploy

One-time setup, then `npm run deploy` per change.

## §0 Pre-flight

```sh
cd cipher-store-cf
npm install
npx tsc --noEmit         # must be silent
```

## §1 Cloudflare account

```sh
npx wrangler login        # browser-based OAuth
npx wrangler whoami       # confirm
```

## §2 Provision D1

```sh
npx wrangler d1 create osl-cipher-store-prod
```

Paste the printed `database_id` into `wrangler.toml` under
`[[d1_databases]]`.

```sh
npm run db:migrate:prod
```

## §3 Provision KV (rate-limit)

```sh
npx wrangler kv namespace create RATE_LIMIT
```

Paste the printed `id` into `wrangler.toml` under
`[[kv_namespaces]]`.

Install a random server-only HMAC key. Never put this value in the repo or a
command-line argument:

```sh
openssl rand -base64 48 | npx wrangler secret put RATE_LIMIT_HASH_KEY
```

## §4 Deploy

```sh
npm run deploy
```

The Worker registers under `oslprivacy-cipher-store`. Take the
deployed `*.workers.dev` URL and add a custom-domain route
(Cloudflare dashboard → Workers & Pages → cipher-store → Triggers
→ Custom Domain) pointing at `ciphers.oslprivacy.com` (or whatever
public subdomain you prefer — the client config will point here).

## §5 Smoke test

```sh
# Upload (3 sec TTL would be ideal for a smoke test but the server
# only accepts 24h/72h/7d -- so we upload then immediately delete).
TOKEN=$(openssl rand -hex 16)
ID=$(curl -sX POST https://ciphers.oslprivacy.com/v1/blob \
  -H "X-OSL-TTL-Seconds: 86400" \
  -H "X-OSL-Fetch-Token: $TOKEN" \
  -H "content-type: application/octet-stream" \
  --data-binary $'\x01\x02\x03\x04' \
  | jq -r .id)
echo "uploaded id=$ID"

# Fetch back.
curl -s https://ciphers.oslprivacy.com/v1/blob/$ID \
  -H "X-OSL-Fetch-Token: $TOKEN" | xxd | head -1

# Burn.
curl -sX DELETE https://ciphers.oslprivacy.com/v1/blob/$ID \
  -H "X-OSL-Fetch-Token: $TOKEN" -i | head -1
# expect: HTTP/2 204
```

## §6 Disable observability

`wrangler.toml` explicitly disables Worker observability, invocation logs,
persistence, traces, and sampling. After deployment, verify the Worker's
settings → Observability page still shows disabled. Do not override the checked
in configuration from the dashboard; retained URL/status logs are outside the
cipher store's intended data-minimisation boundary.

## §7 Operational notes

- `wrangler tail` shows live request logs but writes nothing to
  disk unless you pipe it. **Do not** pipe to a file in production.
- The TTL-sweep cron runs every 5 minutes. If a subpoena lands,
  data older than 5 minutes past its expiry is already gone.
- D1 backups: Cloudflare Time Travel can restore D1 up to 30 days
  back. This DOES restore deleted blobs. If subpoena-resistance is
  the goal, disable Time Travel for this database — the trade-off
  is no D1-side disaster recovery.
