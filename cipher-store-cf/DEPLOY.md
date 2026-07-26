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

Migrations `0003_r2_attachments.sql` and
`0004_attachment_capability_digests_and_quota.sql` create the private R2
expiry index, replace raw bearer tokens with SHA-256 digests, and prepare
Worker-enforced atomic aggregate quota accounting. Apply both before deploying code
that exposes attachment routes. Migration 0004 intentionally replaces the
unshipped, empty 0003 attachment table rather than copying raw capabilities.

Migration `0005_view_once_links.sql` adds the view-once link lane. Apply
it before deploying code that exposes `/v1/link` or `/v/…`. The table has
no key column and no identity column by construction; `data` is nullable
because NULL *is* the burned state, and the residual row is a
content-free receipt so the sender can be told "Retrieved at HH:MM" or
"Expired without being retrieved".

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

## §4 Provision private R2 attachment storage

```sh
npx wrangler r2 bucket create osl-cipher-attachments-prod
```

The checked-in `[[r2_buckets]]` binding is named `ATTACHMENTS`; its bucket name
must match the resource just created. This intentionally makes deployment fail
closed until the private bucket exists.
Keep public access, public custom domains, event notifications, object
metadata logging, and observability disabled. The Worker uses its in-process
binding; there is no access key or secret to store.

## §4b Enable the view-once link lane (optional, deliberate)

`POST /v1/link` is **fail-closed**: it returns 503 until the keyserver's
link-grant public key is installed. Leave it unset and the lane stays
inert — the Worker will not create links at all. That is the correct
default, because an ungated one-time link store is an open, logless,
self-deleting file host.

```sh
# base64 Ed25519 public key of the keyserver's link-grant issuer
npx wrangler secret put LINK_GRANT_PUBKEY_B64
```

Then, and only then, attach the aged link domain. The `[[routes]]` block
in `wrangler.toml` is checked in **commented out** on purpose; uncomment
it and set the real hostname. Before enabling the route:

- the `abuse@<host>` mailbox must exist (every landing page publishes it);
- the wildcard certificate must cover any random subdomains you intend to use;
- verify `GET /v/<anything>` returns 200 with an identical body, `Content-Length` and `ETag` for a live id, an expired id and an id that never existed.

## §5 Deploy

```sh
npm run deploy
```

The Worker registers under `oslprivacy-cipher-store`. Take the
deployed `*.workers.dev` URL and add a custom-domain route
(Cloudflare dashboard → Workers & Pages → cipher-store → Triggers
→ Custom Domain) pointing at `ciphers.oslprivacy.com` (or whatever
public subdomain you prefer — the client config will point here).

## §6 Smoke test

```sh
# Upload with the shortest accepted TTL, then immediately delete.
TOKEN=$(openssl rand -hex 16)
ID=$(curl -sX POST https://ciphers.oslprivacy.com/v1/blob \
  -H "X-OSL-TTL-Seconds: 3600" \
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

## §7 Disable observability

`wrangler.toml` explicitly disables Worker observability, invocation logs,
persistence, traces, and sampling. After deployment, verify the Worker's
settings → Observability page still shows disabled. Do not override the checked
in configuration from the dashboard; retained URL/status logs are outside the
cipher store's intended data-minimisation boundary.

## §8 Operational notes

- `wrangler tail` shows live request logs but writes nothing to
  disk unless you pipe it. **Do not** pipe to a file in production.
- The TTL-sweep cron runs every 5 minutes. If a subpoena lands,
  data older than 5 minutes past its expiry is already gone.
- D1 backups: Cloudflare Time Travel can restore D1 up to 30 days
  back. This DOES restore deleted blobs. If subpoena-resistance is
  the goal, disable Time Travel for this database — the trade-off
  is no D1-side disaster recovery.
