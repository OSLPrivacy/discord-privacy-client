# F1-DEPLOY runbook

Walks the keyserver-cf deploy end-to-end: provision resources, set
secrets, deploy, register the Stripe webhook, smoke-test every
endpoint, run the canonical end-to-end Stripe-test-mode checkout.

**Secrets handling, non-negotiable:** every credential below goes
through `wrangler secret put <NAME>` — wrangler prompts
interactively, you paste the value at that prompt. **Never** put a
secret into `wrangler.toml`, `.dev.vars`, a `.env` file committed
to git, a shell variable (history leak), or this document. The
only file values are the D1 `database_id` and the KV `id`, which
are resource identifiers, not credentials.

**Mode for this entire runbook:** Stripe TEST MODE (`sk_test_…`).
Do not paste live keys until a separate pre-launch cutover.

---

## §0 Pre-flight

Local checks that must pass before touching production:

```sh
cd keyserver-cf
npm install
npx tsc --noEmit               # must be silent
npx vitest run                  # must be 103 passed
```

If either fails, fix locally first — do not deploy a red worker.

---

## §1 Account login

```sh
npx wrangler login              # browser-based OAuth, one-time
npx wrangler whoami             # confirm
```

`wrangler login` writes an OAuth token to your machine's
keychain (Windows Credential Manager / macOS Keychain). It does
not touch the repo.

---

## §2 Provision D1 + KV

These produce **resource IDs** that go into `wrangler.toml` (not
secrets — IDs identify which D1/KV to bind, but authorization is
your wrangler token).

```sh
npx wrangler d1 create osl-keyserver-prod
```

Output ends with:

```
[[d1_databases]]
binding = "DB"
database_name = "osl-keyserver-prod"
database_id = "<UUID>"
```

→ Paste the UUID into `wrangler.toml` line 21 (replace
`REPLACE_WITH_DATABASE_ID`).

```sh
npx wrangler kv namespace create RATE_LIMIT_KV
```

Output ends with:

```
{ binding = "RATE_LIMIT_KV", id = "<KV_ID>" }
```

→ Paste the id into `wrangler.toml` line 29 (replace
`REPLACE_WITH_KV_NAMESPACE_ID`).

Commit `wrangler.toml` if you keep this branch on git. The IDs are
not secret.

---

## §3 Apply migrations

All three migrations (F1.1 baseline + F1.2 subscriptions + F1.3
crypto) ship together:

```sh
npm run db:migrate:prod
# equivalent: npx wrangler d1 migrations apply osl-keyserver-prod --remote
```

Wrangler reports each `00NN_*.sql` as applied. Re-runs are no-ops.

---

## §4 Generate the two random secrets locally

These never leave your machine until you paste them into the
wrangler prompt. Pick the variant matching your shell:

**Node (any OS, wrangler requires node anyway):**

```sh
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
```

Run it twice, once for each random secret. Keep both values open
in a notepad / password manager that you'll clear at end-of-step.

**PowerShell (Windows native, no openssl required):**

```ps1
-join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Maximum 256) })
```

**openssl (macOS / Linux / Git-Bash on Windows):**

```sh
openssl rand -hex 32
```

You'll use the first value for `OSL_KEYSERVER_ADMIN_TOKEN` and the
second for `LICENSE_HMAC_SECRET`.

---

## §5 Set the secrets (Phase A — everything except the Stripe webhook signing secret)

Run each `wrangler secret put` command. Wrangler prompts:

```
✏ Enter a secret value: ›
```

Paste the value. **Do not echo it on the command line**, do not
use here-strings, do not pipe from a file — paste at the prompt
only.

| Variable | What to paste | Where to get it |
| --- | --- | --- |
| `OSL_KEYSERVER_ADMIN_TOKEN` | first 32-byte hex from §4 | generated locally |
| `OSL_KEYSERVER_ALLOWED_USERS` | CSV of allowlisted Discord snowflakes, e.g. `147700845179948241,<henry's>` | Discord user IDs (not secret, but config) |
| `LICENSE_HMAC_SECRET` | second 32-byte hex from §4 | generated locally |
| `STRIPE_SECRET_KEY` | `sk_test_…` value | Stripe Dashboard → Developers → API keys, **toggle "View test data"** |
| `STRIPE_PRICE_ID_MONTHLY` | `price_…` for the $5/mo product | Stripe Dashboard → Products → your monthly product → Pricing |
| `STRIPE_PRICE_ID_YEARLY` | `price_…` for the $50/yr product | same screen, yearly product |
| `CHECKOUT_SUCCESS_URL` | `https://oslprivacy.com/checkout/success` (or wherever your post-checkout page lives) | your site |
| `CHECKOUT_CANCEL_URL` | `https://oslprivacy.com/checkout/cancel` | your site |
| `BILLING_PORTAL_RETURN_URL` | `https://oslprivacy.com/account` | your site |
| `RESEND_API_KEY` | `re_…` | Resend Dashboard → API Keys |
| `RESEND_FROM` | `OSL <noreply@oslprivacy.com>` (or `licenses@`) | your verified Resend domain |
| `SUPPORT_EMAIL` | `support@oslprivacy.com` | your inbox |
| `CRYPTO_BTC_ADDRESS` | a BTC address you control | your wallet |
| `CRYPTO_XMR_ADDRESS` | an XMR integrated address you control | your wallet |
| `CRYPTO_MONTHLY_USD_CENTS` | `500` | fixed |
| `CRYPTO_YEARLY_USD_CENTS` | `5000` | fixed |

Commands (run sequentially — wrangler prompts each time):

```sh
npx wrangler secret put OSL_KEYSERVER_ADMIN_TOKEN
npx wrangler secret put OSL_KEYSERVER_ALLOWED_USERS
npx wrangler secret put LICENSE_HMAC_SECRET
npx wrangler secret put STRIPE_SECRET_KEY
npx wrangler secret put STRIPE_PRICE_ID_MONTHLY
npx wrangler secret put STRIPE_PRICE_ID_YEARLY
npx wrangler secret put CHECKOUT_SUCCESS_URL
npx wrangler secret put CHECKOUT_CANCEL_URL
npx wrangler secret put BILLING_PORTAL_RETURN_URL
npx wrangler secret put RESEND_API_KEY
npx wrangler secret put RESEND_FROM
npx wrangler secret put SUPPORT_EMAIL
npx wrangler secret put CRYPTO_BTC_ADDRESS
npx wrangler secret put CRYPTO_XMR_ADDRESS
npx wrangler secret put CRYPTO_MONTHLY_USD_CENTS
npx wrangler secret put CRYPTO_YEARLY_USD_CENTS
```

`STRIPE_WEBHOOK_SECRET` is set in §7 after the worker is deployed
and the webhook endpoint is registered with Stripe — chicken-and-egg.

---

## §6 First deploy

```sh
npx wrangler deploy
```

Final line of output is the `.workers.dev` URL, e.g.:

```
https://oslprivacy-keyserver.<account-subdomain>.workers.dev
```

Save that URL — you'll use it for §7 and §8. From this point you
can run the §8 smoke tests for everything except the Stripe
webhook path.

---

## §7 Register the Stripe webhook → secret-put the signing secret → re-deploy

The webhook endpoint must exist before Stripe will issue a
signing secret. The signing secret must exist before your worker
will accept events.

1. **Stripe Dashboard → Developers → Webhooks → Add endpoint**
2. Endpoint URL: `https://<your-workers.dev URL>/v1/stripe/webhook`
3. **Events to send** (select these specific ones — don't pick "all
   events"):
   - `checkout.session.completed`
   - `customer.subscription.created`
   - `customer.subscription.updated`
   - `customer.subscription.deleted`
   - `invoice.payment_failed`
   - `invoice.paid`
   - `charge.dispute.created`
4. Click **Add endpoint**.
5. On the new endpoint's page, click **Reveal signing secret**.
6. Paste the `whsec_…` value into wrangler:

```sh
npx wrangler secret put STRIPE_WEBHOOK_SECRET
```

7. **Re-deploy** so the new secret binds to the worker:

```sh
npx wrangler deploy
```

(Wrangler picks up updated secrets at deploy time, not at
secret-put time.)

---

## §8 Smoke test matrix (curl)

Set the URL once **without echoing the admin token to your
shell**. Use `read -s` so the token never enters shell history or
your screen buffer:

```sh
KS=https://oslprivacy-keyserver.<your-subdomain>.workers.dev
read -rs -p "Admin token: " TOKEN; echo
# Now $TOKEN exists for this shell session only. When you exit the
# shell it's gone. It does not appear in `history`.
```

When you're done with the smoke test, close the shell or run
`unset TOKEN`.

### 8.1 Health (public)

```sh
curl -s "$KS/v1/healthz"
# expected: {"ok":true}
```

### 8.2 Unknown user (public)

```sh
curl -s -w "\n%{http_code}\n" "$KS/v1/pubkeys/ghost"
# expected:
#   {"error":"unknown user_id"}
#   404
```

### 8.3 Selector manifest (public, 503 when unconfigured)

```sh
curl -s -w "\n%{http_code}\n" "$KS/v1/selector-manifest"
# expected:
#   {"error":"selector manifest not configured on this keyserver"}
#   503
```

### 8.4 Mutation without token → 401

```sh
curl -s -i -X POST "$KS/v1/register" \
  -H "content-type: application/json" \
  -d '{"user_id":"x","ik_x25519_pub":"x","ik_ed25519_pub":"x","ik_mlkem768_pub":"x","ik_x25519_signature":"x"}' \
  | head -1
# expected: HTTP/2 401
```

### 8.5 Mutation with token + disallowed user → 403

```sh
curl -s -i -X POST "$KS/v1/register" \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"user_id":"mallory","ik_x25519_pub":"eA==","ik_ed25519_pub":"eA==","ik_mlkem768_pub":"eA==","ik_x25519_signature":"eA=="}' \
  | head -1
# expected: HTTP/2 403
```

### 8.6 Register an allowlisted snowflake → 201

Replace `ALLOWED` with one of the snowflakes you put in
`OSL_KEYSERVER_ALLOWED_USERS`:

```sh
ALLOWED=147700845179948241
curl -s -X POST "$KS/v1/register" \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d "{\"user_id\":\"$ALLOWED\",\"ik_x25519_pub\":\"AAAA\",\"ik_ed25519_pub\":\"AAAA\",\"ik_mlkem768_pub\":\"AAAA\",\"ik_x25519_signature\":\"AAAA\"}"
# expected: {"user_id":"…","registered_at":"2026-…Z"}
```

### 8.7 Fetch pubkeys

```sh
curl -s "$KS/v1/pubkeys/$ALLOWED"
# expected: full pubkey row (user_id, ik_x25519_pub, ik_ed25519_pub,
# ik_mlkem768_pub, ik_ratchet_initial_pub:null, registered_at, …)
```

### 8.8 Prekey bundle for an unreplenished user → 404

```sh
curl -s -w "\n%{http_code}\n" "$KS/v1/prekey-bundle/$ALLOWED"
# expected:
#   {"error":"unknown user_id or no prekey bundle uploaded"}
#   404
```

(`POST /v1/prekey-bundle/replenish` and `DELETE /v1/wrapped-keys`
require Ed25519 signatures — covered by the integration tests
locally, not exercised in this curl matrix. The real client will
sign these on first message send.)

### 8.9 Wrapped key insert + fetch

```sh
NOW=$(date -u -d '+5 minutes' +%Y-%m-%dT%H:%M:%SZ 2>/dev/null \
       || date -u -v+5M +%Y-%m-%dT%H:%M:%SZ)   # GNU or BSD date
CID="smoke-$(date +%s)"

curl -s -X POST "$KS/v1/wrapped-keys" \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d "{\"content_id\":\"$CID\",\"content_type\":\"text\",\"sender_id\":\"$ALLOWED\",\"recipient_id\":\"$ALLOWED\",\"session_version\":1,\"share_index\":0,\"wrapped_share_blob\":\"AAAA\",\"blob_version\":1,\"single_use\":false,\"expires_at\":\"$NOW\"}"
# expected: {"content_id":"smoke-…"}

curl -s "$KS/v1/wrapped-keys/$CID"
# expected: full row including the blob, single_use:false
```

### 8.10 Checkout session

```sh
curl -s -X POST "$KS/v1/checkout-session" \
  -H "content-type: application/json" \
  -d '{"plan":"monthly","email":"smoketest@example.com"}'
# expected: {"url":"https://checkout.stripe.com/c/pay/cs_test_…",
#            "session_id":"cs_test_…"}
```

Open the `url` in a browser — confirms Stripe Checkout renders
correctly. **Do not pay yet**; that's §9.

### 8.11 License validate on a nonexistent key

```sh
curl -s -X POST "$KS/v1/license/validate" \
  -H "content-type: application/json" \
  -d '{"license_key":"OSL-2222-3333-4444-5555"}'
# expected: {"status":"UNKNOWN","checksum_ok":false}
# (false because the random body's checksum won't match)
```

### 8.12 Crypto quote

```sh
curl -s -X POST "$KS/v1/crypto/quote" \
  -H "content-type: application/json" \
  -d '{"plan":"monthly","payment_method":"btc","email":"smoketest@example.com"}'
# expected: {"payment_id":"cpay_…","address":"<your BTC addr>",
#            "amount_native":"…","amount_usd_cents":500,
#            "price_locked_at":"YYYY-MM-DD","expires_at":…}
```

If you see `503 no recent price snapshot`, the daily cron hasn't
run yet. Either wait for the next 00:00 UTC tick, or seed today's
price manually:

```sh
TODAY=$(date -u +%Y-%m-%d)
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "INSERT OR REPLACE INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at) VALUES ('btc', '$TODAY', '60000', strftime('%s','now')), ('xmr', '$TODAY', '150', strftime('%s','now'))"
```

After seeding, re-run the quote curl.

---

## §9 End-to-end Stripe test-mode checkout (the canonical proof)

This walks the pipeline checkout → webhook → license → email →
validate. Use Stripe's test card `4242 4242 4242 4242` — no real
money moves.

### 9.1 Mint a checkout session

```sh
EMAIL="<an email inbox you can read>"
RESP=$(curl -s -X POST "$KS/v1/checkout-session" \
  -H "content-type: application/json" \
  -d "{\"plan\":\"monthly\",\"email\":\"$EMAIL\"}")
echo "$RESP"
# Pull the URL out:
URL=$(echo "$RESP" | python3 -c 'import sys,json; print(json.load(sys.stdin)["url"])')
echo "Open: $URL"
```

### 9.2 Complete the Stripe Checkout

Open `$URL` in a browser. On the Stripe page:

- **Email:** `$EMAIL` (pre-filled)
- **Card number:** `4242 4242 4242 4242`
- **Expiry:** any future month/year, e.g. `12/30`
- **CVC:** any 3 digits, e.g. `123`
- **Name on card:** anything
- **Billing address:** anything; ZIP `12345` works

Click **Subscribe**. You're redirected to `CHECKOUT_SUCCESS_URL`.

### 9.3 Verify Stripe fired the webhook

In Stripe Dashboard → Developers → Webhooks → your endpoint, the
event list should show:

- `checkout.session.completed` — **succeeded** (200 from worker)
- `customer.subscription.created` — succeeded
- `invoice.paid` — succeeded

If any show non-200, click it to see the worker's response body.

### 9.4 Verify license email landed

Check `$EMAIL` inbox. Expected subject: `Your OSL license key`,
body containing `OSL-XXXX-XXXX-XXXX-XXXX`.

If the email didn't arrive:
- Check spam.
- Check Resend Dashboard → Emails for delivery status / bounce.
- The license still exists in D1 (§9.5) — recovery path is the
  Customer Portal "resend" button (F2 wires this into the OSL
  client).

### 9.5 Inspect D1 directly

```sh
# Subscriptions — most recent row should be your test purchase:
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "SELECT subscription_id, customer_email, status, current_period_end, datetime(created_at,'unixepoch') AS created FROM subscriptions ORDER BY created_at DESC LIMIT 5"

# Licenses — one row per subscription:
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "SELECT substr(license_hash,1,12) AS hash_prefix, subscription_id, datetime(issued_at,'unixepoch') AS issued, revoked_at FROM licenses ORDER BY issued_at DESC LIMIT 5"

# Stripe events received — idempotency table:
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "SELECT event_id, event_type, datetime(processed_at,'unixepoch') AS processed FROM stripe_events ORDER BY processed_at DESC LIMIT 10"
```

Expected for your test:
- `subscriptions`: status starts `PENDING`, then flips to `ACTIVE`
  once `customer.subscription.created` lands (often same second).
- `licenses`: one row, `revoked_at = NULL`.
- `stripe_events`: 3 rows (`checkout.session.completed`,
  `customer.subscription.created`, `invoice.paid`) at minimum.

### 9.6 Validate the license

Open the email, copy the license key (`OSL-XXXX-XXXX-XXXX-XXXX`),
then:

```sh
curl -s -X POST "$KS/v1/license/validate" \
  -H "content-type: application/json" \
  -d '{"license_key":"OSL-XXXX-XXXX-XXXX-XXXX"}'
# expected: {"status":"ACTIVE","current_period_end":…,
#            "checksum_ok":true}
```

If `status=ACTIVE` and `checksum_ok=true`, the F1.2 pipeline is
end-to-end verified.

### 9.7 Idempotency check

Re-deliver the `checkout.session.completed` event from Stripe's
dashboard (event row → "Resend"). The worker should respond 200
with `deduped:true`:

```sh
# Stripe Dashboard → Webhooks → event → Send test webhook → Re-send
# Worker logs (npx wrangler tail) should show:
#   [webhook] dedup hit, event_id=evt_…
# stripe_events table should still have ONE row for that event_id:
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "SELECT event_id, COUNT(*) FROM stripe_events GROUP BY event_id HAVING COUNT(*) > 1"
# expected: empty result set (no duplicates)
```

### 9.8 Customer Portal session

```sh
LICENSE="OSL-XXXX-XXXX-XXXX-XXXX"   # from §9.4 email
curl -s -X POST "$KS/v1/billing-portal-session" \
  -H "content-type: application/json" \
  -d "{\"license_key\":\"$LICENSE\"}"
# expected: {"url":"https://billing.stripe.com/p/session/test_…"}
```

Open the URL — Stripe Customer Portal should render with cancel /
update-card / view-invoices options.

---

## §10 Watch the cron

The hourly EXPIRED sweep and daily price snapshot run on
`controller.cron`. Tail the worker logs to confirm:

```sh
npx wrangler tail
# Wait for top-of-hour. Should see:
#   [cron] EXPIRED sweep promoted 0 subscription(s)
# At 00:00 UTC additionally:
#   [crypto-prices] persisted btc=… xmr=…
```

To force-test the snapshot path without waiting:

```sh
npx wrangler dev --test-scheduled
# In another terminal:
curl "http://localhost:8787/cdn-cgi/handler/scheduled?cron=0+0+*+*+*"
```

---

## §11 Cleanup

```sh
# Don't leak the admin token to history:
unset TOKEN
unset KS
# Close the notepad / clear the clipboard that held the
# random secrets from §4.
```

---

## §12 Next: F1.4 cutover

Once §0–§11 all pass against the `.workers.dev` URL, proceed to
[`CUTOVER.md`](./CUTOVER.md) to wire `keyserver.oslprivacy.com`
and flip Railway to redirect mode.

---

## Appendix A — What's in `wrangler.toml` vs `wrangler secret put`

**`wrangler.toml` (committed to repo, NOT secret):**

| Field | Type | Why it's not secret |
| --- | --- | --- |
| `name`, `main`, `compatibility_date` | config | worker metadata |
| `[[d1_databases]] database_id` | resource ID | identifies which D1 to bind; access gated by your wrangler OAuth token |
| `[[kv_namespaces]] id` | resource ID | same — identifier, not authorization |
| `[[routes]]` | config | public DNS pattern |
| `[triggers] crons` | config | cron schedule |

**`wrangler secret put` (encrypted, never in repo):**

All the §5 + §7 secrets. Setting these via the secret API stores
them encrypted-at-rest in Cloudflare and exposes them only as
`env.NAME` in the worker runtime.

If you accidentally leak any secret (paste into chat, commit to
git, etc.) treat it as compromised:

1. Rotate it (regenerate, paste new value via `wrangler secret put`).
2. For Stripe / Resend: revoke the leaked key in their dashboards.
3. For `OSL_KEYSERVER_ADMIN_TOKEN`: existing client `keyserver.json`
   files become invalid until you push out new tokens; for the
   beta this means coordinating directly with the affected user.
4. For `LICENSE_HMAC_SECRET`: rotating invalidates *every existing
   license's checksum*. All issued licenses still validate via the
   DB lookup (which doesn't use the HMAC), but client-side typo
   detection will break for them. Avoid unless truly compromised.

---

## Appendix B — F1.1 → F1.4 surface summary

The deployed worker exposes:

| Method | Path | Auth |
| --- | --- | --- |
| GET | `/v1/healthz` | public |
| POST | `/v1/register` | admin token + allowlist |
| GET | `/v1/pubkeys/:user_id` | public |
| POST | `/v1/wrapped-keys` | admin token |
| GET | `/v1/wrapped-keys/:content_id` | public |
| DELETE | `/v1/wrapped-keys` | admin token + Ed25519 sig |
| GET | `/v1/prekey-bundle/:user_id` | public |
| POST | `/v1/prekey-bundle/replenish` | admin token + Ed25519 sig |
| GET | `/v1/selector-manifest` | public |
| POST | `/v1/checkout-session` | public (rate-limited) |
| POST | `/v1/stripe/webhook` | Stripe HMAC signature |
| POST | `/v1/license/validate` | public (rate-limited) |
| POST | `/v1/billing-portal-session` | license-gated |
| POST | `/v1/crypto/quote` | public (rate-limited) |
| POST | `/v1/crypto/submit` | public (rate-limited) |
| POST | `/v1/admin/crypto/confirm` | admin token |

Plus `scheduled()` handler driven by `[triggers] crons`.
