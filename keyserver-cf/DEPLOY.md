# Keyserver Worker deploy runbook

Walks the keyserver-cf deploy end-to-end: provision resources, set
secrets, deploy, register the Stripe webhook, smoke-test every
endpoint, and run one controlled end-to-end live Stripe checkout.

**Secrets handling, non-negotiable:** every credential below goes
through `wrangler secret put <NAME>` — wrangler prompts
interactively, you paste the value at that prompt. **Never** put a
secret into `wrangler.toml`, `.dev.vars`, a `.env` file committed
to git, a shell variable (history leak), or this document. The
only persistent file values are the D1 `database_id` and native
rate-limit namespace IDs, which are resource identifiers, not
credentials.

**Mode for this entire runbook:** Stripe LIVE MODE. Use a least-privilege
restricted key (`rk_live_…`) where possible, or a live secret key
(`sk_live_…`). Test-mode and publishable keys are rejected by the Worker.

---

## §0 Pre-flight

Local checks that must pass before touching production:

```sh
cd keyserver-cf
npm install
npx tsc --noEmit               # must be silent
npx vitest run                  # all tests must pass
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

## §2 Provision D1

This produces a **resource ID** that goes into `wrangler.toml` (not
a secret — it identifies which D1 to bind, while authorization is
provided by your Wrangler login).

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

The `[[ratelimits]]` entries are Cloudflare native bindings. Their
positive-integer namespace IDs must be unique within the Cloudflare
account, but no KV namespace is created. Commit `wrangler.toml` if
you keep this branch on git; resource and namespace IDs are not secrets.

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

## §4 Generate the three random secrets locally

These never leave your machine until you paste them into the
wrangler prompt. Pick the variant matching your shell:

**Node (any OS, wrangler requires node anyway):**

```sh
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
```

Run it twice, once for each random secret. Keep the values open
in a notepad / password manager that you'll clear at end-of-step.

**PowerShell (Windows native, no openssl required):**

```ps1
-join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Maximum 256) })
```

**openssl (macOS / Linux / Git-Bash on Windows):**

```sh
openssl rand -hex 32
```

Use separate values for `OSL_KEYSERVER_ADMIN_TOKEN` and
`LICENSE_HMAC_SECRET`. Public client mutations are authorized by each
registered identity's Ed25519 key. Do not create or distribute a shared
client bearer: an open-source desktop binary cannot keep one confidential.

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
| `LICENSE_HMAC_SECRET` | second 32-byte hex from §4 | generated locally |
| `STRIPE_SECRET_KEY` | restricted `rk_live_…` or secret `sk_live_…` key | Stripe Dashboard, live mode API keys |
| `STRIPE_PRICE_ID_PRO` | `price_…` for the one-time $5 Pro product | Stripe Dashboard → Products → OSL Pro → one-time Pricing |
| `CHECKOUT_SUCCESS_URL` | `https://oslprivacy.com/success?session_id={CHECKOUT_SESSION_ID}` | your site |
| `CHECKOUT_CANCEL_URL` | `https://oslprivacy.com/cancel` | your site |
| `BILLING_PORTAL_RETURN_URL` | `https://oslprivacy.com/pricing` | your site |
| `RESEND_API_KEY` | `re_…` | Resend Dashboard → API Keys |
| `RESEND_FROM` | `OSL <noreply@oslprivacy.com>` (or `licenses@`) | your verified Resend domain |
| `SUPPORT_EMAIL` | `support@oslprivacy.com` | your inbox |
| `TELEGRAM_BOT_TOKEN` | BotFather token | install only with `wrangler secret put` |
| `TELEGRAM_WEBHOOK_SECRET` | at least 32 random characters | use as Telegram's webhook secret token |
| `TELEGRAM_OPERATOR_CHAT_IDS` | comma-separated private chat IDs and optionally one negative group chat ID | obtain from authenticated bot updates; do not accept IDs from a public request as deployment config |
| `CRYPTO_WATCHER_URL` | dedicated HTTPS Cloudflare Tunnel hostname for the watch-only watcher | watcher deployment |
| `CRYPTO_WATCHER_REQUEST_SECRET` | HMAC text used only for Worker invoice requests, at least 32 random characters | `wrangler secret put` |
| `CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY` | base64 Ed25519 public SPKI DER; never the private key | watcher key-generation step |
| `CRYPTO_BTC_CONFIRMATIONS` | `2` | fixed |
| `CRYPTO_XMR_CONFIRMATIONS` | `10` | fixed |
| `CRYPTO_PRO_USD_CENTS` | `500` | tracked non-secret `wrangler.toml` variable; do not accept browser pricing |

Commands (run sequentially — wrangler prompts each time):

```sh
npx wrangler secret put OSL_KEYSERVER_ADMIN_TOKEN
npx wrangler secret put LICENSE_HMAC_SECRET
npx wrangler secret put STRIPE_SECRET_KEY
npx wrangler secret put STRIPE_PRICE_ID_PRO
npx wrangler secret put CHECKOUT_SUCCESS_URL
npx wrangler secret put CHECKOUT_CANCEL_URL
npx wrangler secret put BILLING_PORTAL_RETURN_URL
npx wrangler secret put RESEND_API_KEY
npx wrangler secret put RESEND_FROM
npx wrangler secret put SUPPORT_EMAIL
npx wrangler secret put TELEGRAM_BOT_TOKEN
npx wrangler secret put TELEGRAM_WEBHOOK_SECRET
npx wrangler secret put TELEGRAM_OPERATOR_CHAT_IDS
npx wrangler secret put CRYPTO_WATCHER_URL
npx wrangler secret put CRYPTO_WATCHER_REQUEST_SECRET
npx wrangler secret put CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY
npx wrangler secret put CRYPTO_BTC_CONFIRMATIONS
npx wrangler secret put CRYPTO_XMR_CONFIRMATIONS
```

For multiple operators, enter a strict comma-separated value such as
`1122334455,5566778899,-1001234567890`. The Worker permits multiple positive
private-chat IDs and at most one negative group-chat ID. Empty entries,
non-decimal values, unsafe integers, or more than one group ID fail closed;
duplicates are harmless.

Migration from the old single-chat configuration is non-breaking:

1. Leave `TELEGRAM_ADMIN_CHAT_ID` installed and deploy code that understands
   both settings.
2. Set `TELEGRAM_OPERATOR_CHAT_IDS` and verify commands and proactive alerts
   from each intended private chat or the shared group.
3. Delete the legacy setting with
   `npx wrangler secret delete TELEGRAM_ADMIN_CHAT_ID`.

When present, `TELEGRAM_OPERATOR_CHAT_IDS` always supersedes the legacy value;
a malformed new allowlist never falls back to the old ID.

Remove the retired `CRYPTO_WATCHER_SHARED_SECRET`,
`CRYPTO_MONTHLY_USD_CENTS`, and `CRYPTO_YEARLY_USD_CENTS` bindings only after
the watcher and Worker have both been updated. The private Ed25519 PKCS#8 key
stays solely in the watcher's mode-0600 file.

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
   - `checkout.session.async_payment_succeeded`
   - `customer.subscription.created`
   - `customer.subscription.updated`
   - `customer.subscription.deleted`
   - `invoice.payment_failed`
   - `invoice.paid`
   - `charge.dispute.created`
   - `charge.refunded`
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

Set the URL once **without echoing the client token to your
shell**. Use `read -s` so the token never enters shell history or
your screen buffer:

```sh
KS=https://oslprivacy-keyserver.<your-subdomain>.workers.dev
read -rs -p "Client token: " CLIENT_TOKEN; echo
# Now $CLIENT_TOKEN exists for this shell session only. When you exit the
# shell it's gone. It does not appear in `history`.
```

When you're done with the smoke test, close the shell or run
`unset CLIENT_TOKEN`.

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

### 8.4–8.6 Signed identity routes

Registration and consuming GETs require canonical Ed25519 signatures;
an ad-hoc placeholder `curl` is intentionally rejected. Exercise these
with the integration suite or an actual OSL client. Registration proves
key possession but does **not yet** prove ownership of the claimed Discord
snowflake; Discord OAuth binding remains a release gate.

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
  -H "authorization: Bearer $CLIENT_TOKEN" \
  -H "content-type: application/json" \
  -d "{\"content_id\":\"$CID\",\"content_type\":\"text\",\"sender_id\":\"$ALLOWED\",\"recipient_id\":\"$ALLOWED\",\"session_version\":1,\"share_index\":0,\"wrapped_share_blob\":\"AAAA\",\"blob_version\":1,\"single_use\":false,\"expires_at\":\"$NOW\"}"
# expected: {"content_id":"smoke-…"}

curl -s "$KS/v1/wrapped-keys/$CID"
# expected: full row including the blob, single_use:false
```

### 8.10 Checkout session

Use the website checkout. It generates the browser-only claim capability and
RSA delivery key required by the endpoint, then sends `plan: "pro"`. Direct
email or recurring-plan requests are intentionally rejected.

### 8.11 License validate on a nonexistent key

```sh
curl -s -X POST "$KS/v1/license/validate" \
  -H "content-type: application/json" \
  -d '{"license_key":"OSL-2222-3333-4444-5555"}'
# expected: {"status":"UNKNOWN","checksum_ok":false}
# (false because the random body's checksum won't match)
```

### 8.12 Crypto quote

Keep public BTC/XMR controls disabled. Use the private canary client, which
generates the required browser-held RSA delivery key, and submit only
`plan: "pro"`, `payment_method`, and `delivery_public_key_spki`. The Worker
rejects email, recurring plan names, and browser-provided price or amount.

If you see `503 no recent price snapshot`, the five-minute price cron has not
completed successfully. Diagnose the scheduled handler; for a bounded canary
only, seed today's
price manually:

```sh
TODAY=$(date -u +%Y-%m-%d)
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "INSERT OR REPLACE INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at) VALUES ('btc', '$TODAY', '60000', strftime('%s','now')), ('xmr', '$TODAY', '150', strftime('%s','now'))"
```

After seeding, re-run the quote curl.

---

## §9 End-to-end live Stripe checkout (the canonical proof)

This walks the pipeline checkout → webhook → browser-bound license →
validate. Use one controlled real $5 payment. The Worker deliberately rejects
Stripe test keys and signed test-mode events.

### 9.1 Use the website checkout

```sh
Open `https://oslprivacy.com/pricing` in a current browser and choose the
one-time Pro purchase. The browser generates the claim capability and RSA delivery key
that the API now requires. A direct email-only curl request is intentionally
rejected.
```

### 9.2 Complete the Stripe Checkout

This deployment rejects Stripe test keys and signed test mode events. Use a
real card for a controlled live canary. Complete one $5 payment, confirm
the activation code appears in the same browser tab, then refund the canary in
Stripe if appropriate.

### 9.3 Verify Stripe fired the webhook

In Stripe Dashboard → Developers → Webhooks → your endpoint, the
event list should show:

- `checkout.session.completed` — **succeeded** (200 from worker)

If any show non-200, click it to see the worker's response body.

### 9.4 Verify instant activation delivery

The success page should show `OSL-XXXX-XXXX-XXXX-XXXX` after the signed live
webhook completes. No OSL email is required. If the browser tab that created
checkout is lost, use the Stripe receipt and the manual support recovery path.

### 9.5 Inspect D1 directly

```sh
# Entitlements — the legacy table name is subscriptions, but one-time rows
# contain no customer id, email, recurrence, or expiry:
npx wrangler d1 execute osl-keyserver-prod --remote \
  --command "SELECT subscription_id, status, current_period_end, datetime(created_at,'unixepoch') AS created FROM subscriptions ORDER BY created_at DESC LIMIT 5"

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
unset CLIENT_TOKEN
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
| `[[ratelimits]] namespace_id` | counter namespace | positive integer identifier, not authorization |
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
3. For `LICENSE_HMAC_SECRET`: rotating invalidates *every existing
   license's checksum*. All issued licenses still validate via the
   DB lookup (which doesn't use the HMAC), but client-side typo
   detection will break for them. Avoid unless truly compromised.

---

## Appendix B — F1.1 → F1.4 surface summary

The deployed worker exposes:

| Method | Path | Auth |
| --- | --- | --- |
| GET | `/v1/healthz` | public |
| POST | `/v1/register` | Ed25519 self/rotation proof; Discord ownership not yet bound |
| GET | `/v1/pubkeys/:user_id` | public |
| POST | `/v1/wrapped-keys` | fresh registered-sender Ed25519 signature |
| GET | `/v1/wrapped-keys/:content_id` | public reusable / signed recipient for one-use |
| DELETE | `/v1/wrapped-keys` | registered-sender Ed25519 signature |
| GET | `/v1/prekey-bundle/:user_id` | registered-requester Ed25519 signature |
| POST | `/v1/prekey-bundle/replenish` | registered-owner Ed25519 signature |
| GET | `/v1/selector-manifest` | public |
| POST | `/v1/checkout-session` | public (rate-limited) |
| POST | `/v1/stripe/webhook` | Stripe HMAC signature |
| POST | `/v1/license/validate` | public (rate-limited) |
| POST | `/v1/billing-portal-session` | license-gated |
| POST | `/v1/crypto/quote` | public (rate-limited) |
| POST | `/v1/crypto/status` | anonymous claim token (rate-limited) |
| POST | `/v1/internal/crypto/settle` | timestamped watcher Ed25519 signature |

Plus `scheduled()` handler driven by `[triggers] crons`.
