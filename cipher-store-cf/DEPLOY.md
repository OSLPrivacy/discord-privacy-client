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

Migration `0008_attachment_sweep_claims.sql` is the additive attachment
cleanup claim boundary. It must be applied before the matching Worker: the
previous Worker never names the companion table and remains unchanged after
the migration, while the matching Worker performs a read-only exact-column
gate before legacy expiry marking, R2 cleanup, or metadata deletion. A missing
or partial `0008` schema therefore fails the attachment sweep closed.
Multipart completion uses the same identity/token/version claim before R2
completion and also fails before touching R2 when `0008` is absent. An expired
`completing` claim recovers an already completed, correctly sized R2 object to
`ready`; it never deletes that object. Only a successful multipart abort plus
an empty post-abort HEAD permits incomplete metadata removal. The
presence of this file in a checkout is not evidence that any live D1 database
has applied it.

Migration `0009_predecessor_completing_adoption.sql` is the additive rolling
deployment recovery boundary for rows a predecessor Worker left in
`completing` without a claim. Apply it before its matching Worker. It records
one immutable one-hour creation cutoff and adds claim origin plus R2 absence
stage; the 0008 Worker ignores all three. The rollout must replace predecessor
Workers inside that hour. The matching Worker adopts only expired,
unlineaged rows created through the cutoff, at the existing 100-row cycle
bound. It never shortens or recovers an active lineaged lease.

For an adopted row the Worker performs HEAD, multipart abort, and a mandatory
post-abort HEAD. A correctly sized object is promoted with the exact
Worker/token/lease-version ready CAS. Empty or mismatched storage is eligible
for metadata removal only after an exact claim/version CAS records confirmed
absence. A crash before metadata deletion keeps the attachment row, quota, and
absence marker; retry rechecks HEAD and completes idempotently. Missing or
malformed 0009 marker state refuses before any R2 call. The migration and
source tests are not evidence that production D1 has applied 0009.

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

## §7b Migration 0006 — session budget + atomic rate counters (STAGED, NOT DEPLOYED)

Fixes the two 2026-07-26 audit findings against this Worker: bodyless multipart
reservations exhausting the global attachment quota, and the non-atomic KV rate
limiter. Report: `docs/reports/server-lane-2026-07-26.md`.

**A migration and its Worker are one operation.** The old Worker never names
`content_expires_at` or `rate_counters`, so it is unaffected by their existence;
the new Worker reads both on every request, so a Worker deployed against the old
schema returns 500 for everything. Migration first, always, and do not leave the
gap open.

### TWO migrations are pending here, not one

`0005_view_once_links.sql` was never applied to production. `wrangler d1 migrations
apply` applies **every** pending migration in one invocation, so this step applies
0005 and 0006 together — the same way 0028 rode along with 0029 on the keyserver on
2026-07-26. Confirm the list before running it and expect both names.

Applying 0005 is safe, and it repairs a latent fault rather than creating one:

- It is pure DDL. It creates `view_once_links` and its indexes and touches no
  existing table, so it cannot affect blobs or attachments.
- The Worker already reads that table. `sweepExpiredLinks` runs unconditionally
  from `scheduled()` (`src/lib/sweep.ts:49-65`), so on a database without the table
  the five-minute cron throws every tick. It is swallowed by its own `try/catch` in
  `src/index.ts`, and the blob and attachment sweeps run in *separate* try blocks
  before it, so the failure is contained — expiry reclamation has not been
  affected. Applying 0005 stops that error.
- `POST /v1/link` cannot reach the missing table: `handleLinkCreate` calls
  `verifyLinkGrant` first, which returns 503 whenever `LINK_GRANT_PUBKEY_B64` is
  unset (`src/lib/link-grant.ts:73-90`). The link lane therefore still cannot be
  exercised end to end after this, and nothing here enables it.
- `POST /v/<id>/fetch` and `/burn` do query the table and currently answer 500 for
  every id. After 0005 they answer the intended collapsed 404/200. That is a small
  improvement to the no-oracle property, not a regression.

Note the ordering consequence: the link-sweep stops erroring at **migration** time,
before the Worker deploy. That is expected.

The test suite already covers the combined state — `test/helpers/d1.ts` applies
every file in `migrations/` in order, so 86/86 passing is a result against
0001–0006 together, not against 0006 alone.

```sh
cd cipher-store-cf

# 0. Prove the tree is the one that was tested.
npm run typecheck && npx vitest run --maxWorkers=1     # expect 10 files / 86 tests

# 1. List pending migrations. EXPECT TWO:
#      0005_view_once_links.sql
#      0006_session_budget_and_atomic_rate_counters.sql
#    If 0005 is absent, stop and reconcile — this doc's premise no longer holds.
npx wrangler d1 migrations list osl-cipher-store-prod --remote

# 1b. Read-only: does the live Worker already contain the view-once link lane?
#     503 => yes (grant unset, refused before touching D1). 404 => predates Wave A3.
#     Either answer is fine; this just records which one you started from.
curl -so /dev/null -w '%{http_code}\n' -X POST https://ciphers.oslprivacy.com/v1/link

# 2. Snapshot before mutating. Note the bookmark Time Travel reports.
npx wrangler d1 time-travel info osl-cipher-store-prod

# 3. Migrations — applies 0005 AND 0006.
npx wrangler d1 migrations apply osl-cipher-store-prod --remote

# 4. Worker, immediately after. Note the printed version id.
npx wrangler deploy

# 5. Prove the fixes are LIVE, not merely deployed.
node scripts/post-deploy-probe.mjs --host https://ciphers.oslprivacy.com
```

Smoke test after step 4 (uses the §6 host):

```sh
# Multipart session receipt must still promise the requested content TTL...
TOKEN=$(openssl rand -hex 16)
curl -sX POST https://ciphers.oslprivacy.com/v1/attachment/session \
  -H "X-OSL-TTL-Seconds: 604800" -H "X-OSL-Fetch-Token: $TOKEN" \
  -H "X-OSL-Size-Bytes: 1024" -H "content-length: 0"
# expect: 201, expires_at ~ now + 604800, max_part_bytes 8388608, max_parts 65

# ...while the row itself holds only the short reclaim deadline.
npx wrangler d1 execute osl-cipher-store-prod --remote --command \
  "SELECT state, expires_at - created_at AS held, content_expires_at - created_at AS promised
     FROM attachment_objects ORDER BY created_at DESC LIMIT 1"
# expect: state=uploading, held<=900, promised=604800

# An ordinary blob upload still works (this is the atomic-limiter path).
curl -sX POST https://ciphers.oslprivacy.com/v1/blob \
  -H "X-OSL-TTL-Seconds: 3600" -H "X-OSL-Fetch-Token: $(openssl rand -hex 16)" \
  --data-binary $'\x01\x02\x03\x04' -i | head -1
# expect: HTTP/2 201
```

**Rollback.** Both changes are additive, so the fast path is Worker-only:

```sh
npx wrangler rollback --message "revert audit fixes: <reason>"
```

The previous Worker ignores the new column and the new table, so it runs
correctly against the 0006 schema with no schema rollback needed. Leave the
migration applied — reverting it is neither required nor safe, because
`attachment_objects.content_expires_at` holds the only record of the expiry that
in-flight sessions were promised. If the schema must be undone anyway, restore
from the Time Travel bookmark taken in step 2 and accept that in-flight
multipart sessions are lost.

There is no rollback hazard of the kind a Durable Object would introduce: no new
binding, no DO class, no `[[migrations]]` tag in `wrangler.toml`.

## §8 Operational notes

- `wrangler tail` shows live request logs but writes nothing to
  disk unless you pipe it. **Do not** pipe to a file in production.
- The TTL-sweep cron runs every 5 minutes. If a subpoena lands,
  data older than 5 minutes past its expiry is already gone.
- D1 backups: Cloudflare Time Travel can restore D1 up to 30 days
  back. This DOES restore deleted blobs. If subpoena-resistance is
  the goal, disable Time Travel for this database — the trade-off
  is no D1-side disaster recovery.
