# Server lane — Cloudflare Workers hardening — 2026-07-26

Owner: server lane (tab 5). Exclusive scope: `cipher-store-cf/**`, `keyserver-cf/**`.
No Rust, no website, no build checklist, no `.github` was touched.

Audit source: `docs/security/osl-audit-2026-07-26-codex.md`.

**Nothing in this report has been deployed.** Every change is staged in the working
tree, typechecked, and tested locally. The exact ordered deploy sequences and their
rollbacks are in `cipher-store-cf/DEPLOY.md §7b` and `keyserver-cf/DEPLOY.md §11b`.

---

## Resume here

If you are picking this lane up cold, this is the state:

| Item | State |
|---|---|
| HIGH-1 empty multipart reservations | **Fixed, staged, proved on a real workerd** (`cipher-store-cf`, migration 0006 + Worker) |
| HIGH-2 non-atomic rate limiter | **Fixed, staged, proved on a real workerd** (`cipher-store-cf`, migration 0006 + Worker) |
| **NEW — attachment uploads were broken on the real runtime** | **Pre-existing, not from these changes. Found by the probe, fixed** — see "Found during verification" |
| Control-inbox reserve was check-then-act | **Closed** — admission cap moved into the insert statement, no migration |
| Post-deploy probes for both workers | **Written and passing** against a local workerd (`scripts/post-deploy-probe.mjs` in each) |
| Standing smoke suite | **`scripts/osl-server-smoke.sh`** — runs both probes, ties evidence to a build identity, redacts token-shaped hex, writes a dated file to `docs/evidence/server-smoke/` |
| Attachment defect age | **Established: never worked in any committed state.** `attachment.ts` has two commits total — see "How far back it goes" |
| Keyserver limiter — same KV race? | **Verified no.** It already uses Cloudflare's native rate-limit binding, not KV read/modify/write |
| Residual of the open-registration critical | **Still open, documented** — 0029 closed the snowflake vector, not identifier binding |
| HIGH-2b generic blobs had no aggregate quota | **Fixed, staged, proved** (`cipher-store-cf`, Worker only) |
| MEDIUM cross-sender control-inbox eviction | **Fixed, staged, proved** (`keyserver-cf`, Worker only) |
| MEDIUM pubkeys lifecycle leakage | **Partly fixed, staged, proved.** `last_rotated_at` removed; `registered_at` coarsened. Full removal is blocked on a one-line Rust change — see "Cross-lane" below |
| MEDIUM migration 0002 doc contradiction | **Corrected** (`cipher-store-cf/migrations/0002_fetch_token.sql`) |
| HIGH scope-wide prose-token delete | **Not fixed, by instruction.** Fix spans `crates/ipc`; exact required change written out below |
| Deploy | **Blocked on owner approval.** One `action-needed` event sent |
| Migration 0028 link-grant lane | **Left dark.** `LINK_GRANT_ENABLED` still defaults off; not touched |
| Migration 0027 stale `NOT DEPLOYED` header | **Not touched.** Not on this work list; flagged below |

Verification state at hand-off, both workers clean:

```
cipher-store-cf   tsc --noEmit: exit 0    vitest: 10 files / 87 tests passed   (baseline 8 / 75)
keyserver-cf      tsc --noEmit: exit 0    vitest: 39 files / 381 tests passed  (baseline 38 / 377)
keyserver-cf      vitest --config vitest.node.config.ts: 1 file / 3 tests passed
```

Plus — and this is the part that changed my confidence — the cipher-store fixes are
now proved against a **real workerd**, not only against unit tests. All six
migrations applied to a local D1, `wrangler dev --local`, then the post-deploy
probe:

```
PASS Probe A DECISIVE  reservation pool is isolated from stored content
                       (first4=201,201,201,201  fifth=503/storage_capacity  direct=201)
PASS Probe B guard     multipart receipts agree and content is retrievable
                       (session=201 part=201 complete=201 expires_agree=true get=200/1024b)
PASS Probe C DECISIVE  mutation rate limiting is atomic (non_429=18  429=22)
SUMMARY PASS passed=3 failed=0
```

Probe C's number is worth reading closely: probes A and B had already consumed 6
session creations, and C was admitted exactly 18 more. 6 + 18 = 24, the budget,
exactly. That is the ceiling being real, not approximately real.

### Production status — what is confirmed, and what is not

Both Workers were deployed by the owner: **cipher-store `3374d057`**, **keyserver
`3f92f0f5`**. Three different confidence levels apply, and they should not be
collapsed:

| Fix | Status | Evidence |
|---|---|---|
| HIGH-1 session budget | `verified-live` | Owner confirmed on the production hostname: `held=900`, `promised=604800` — the reclaim deadline and the promised content TTL are exactly the two distinct values the fix introduces |
| HIGH-2 atomic limiter | `deployed, not independently confirmed live` | Ships in the same Worker version; the decisive concurrency check has only been run against local workerd |
| Control-inbox + pubkeys | `deployed, not independently confirmed live` | Keyserver `3f92f0f5`; the pubkeys check is free to run (one `curl`) |
| **R2 upload fix** | **`unknown — must be checked`** | See below |

**The R2 fix's live status is genuinely unknown and matters most.** The quota fix
and the R2 fix both live in `attachment.ts` but were written hours apart, so a
deploy cut from the tree between them would carry one without the other.
`held=900` proves only the quota fix. The discriminator is a single small upload,
because the old code returns 500 where the new one returns 201:

```sh
T=$(openssl rand -hex 16)
ID=$(curl -sX POST https://ciphers.oslprivacy.com/v1/attachment \
  -H "X-OSL-TTL-Seconds: 3600" -H "X-OSL-Fetch-Token: $T" \
  --data-binary $'\x01\x02\x03\x04' | jq -r .id)
curl -sX DELETE "https://ciphers.oslprivacy.com/v1/attachment/$ID" \
  -H "X-OSL-Fetch-Token: $T" -i | head -1
```

201 plus an id means the fix is live. 500 means the deployed build predates it and
attachments are still broken in production.

Local `runtime-proven` remains true for all cipher-store fixes: a real workerd on
this machine with all six migrations applied, 3/3 probes passing.

Every fix below has a test that was **observed failing against the unmodified code
first**. The captured baseline failures are quoted with each finding.

---

## How these were tested, and why that matters

The pre-existing `cipher-store-cf` tests use hand-rolled `DB.prepare` fakes that
model only the statements they were written for. That is fine for transport
behaviour and useless for proving a quota, because such a fake can only re-assert
whatever the test author already believed. A test for "sixteen bodyless requests
exhaust the budget" written against a fake proves nothing.

So `cipher-store-cf/test/helpers/d1.ts` is new: a D1 shim over `node:sqlite` that
applies the real `migrations/*.sql` and executes the Worker's real SQL. CHECK
constraints, the conditional-INSERT admission predicates and the `ON CONFLICT`
counter all behave as they do in production. It also models D1's concurrency
contract deliberately — every call yields to the event loop *before* its statement
and never inside it — which is exactly the property the rate-limiter fix depends
on. No new dependency: `node:sqlite` is in the Node runtime already.

`keyserver-cf` already runs against real D1 + real migrations under
`@cloudflare/vitest-pool-workers`, so its tests use that.

One thing this does **not** establish: none of it is a live-deployment proof. These
are local runs against local databases. Status for everything below is
`test-proven-only`, not `verified-live`.

---

## HIGH-1 — Sixteen empty multipart reservations exhausted the global attachment quota for seven days

### What was actually wrong

`POST /v1/attachment/session` is public. It accepted a caller-chosen 128-bit
capability and a declared size up to 513 MiB, created an R2 multipart upload, and
immediately inserted the **declared** size as a reservation carrying the caller's
full content TTL — before one byte of ciphertext existed
(`cipher-store-cf/src/endpoints/attachment.ts:170-209` pre-change). Global admission
was 512 rows or 8 GiB (`:129-167`, `src/lib/attachment-limits.ts:1-14`), and the
route shared the 140/hour `attachment-upload` bucket (`src/index.ts:156-165`).

8 GiB ÷ 512 MiB = 16. Sixteen bodyless POSTs, comfortably inside the hourly bucket,
reserved the entire global budget, and because the row carried the content TTL, the
resulting `503 storage_capacity` for every other user lasted seven days.

### Baseline failure, before any change

```
× bodyless multipart reservations cannot deny service to real uploads
  AssertionError: expected 503 to be 201
× holds an unfinished reservation for minutes, not for the content TTL
  AssertionError: expected 604800 to be less than or equal to 900
```

### The fix — four layers

**1. A reservation is no longer the same thing as stored content.** Rows whose
state is not `ready` are admitted against their own pool — 64 rows and 2 GiB
(`src/lib/attachment-limits.ts`) — as well as the existing global backstop. The
predicate is part of the same single conditional INSERT, so it cannot race
(`src/endpoints/attachment.ts`, `insertObject`). Filling the reservation pool
completely now leaves 448 rows and 6 GiB for real attachments, so the denial-of-
service against *everyone else* is gone rather than merely reduced.

**2. An unfinished session holds capacity for 15 minutes, not seven days.** The row
carries two times now. `expires_at` is the reclaim deadline; the new
`content_expires_at` column holds the expiry the caller was promised, and it is
applied to `expires_at` only on successful completion. The existing five-minute
sweep already aborts the R2 multipart and deletes the row, so an abandoned
reservation self-heals without an operator.

**3. Genuine slow uploads are not punished for this.** Each accepted part slides
the reclaim deadline forward by another 15 minutes, bounded by the promised content
expiry. Only a session that stops making progress is reclaimed.

**4. Session creation has its own small per-address budget:** 24/hour, separate
from the 140/hour that parts and completion draw on
(`src/lib/rate-limit.ts`, `src/index.ts`). Two consequences. A flood of creations
can no longer consume the budget a caller needs to *finish* an upload already in
flight. And because that budget is now atomically enforced (HIGH-2), holding the
64-slot pool full requires at least eleven distinct addresses sustained
indefinitely, instead of sixteen requests once.

### The wire constraint that shaped this

The obvious implementation — return a short session expiry, then a real one at
completion — breaks the shipping client, and I only found this by reading it.
`crates/ipc/src/cipher_store_client.rs:213-235` declares both response structs
`#[serde(deny_unknown_fields)]`, so no new field can be added to the session
receipt; and `:515-525` rejects the upload outright if
`complete.expires_at != session.expires_at`. So the promised expiry must be
computed once, at creation, reported identically in both receipts, and stored
server-side in the meantime. That is precisely why `content_expires_at` exists as a
column rather than as an arithmetic expression. There is a regression test pinning
this contract.

### What is still not done here

The audit also asked for "an authenticated or anonymously issued creation
capability". I did not ship one, and I want to be exact about why rather than let
it look complete. This Worker's core posture is that it has no identity binding at
all (`src/env.ts:1-7`, `src/index.ts:12-19`), and every existing client creates
sessions unauthenticated. Adding a required creation capability is a protocol
change across `crates/ipc` — not my lane — and shipping it fail-closed before the
client signs would break all attachment uploads. Adding a dark default-off verifier
would add attack surface today for no benefit today.

The four layers above stand on their own: they convert "16 requests, global outage,
seven days" into "sustained traffic from ≥11 addresses, degrades only new multipart
session creation, self-heals in 15 minutes, never touches stored content." The
capability is the right next increment, not a precondition. Its exact shape is in
"Cross-lane" below.

### Files

- `cipher-store-cf/migrations/0006_session_budget_and_atomic_rate_counters.sql` (new)
- `cipher-store-cf/src/lib/attachment-limits.ts`
- `cipher-store-cf/src/endpoints/attachment.ts`
- `cipher-store-cf/src/index.ts`
- `cipher-store-cf/test/attachment-session-budget.test.ts` (new)
- `cipher-store-cf/test/helpers/d1.ts` (new)

---

## HIGH-2 — The rate limiter was a read/modify/write race

### What was actually wrong

`KV.get` → compare `used` → `KV.put(used + 1)`, across two awaits
(`src/lib/rate-limit.ts:95-125` pre-change). KV is eventually consistent by design,
which the code's own comment conceded while claiming the ceiling still held at
`budget × pop_count`. It does not hold even within one POP: concurrent invocations
all read the same value and collapse into a single increment. Separately, generic
blob insertion had no aggregate row or byte quota behind the limiter at all
(`src/endpoints/blob.ts:99-112`), so the limiter was the *only* bound on that
table — and a per-address bound is not a storage bound, it scales with the number
of addresses.

### Baseline failures, before any change

```
× admits no more than the budget when requests race
  AssertionError: expected 700 to be 600
× keeps a separate, much smaller budget for multipart session creation
  AssertionError: expected 200 to be less than or equal to 24
× never records a raw client address in the limiter's durable state
  Error: no such table: rate_counters
× refuses new blobs once the aggregate byte budget is reached
  AssertionError: expected 201 to be 503
```

A note on the first one, because it nearly produced a false pass. My initial
concurrency fake resolved `get` and `put` on the microtask queue, and the test
passed against the *unfixed* code — Node's WebCrypto awaits inside the key
derivation were serialising the calls, so nothing ever raced. The fake now models
what KV actually does: a `put` becomes visible several event-loop turns after it
resolves. That is what makes 700 concurrent calls all read zero.

### The fix — atomic where it matters, unchanged where it does not

**Mutation buckets** (`upload`, `delete`, `attachment-upload`, `attachment-session`,
`attachment-delete`, `link-create`) now increment through one conditional D1
statement:

```sql
INSERT INTO rate_counters (bucket_key, window_start, used)
VALUES (?, ?, 1)
ON CONFLICT(bucket_key) DO UPDATE SET used = used + 1
  WHERE rate_counters.used < ?
RETURNING used
```

The check and the increment cannot be separated, and D1 serialises writers, so the
budget is an exact ceiling rather than an approximation.

**Read buckets stay on KV.** They bound cost and scraping, not integrity; they are
the highest-volume routes; and they must keep failing *open*, because a limiter
outage must never make already-stored ciphertext unfetchable. Paying a D1 write on
every fetch to tighten an availability-only control is the wrong trade. Write paths
already write D1, so for them the extra statement is nearly free.

**Generic blobs got the database-level backstop** the audit asked for: the insert
now carries `COUNT(*) < 100000 AND SUM(size_bytes) <= 2 GiB - size` in the same
statement (`src/lib/blob-limits.ts`, `src/endpoints/blob.ts`). A primary-key
collision still raises a constraint error and is handled by the existing retry, so
zero affected rows means capacity and nothing else.

### Why not a Durable Object

The audit offered three primitives. I chose the D1 counter over a DO deliberately,
and the reason is operational rather than technical: introducing a DO class means a
`[[migrations]]` tag in `wrangler.toml`, and a deploy that creates a DO class
cannot be undone with `wrangler rollback` — reverting to a Worker that does not
export the class fails, and removing it needs a `deleted_classes` migration that
destroys the data. Given that the hard constraint on this lane is "stage it, write
the rollback, do not deploy", handing the owner a one-way door would have been the
wrong deliverable. The D1 counter is atomic, needs no new binding, and rolls back
with a plain `wrangler rollback`.

The provider-native rate-limiting binding was ruled out on capability: its `simple`
period only supports 10 or 60 seconds, and these are hourly budgets.

If per-request D1 cost on write paths ever becomes a problem, the DO is the right
next step, and the `rateLimit()` signature was kept unchanged so it is a
single-module swap.

### The privacy posture question I had to answer

`wrangler.toml` and `src/env.ts` both state that rate-limit state lives in KV and is
"never persisted to D1". Moving mutation counters into D1 contradicts the letter of
that. I judged it a restatable comment rather than a property being weakened, and
recorded the reasoning in the migration:

- `bucket_key` is the *same opaque value the KV key already used* — a truncated
  HMAC of (bucket, client address) under a server-only key. No address, and nothing
  derivable from one without the secret, is stored.
- Retention is **shorter** than before: KV entries lived for 2× the window; these
  rows are deleted by the existing five-minute cron as soon as their window closes
  (`sweepRateCounters`).

There is a test asserting no raw address appears in either store. If the owner
disagrees with this reading, the alternative is the DO — same atomicity, same
opacity, with the rollback hazard above.

### Files

- `cipher-store-cf/migrations/0006_session_budget_and_atomic_rate_counters.sql` (new)
- `cipher-store-cf/src/lib/rate-limit.ts`
- `cipher-store-cf/src/lib/blob-limits.ts` (new)
- `cipher-store-cf/src/endpoints/blob.ts`
- `cipher-store-cf/src/index.ts`
- `cipher-store-cf/test/rate-limit-atomic.test.ts` (new)
- `cipher-store-cf/test/rate-limit.test.ts`, `test/link-lane.test.ts`,
  `test/blob-bounds.test.ts`, `test/attachment-r2.test.ts` (fakes updated — see
  "Test-fake changes" below)

---

## MEDIUM — Any registered sender could evict another sender's undelivered control messages

### What was actually wrong

The ordinary lane answered a full recipient inbox by deleting the oldest
undelivered rows **for that recipient, regardless of who sent them**
(`keyserver-cf/src/endpoints/control-inbox.ts:419-426` pre-change). Registration is
open, so an attacker with a handful of identities could silently destroy an offline
victim's pending SKDM/control state — and the protected messages depending on it
became unopenable, with no error to either side.

### Baseline failure, before any change — the attack, reproduced

```
× keeps a victim's queued row when an attacker fills the recipient's lane
  AssertionError: victim row a4bb2e1636328e54f22b0a16ae22620b was evicted:
  expected +0 to be 1
```

### The fix — two caps, two different answers

**Per pair: still evict.** A sender's 33rd undelivered message to one person
displaces that same sender's own stalest one. Refusing here would punish the sender
for something only the recipient can fix, and the original comment's reasoning is
correct: these rows drain on recipient action, not on a timer, so a recipient who
never runs OSL would block the sender for the whole seven-day TTL. Recycling your
own slot is safe because the loss falls on the party who chose to keep sending.
Eviction now runs *first*, so a chatty sender recycles before admission is judged.

**Recipient-wide: refuse.** `429 recipient_inbox_full`, which the client already
handles (`crates/keystore/src/client.rs`). Cross-sender data loss is strictly worse
than a sender learning it must retry.

**Reserved headroom, so refusal does not become the new attack.** Refusal alone
converts "attacker deletes your rows" into "attacker makes the recipient
unreachable". 128 of the 512 slots are held back from any sender already holding
four or more rows, so 32 distinct senders can still get a first contact through a
congested lane. `evictOldestPending` now carries an invariant comment that its
predicate must always be sender-scoped.

### A contract change I made deliberately

`test/integration/control-inbox.test.ts:143` previously asserted `201` for exactly
this case, with a comment explaining that eviction was the intended design. That
comment is the defect, written down. I changed the assertion to `429` and replaced
the rationale. Flagging it explicitly because it is the one place where I overrode
an existing, deliberate, documented decision rather than filling a gap.

The cost, stated plainly: a sender holding four or more undelivered rows to a
recipient whose inbox is above 384 rows now gets a retryable refusal where it
previously got silent success at a third party's expense. Reaching that state
requires a recipient offline long enough to accumulate hundreds of undelivered
control rows.

### What this still does not fix

Open registration is the root cause and is untouched. An attacker with enough
identities can still fill a recipient's ordinary lane and hold it full — they just
cannot destroy anything doing it. The audit's "recipient-issued mailbox
capabilities or an authenticated relationship" is the real fix and spans the Rust
client; see "Cross-lane".

### Files

- `keyserver-cf/src/endpoints/control-inbox.ts`
- `keyserver-cf/test/integration/control-inbox-cross-sender.test.ts` (new)
- `keyserver-cf/test/integration/control-inbox.test.ts`

---

## MEDIUM — Public lookup leaked adoption and lifecycle timing

### What was actually wrong

`GET /v1/pubkeys/:user_id` is public and unauthenticated, and returned
`registered_at` and `last_rotated_at` (`src/endpoints/pubkeys.ts:40-56`). Migration
0029 already closed the enumeration half by refusing Discord snowflakes and making
identities opaque. What remained: anyone holding an identifier could read that
account's lifecycle activity for free.

### Baseline failure, before any change

```
× does not publish identity lifecycle timing
  AssertionError: expected { user_id: 'alice-lifecycle', …(8) }
  to not have property "last_rotated_at"
```

### The fix, and the part of it that is blocked

`last_rotated_at` is **removed**. It is the live-activity signal, and it is
`Option<String>` in every consumer (`crates/keystore/src/client.rs:345`,
`crates/ipc/src/commands.rs:901`), so its absence deserialises cleanly.

`registered_at` is **coarsened to UTC date granularity**, not removed. It cannot be
removed from this lane: it is a required, non-`Option` `String` in
`crates/keystore/src/client.rs:344` and `crates/ipc/src/commands.rs:900`, so a
missing key fails serde and breaks **every key fetch on every deployed client**.
Deploying that ahead of the Rust change would be a production outage, so it is
staged as step 2 in "Cross-lane".

I checked before coarsening: the value is plumbed into the IPC DTO at
`crates/ipc/src/commands.rs:1032` and is not rendered anywhere in the hub UI or the
original client, so reducing its resolution changes no user-visible text.

### Files

- `keyserver-cf/src/endpoints/pubkeys.ts`
- `keyserver-cf/test/integration/pubkeys.test.ts`

---

## MEDIUM — Migration 0002 claimed a property the implementation does not have

`cipher-store-cf/migrations/0002_fetch_token.sql:3-8` said the fetch token was
"computed client-side from data only the sender + recipients of a specific
conversation possess." That is false for the prose-token lane, and it is the kind
of false that makes a reader mis-scope a security decision.

Corrected in place, with the specific contradiction spelled out: HKDF over
`Scope::storage_key()` or the Discord DM channel id takes **no secret input**
(`crates/ipc/src/prose_token.rs:19-26`, `:97-133`), the source itself notes anyone
knowing the scope can recompute it, and the token is derived against an all-zero
placeholder so it is per-**scope**, not per-blob (`:181-224`). The corrected comment
states the three consequences a schema reader must not be misled about, and points
out that the attachment lane (0004, digest-only) and the view-once lane (0005, key
never reaches the server) are genuinely stronger and are not described by it.

Editing an applied migration's comments is safe — D1 tracks applied migrations by
filename and the SQL is unchanged.

---

## Not fixed by instruction — scope-wide prose-token delete capability

The exact required change, for whoever owns `crates/ipc`.

**The defect.** `derive_scope_primitives` (`crates/ipc/src/prose_token.rs:19-26`,
`:97-133`) runs HKDF over public conversation metadata with no secret input. Upload
derives one token from that public value and an all-zero placeholder (`:181-224`),
so the token is identical for every blob in the scope. `prose_token_burn_id`
derives the same value and sends DELETE (`:330-348`), and the Worker authorises
deletion by comparing that token alone (`cipher-store-cf/src/endpoints/blob.ts:238-272`).
A hostile channel member — or anyone who learns the channel/server ids — derives
the cover-decoding key, extracts the 64-bit blob id, and destroys the ciphertext
before its recipients fetch it. AEAD never gets a chance to help.

**Required change, in order:**

1. **Secret input to the KDF.** `derive_scope_primitives` must take per-conversation
   secret state, not `Scope::storage_key()` or a channel id. Public scope ids may
   remain as domain separation, never as the sole entropy.
2. **Per-blob binding.** Replace the all-zero placeholder at
   `prose_token.rs:181-224` with the blob id, so a capability authorises one object.
3. **Split read from delete.** Derive two independent values from the same secret
   under distinct domain labels — `fetch` for recipients, `delete` for the author.
   Recipients receive only the first.
4. **Server side (this lane, one line each, once 1–3 land).** Add a second column
   alongside `fetch_token` holding the delete capability digest, and make
   `handleDelete` compare against that column instead of the fetch token. This is
   trivial and I will do it — it is sequenced after the client change because the
   Worker cannot invent a capability the client does not send.
5. **Migration.** Legacy rows have no delete capability. Simplest safe answer: rows
   written before the cutover keep the old behaviour and TTL out within seven days;
   do not backfill a derived value, because a derived value has the same defect.

Note the interaction with step 4: until it lands, a delete capability that is
*also* the fetch capability leaks delete authority to every recipient. Steps 1–3
alone narrow the attacker from "anyone who knows the channel" to "any recipient",
which is an improvement but not the fix. Ship 1–5 together.

---

## Cross-lane items this lane cannot complete

**1. `registered_at` removal (crypto lane, one line, then one line here).**
Change `crates/keystore/src/client.rs:344` and `crates/ipc/src/commands.rs:900`
from `pub registered_at: String` to `pub registered_at: Option<String>`, update the
five test fixtures that construct it, then this lane deletes the field from
`pubkeys.ts` and the assertion from `pubkeys.test.ts`. Client first, server second —
the reverse order is an outage.

**2. Anonymous session-creation capability (crypto lane + this lane).**
Mirror the link-grant pattern that already exists: the keyserver signs a short-lived
Ed25519 grant bound to the declared size and a creation nonce; the cipher-store
verifies it with a `ATTACHMENT_SESSION_PUBKEY_B64` secret, exactly as
`lib/link-grant.ts` does today. Roll out in three deploys — server accepts-and-
ignores, client starts sending, server enforces — so no step is fail-closed against
a client that has not shipped. Not started; the four layers in HIGH-1 do not depend
on it.

**3. Recipient-issued mailbox capabilities for the control inbox (crypto lane).**
The real fix for open registration filling a recipient's lane. Recipient issues a
signed, revocable per-sender mailbox capability out of band; `POST /v1/control-inbox`
requires it for the ordinary lane. Until then, this lane's reserved headroom is a
mitigation, not a cure.

---

## Test-fake changes, disclosed

Four pre-existing `cipher-store-cf` test files needed their fakes updated because
they modelled the old SQL positionally. These were fake-drift, not behaviour
regressions, and I want them visible rather than buried:

- `test/attachment-r2.test.ts` — the fake destructured INSERT binds by position and
  `content_expires_at` shifted them; the completion UPDATE now binds the expiry
  first, so the id moved to `values[1]`; added handling for the deadline-sliding
  UPDATE and the unbound reservation-pool probe.
- `test/blob-bounds.test.ts` — its fake returned no `meta.changes`, which the new
  conditional insert reads as "at capacity". Now returns `changes: 1`.
- `test/link-lane.test.ts` — `link-create` is a mutation bucket now, so its limiter
  env needs a D1; the key-opacity test was extended to assert opacity in *both*
  stores rather than only KV.
- `test/rate-limit.test.ts` — the KV-key test now uses a read bucket (the one that
  still uses KV) and gained a sibling asserting the same property for the D1
  counter.

I considered converting all of these to the real-SQLite shim and decided against it
for now: the risk of silently weakening a 718-line assertion set outweighs the
tidiness. The security properties are proved against real SQL in the new files;
these keep proving transport behaviour.

---

## Found during verification — attachment uploads were broken on the real runtime

**This is pre-existing and separate from the audit. It is also the most serious
thing in this report.**

Running the new probe against a local workerd, `POST /v1/attachment` and
`PUT /v1/attachment/{id}/part/{n}` both returned 500. The swallowed exception was:

```
TypeError: Provided readable stream must have a known length
            (request/response body or readable half of FixedLengthStream)
  at handleAttachmentUpload (src/endpoints/attachment.ts:443)
```

`boundedAttachmentStream` piped the request body through a counting
`TransformStream` and handed the result to `put`/`uploadPart`. workerd requires a
streamed R2 body to carry a known length, and the output of `pipeThrough` does
not. workerd is the same runtime in production, so this is not a local quirk:
**every attachment upload has been failing.**

I verified it is not mine before doing anything else — the `put`, `uploadPart`
and `boundedAttachmentStream` call sites are byte-identical to `git HEAD`
(`git show HEAD:cipher-store-cf/src/endpoints/attachment.ts`). It does not
interact with the audit fixes and does not make the staged deploy riskier.

**Why the test suite was green the whole time.** The R2 test double accepts any
stream. The suite proved the code did what its author believed and never touched
what the runtime requires. That is the same failure mode as the hand-rolled D1
fakes, and it is why the probe exists.

**Fix.** Both paths now read the body into memory under their existing ceiling
and hand R2 a `Uint8Array`, which has a known length by construction. Validation
also moved ahead of storage, so an invalid upload no longer creates an R2 object
that has to be cleaned up afterwards.

I deliberately did **not** use `FixedLengthStream`, which would preserve true
streaming. It is a workerd global that does not exist under this package's
Node-based test runner, so using it would mean the tested path and the shipped
path differ — exactly the gap that hid this bug. Buffering keeps one path for
both. The cost is bounded and small: 8 MiB for a multipart part, 26 MiB for a
direct upload. A 512 MiB attachment is still never held in memory; it arrives as
up to 65 separately bounded parts. If cipher-store ever gains a
`@cloudflare/vitest-pool-workers` environment like the keyserver already has,
switching to `FixedLengthStream` becomes safe and is worth doing.

A unit guard now asserts R2 receives a `Uint8Array` and not a `ReadableStream`.
That is a proxy for the real property — a test double cannot enforce workerd's
rule — so the end-to-end proof remains the probe.

### How far back it goes, and what it invalidates

**Verdict: the R2 attachment upload path has never worked in any committed state
of this repository. There is no window in which it functioned.**

`cipher-store-cf/src/endpoints/attachment.ts` has had exactly two commits in its
entire history:

| Commit | Date | Subject |
|---|---|---|
| `08552e5` | 2026-07-26 10:02:35 -0700 | WIP snapshot: encrypted send working, eye decode one fix away |
| `b6f456e` | 2026-07-26 | server lane: this fix |

The first committed version already contained the defect in identical form —
`pipeThrough` at line 90, feeding `uploadPart` at 275 and `put` at 368. So the
file was born broken.

One honest bound: `08552e5` is a *WIP snapshot*, which means the code existed
uncommitted for an unknown period before it. Git cannot see that period, so the
true age is unbounded below. What git does establish is that **no committed
version ever worked**, and no tag contains a working one (6 tags exist; none
predate the file with a functioning variant, because no functioning variant
exists).

**Confirmed live 2026-07-26.** The owner probed production and got the 500 —
`3374d057` had been cut *before* the fix existed, so the fix was local-only.
Redeployed as **`0a17547d`**; part upload now returns
`201 {"part_number":1,"size_bytes":1024}`. Attachment upload works in production
for the first time.

**The bound is structural, not temporal.** Every artefact of the attachment lane
entered git in the *same* commit `08552e5` — the endpoint, migrations 0003 and
0004, the `osl-cipher-attachments-prod` R2 binding in `wrangler.toml`, and the
Rust multipart client in `crates/ipc/src/cipher_store_client.rs`. There is no
earlier attachment implementation anywhere in history that might have worked, and
no separate upload route: `cipher_store_client.rs` is the only caller of
`/v1/attachment*`. So the answer to "how far back" is not a date. It is: **for the
entire committed existence of the feature, an attachment body could not reach R2.**

Residual uncertainty, stated precisely: `08552e5` is a WIP snapshot, so the code
lived uncommitted for a window git cannot see. Everything in that window is the
same broken code — the first committed version already had it — so the only way
exposure could be non-nil is if a *differently implemented* attachment path was
deployed and later replaced without ever being committed. Two read-only checks
close that, and both are cheap:

```sh
# 1. Has any attachment ever completed? A `ready` row is the only way one can exist.
npx wrangler d1 execute osl-cipher-store-prod --remote --command \
  "SELECT state, COUNT(*) AS rows, SUM(size_bytes) AS bytes,
          MIN(created_at) AS oldest, MAX(created_at) AS newest
     FROM attachment_objects GROUP BY state"

# 2. Ground truth: does the bucket hold anything that predates today's probes?
npx wrangler r2 object list osl-cipher-attachments-prod
```

Neither proves "never" on its own, because rows and objects are swept at TTL
(7 days maximum). What they do is corroborate the code argument with physical
evidence for the last week. Code argument plus empty bucket is as close to proof
as this can get without Cloudflare-side deploy history.

**A second-order effect worth checking while you are in there.** Under the old
code a session creation succeeded and then every part upload threw, and that
throw did **not** delete the reservation row (`08552e5`, part-upload catch: it
only cleans up on `attachment_too_large`, otherwise it rethrows). So each failed
upload attempt could leave an `uploading` row holding its declared size for the
full content TTL — HIGH-1's pathology occurring naturally rather than
adversarially. I initially expected a pile of these and checked before saying so:
the Rust client calls `delete_attachment` on failure
(`cipher_store_client.rs`), and that path does not touch the broken stream code,
so a well-behaved client cleaned up after itself. Orphans would only remain from
clients killed between session-create and delete. Query 1 above shows whether any
exist. If it reports stuck `uploading` rows, do **not** delete them directly —
that orphans the R2 multipart uploads, which are billable and invisible. Expire
them instead and let the existing sweep abort the multipart properly:

```sh
npx wrangler d1 execute osl-cipher-store-prod --remote --command \
  "UPDATE attachment_objects SET expires_at = unixepoch()
    WHERE state <> 'ready' AND created_at < <epoch-of-0a17547d-deploy>"
```

Note migration 0006 backfilled `content_expires_at = expires_at` for pre-existing
rows but deliberately did not shorten their `expires_at`; the 15-minute hold
applies to sessions created *after* the fix. Legacy stuck rows therefore keep
their original TTL until swept or expired by hand.

**What this invalidates — in descending order of how much it should worry you:**

1. **Every test that "proves" attachment upload.** The whole cipher-store
   attachment suite, and `apps/osl-hub/tests/peer_attachment_network_e2e.rs`,
   run against fakes or a stub HTTP server. None of them touch workerd's
   known-length rule. They were green throughout and proved nothing about
   whether an upload can execute. Treat any prior "attachment transport tested"
   as covering wire shape and quota logic only.
2. **Any claim that OSL can send a file.** Anything attachment-shaped on the
   website, in the claim allowlist, or in the support matrix was describing a
   path that could not run. Truth already has this.
3. **Any prior "attachment sent" receipt or QA observation.** If one exists, it
   did not go through `/v1/attachment`. Either it predates cipher-store, or it
   observed something other than what it recorded. Crypto should treat all of
   them as unproven, which I understand they have been told.
4. **A consequence for the audit's own CRITICAL #4.**
   "Shipping non-image attachments are decrypted into durable plaintext files"
   describes the *open* path: a recipient fetches an attachment and OSL writes
   the decrypted bytes to LocalAppData. But a recipient can only fetch what a
   sender uploaded, and `crates/ipc/src/cipher_store_client.rs` is the only
   upload route. If nothing was ever uploadable, then no plaintext was ever
   staged from this path in the wild — the defect is real in source and the fix
   is still required, but its **historical exposure may be nil**.
   I am flagging this as a question for crypto, not answering it, because it
   turns on something outside my lane: whether any attachment ever reached a
   recipient by another route. If the answer is no, CRITICAL #4 changes from
   "plaintext has been hitting disk" to "plaintext would hit disk the moment
   uploads start working" — which is a different remediation urgency and a
   different disclosure posture.

   What I *can* now contribute to that question, from inside my lane: there is
   no second server-side upload route. `/v1/attachment*` is the only one, and
   `cipher_store_client.rs` is its only caller. So "another route" would have to
   mean an attachment delivered without cipher-store at all — a Discord-native
   upload, or the 64 KiB `/v1/blob` lane, which works fine because it buffers
   via `readBoundedBody` and never had this defect. Whether the client ever
   staged plaintext from either of those is the part crypto must answer.
   **Do not treat "structurally nil" as established until they have.** The
   argument is strong and it is still an argument, not a measurement.

**The generalisable lesson,** which is why this belongs in the report and not
just the commit message: three false greens today share one shape — a harness
that cannot fail. A D1 fake that only models statements its author wrote, an R2
double that accepts any stream, a default-deny assertion that passes vacuously.
Each confirmed the author's belief rather than the system's behaviour. The
countermeasure that actually worked here was not a better fake; it was running
the real runtime once.

## Also closed — my own reserve was check-then-act

The recipient-wide reserve I added for the control-inbox fix was itself a
`SELECT COUNT(*)` followed by a decision followed by an `INSERT`. Migration
0027's trigger backstops the hard 512, but it knows nothing about the 384
reserve, so two concurrent posts could have eroded it. That is the same defect
class as HIGH-2, in code I had just written.

The ordinary-lane insert was already an `INSERT ... SELECT ... WHERE EXISTS`, so
the cap went into the same statement as one more predicate — no migration, no new
round trip. A zero-change result now re-reads the count and answers
`recipient_inbox_full` rather than an opaque 500.

Honest limit: the observable behaviour (429 at the cap) is tested, but I did not
build a live interleaving test for the predicate itself, because the request
boundary is not controllable from the harness. Its correctness rests on the same
argument as the cipher-store counter: one statement, serialised by D1.

## The open-registration residual — exact client contract for the crypto lane

Recorded here in full rather than half-closed, because the server cannot fix it alone.

**Where it stands.** `/v1/register` proves the caller possesses the submitted
Ed25519 key. It proves nothing about who owns the *identifier*: `register.ts:110`
validates `user_id` only with `isProtocolId` (bounded, no control characters).
Migration 0029 refused Discord snowflakes and quarantined every legacy row behind
`identity_lookup_enabled = 0`, which closed the vector the audit described. It did
not make identifiers unforgeable. Any non-snowflake identifier is still
first-come, so an attacker who learns Bob's opaque OSL id before Bob registers can
claim it with attacker-controlled keys.

**What already limits the damage.** Once an id is claimed, rotation is
compare-and-swap against the registered key (`register.ts:302`), so the claim
cannot be stolen afterwards. And the hub's friend-code path carries key material
plus a safety number, so a substituted key is detected out of band rather than
silently trusted. The exposure is first contact through keyserver-first discovery.

**The fix, and the tension nobody should walk into.** The obvious move —
`user_id = H(ik_ed25519_pub)` — is wrong, and it is wrong in a way that will not
show up until someone rotates. Rotation deliberately keeps `user_id` and swaps the
signing key (`register.ts:372`, `last_rotated_at`). Deriving the identifier from
the rotatable key means every rotation changes the identity, which breaks every
peer binding. The identifier must be derived from something that never rotates.

OSL already has such a thing: the recovery entropy deterministically rederives the
whole identity (audit medium, `crates/keystore/src/identity.rs:78-85`). So a
non-rotating root already exists in substance; it just is not published.

**Contract crypto would need to implement:**

1. Derive a long-term, non-rotating `ik_root_ed25519` from the existing recovery
   entropy, distinct from the rotatable `ik_ed25519`.
2. Define the identifier as, exactly:
   `user_id = base32-lowercase-nopad( SHA-256("OSL-ID-v1" || ik_root_ed25519_raw)[0..20] )`
   — 160 bits, 32 characters. Domain-separated so the hash cannot be reused from
   another context; 160 bits because this is a public commitment needing
   second-preimage resistance, not merely collision resistance; base32 lowercase
   so it can never be confused with the 17–20 digit snowflakes 0029 refuses, and
   stays short enough to sit inside a friend code.
3. Registration and rotation both carry a signature by `ik_root` over the
   canonical bundle. Rotation changes `ik_ed25519` and leaves `user_id` intact,
   preserving today's semantics.
4. **Server side, mine, one small change once 1–3 ship:** `register.ts` recomputes
   the identifier from the submitted root key and refuses any mismatch. That is
   the actual fix — the server stops accepting a caller-asserted identifier at
   all, so pre-registration becomes impossible without the root key.
5. **Migration.** Existing identifiers are caller-chosen and will not match. Add
   an `identity_scheme` column (0 = legacy caller-chosen, 1 = key-derived). Legacy
   rows stay permanently unable to enable lookup — they are already quarantined by
   0029 — and must re-onboard under a derived id. Do not grandfather: a
   grandfathered row preserves exactly the attack.
6. **Rollout order**, three deploys, never fail-closed against a client that has
   not shipped: server accepts both schemes and records which → client emits
   derived ids → server refuses scheme 0.

**What this still does not give you.** It binds an identifier to a key, not a key
to a person. Anyone can generate a root key and register its derived id. Human
identity binding remains the safety-number ceremony, and nothing here replaces it.

## The two suites do not have equal credibility — do not average them

Stated plainly because both report a green number and those numbers do not mean
the same thing.

**`keyserver-cf` runs under `@cloudflare/vitest-pool-workers`** with real D1 and
`readD1Migrations` applying the actual `migrations/*.sql` (`vitest.config.ts`).
Its CHECK constraints, foreign keys and the `BEFORE INSERT` triggers from 0016
and 0027 are genuinely exercised. When its control-inbox tests say a quota
holds, a real database enforced it.

**`cipher-store-cf` ran plain vitest under Node** against hand-written doubles.
Its green suite was compatible with every attachment upload in production
returning 500, for the entire life of the feature. That is not a hypothetical
about that suite; it is what actually happened.

The gap is being closed in the order the evidence dictates, not the order that
looks tidiest:

1. **Done** (`d5ea447`). The R2 doubles now refuse an unknown-length body with
   workerd's own message and honour `onlyIf`. This had to come first because
   *the pool-workers migration does not close it* — a permissive double plus a
   correct source is a defect with a note on it, not a closed defect, and it
   could be reintroduced tomorrow with the suite staying green.
   `test/harness-strictness.test.ts` proves the guard fires, and includes a
   positive control so it cannot pass by refusing everything.
2. **In progress.** Migrate cipher-store to `vitest-pool-workers`, which closes
   most of the D1 gaps and the Node-versus-workerd runtime-global split — the
   category ranked highest precisely because it has already hidden a
   production-wide failure once.
3. **Recorded, not chased.** Two gaps neither step closes:
   - **KV semantics.** Link-grant single-use replay suppression is a `Map`
     around a security decision, and production KV is eventually consistent and
     non-atomic. Same class as HIGH-2. Bounded for now only because the lane is
     dark behind `LINK_GRANT_ENABLED`; it must be fixed *before* that flag is
     ever flipped, not after.
   - **Native rate-limit binding semantics** on the keyserver, which are
     provider-side and permissive by documentation. Nothing security-critical
     leans on them — control-inbox admission is enforced by D1 predicates and
     triggers — but no test can prove the provider's behaviour.

**Still open after all three:** `D1 batch()` atomicity is not modelled, and
`handleAttachmentComplete` uses `batch()` for exactly the crash-and-retry
transition those tests exist to prove. Real D1 under pool-workers makes an
interrupted-batch test possible for the first time, so it is sequenced after
step 2 rather than written against a shim that cannot express it.

## Vacuous assertions in the keyserver suite — found, NOT yet fixed

The keyserver's 381 tests had never been swept for the pattern that produced six
false greens across the project today: a check that reports success without
having done the thing. Full inventory:
`docs/reports/keyserver-vacuous-assertions-2026-07-26.md`. Six high-severity
entries. The ones I judge worth fixing first, and why:

1. **`test/integration/prekey-bundle.test.ts:464`** — the clearest instance in
   the package. A uniqueness assertion over the extracted prekeys passes
   vacuously on an *empty* extraction, so a total failure to pop one-time
   prekeys leaves the test green. Same shape as the R2 double.
2. **`test/integration/identity-cas.test.ts:25`** — a compare-and-swap test
   containing only a refusal, with no accepted CAS in the same setup. An
   implementation that is *always* a no-op passes it. CAS is what prevents an
   attacker rotating another identity's key, so this one is ranked higher here
   than in the inventory.
3. **`test/integration/prekey-bundle.test.ts:429`** — asserts the stale-request
   refusal but never the no-consume property its own name claims. A stale
   request that silently burns a one-time prekey would pass.
4. **`test/integration/control-inbox.test.ts:113`** and
   **`prekey-bundle.test.ts:175`** — replay-receipt tests that never prove the
   row existed before it was deleted, so "enqueue never happened" and "enqueue
   then correctly consumed" are indistinguishable.

The fix in every case is the same and is cheap: assert something non-empty on
the positive path *first*, so the negative path cannot pass vacuously. None of
these indicate a product defect — they indicate the tests could not have told us
if there were one, which is a different and quieter problem.

**Status: recorded, not fixed.** Deliberately not dispatched — the fleet was at
load 45 on 10 cores with 46 concurrent Codex processes when this landed, and the
concurrency cap had just been reduced. Queued behind the load gate rather than
added to it.

## Correction to this report: the sweep does NOT self-heal above 100 rows

I wrote earlier that an abandoned reservation "self-heals within one sweep cycle
without an operator". That is only true below 100 expired rows, and I should not
have claimed it without testing the sweep at scale.

**D1 accepts 100 bound parameters per query and rejects 101.** Measured against
real D1 in this repo, not recalled: 99 and 100 succeed, 101 and 102 fail with
`D1_ERROR: too many SQL variables`.

`sweepExpiredAttachments` (`src/lib/sweep.ts:93-97`) builds
`DELETE ... WHERE expires_at < ? AND id IN (<one ? per row>)` with up to
`ATTACHMENT_SWEEP_BATCH_SIZE` = 100 ids. That is **101 bound parameters** whenever
a full batch exists. So the attachment sweep works below 100 expired rows and
**fails entirely at 100 or more**, swallowed by the `try/catch` in `index.ts` and
surfacing only as `[attachment-sweep] failed`.

The consequence is the failure mode HIGH-1 was supposed to remove: once 100
attachments expire together nothing is reclaimed, the 512-row / 8 GiB quota fills
permanently, and every later upload gets `503 storage_capacity` — with no
attacker required. And bulk-expiring 15-minute reservations is precisely how a
busy period reaches 100, so my own fix makes the trigger *more* likely, not less.

**Pre-existing**, from the original attachment commit `08552e5`; the placeholder
loop is untouched by tonight's work. The `node:sqlite` shim could never have
caught it, because SQLite itself permits 999 parameters — **only real D1 enforces
100.** This is the second production defect found by moving off doubles, and on
its own it justifies the pool-workers migration.

Fix in progress: chunk the DELETE at 90 ids per statement, preserving the
existing per-id semantics rather than switching to a subquery that could delete
rows whose R2 objects this pass did not handle. Gated on a regression test that
seeds more than 100 expired rows and is observed failing with `too many SQL
variables` before the change.

## Remaining server-side audit items, checked

- **Does the keyserver have the same KV limiter race?** No — verified, not
  assumed. `keyserver-cf/src/lib/rate-limit.ts` already uses Cloudflare's native
  Rate Limiting binding and fails closed on a binding error; its own comment
  records that it replaced an earlier KV read/modify/write counter. Native
  counters are permissive and eventually consistent, but nothing security-critical
  leans on them: control-inbox admission is enforced by D1 predicates and 0027's
  triggers, not by the limiter.
- **Residual of the open-registration critical, after 0029.** 0029 refuses
  Discord snowflakes and quarantines every legacy identity behind
  `identity_lookup_enabled`, which closes the vector the audit described. It does
  **not** bind `user_id` to the submitted key: `register.ts:110` validates only
  `isProtocolId`, so any non-snowflake identifier is still first-come. An attacker
  who learns someone's opaque OSL id before they register can still claim it.
  Mitigating factors: once claimed, rotation is CAS-protected against a different
  key (`register.ts:302`), and the hub's friend-code path carries key material and
  a safety number, so a pre-registration is detected rather than silently trusted.
  The real fix — key-derived identifiers — changes how the client generates an
  identity and so is not a server-only change. Recorded here rather than closed.
- **Generic-blob aggregate quota** is not probed live, deliberately: proving it
  would mean filling 2 GiB of production storage. It is proved against real SQLite
  in `test/rate-limit-atomic.test.ts`.

## Things I noticed and did not act on

- **Migration 0027's header still says `NOT DEPLOYED`** while the dispatch and
  master both indicate it is applied. Not on this work list, and memory records
  another tab as owning that correction, so I left it. It is still a live
  contradiction for anyone reading the migration directory.
- **Migration 0028's link-grant lane is still dark** behind default-off
  `LINK_GRANT_ENABLED`, as instructed. Not touched. Note that the KV race the audit
  declined to escalate is the same class as HIGH-2 and is *not* fixed by this work:
  `lib/link-grant.ts` single-use consumption still goes through KV. If that route
  is ever enabled, it needs the same treatment.
- **`sweepRateCounters` is new work for the five-minute cron.** It is a single
  indexed `DELETE` over a table bounded by distinct-addresses-per-hour. Not a
  concern at current scale; worth watching if traffic grows.

---

## Acceptance rows this earns

Truth lane applies these; I have not edited the checklist.

| Claim | Evidence | Status |
|---|---|---|
| Audit HIGH "empty multipart reservations exhaust attachment capacity" is fixed in source, with a test that exhausts the quota with bodyless sessions and failed before the fix | `cipher-store-cf/test/attachment-session-budget.test.ts`, 4 tests, real SQLite + real migrations; baseline failures quoted above | `test-proven-only` |
| Audit MEDIUM "non-atomic rate limiting" is fixed for all mutation routes, with a concurrent test that beat the old limiter | `cipher-store-cf/test/rate-limit-atomic.test.ts`, 6 tests; 700-way race admits exactly 600 | `test-proven-only` |
| Generic blob storage has a database-level aggregate quota | same file, 2 tests | `test-proven-only` |
| Audit MEDIUM "cross-sender control-inbox eviction" is fixed; a victim's row survives the documented attack and the attacker receives explicit backpressure | `keyserver-cf/test/integration/control-inbox-cross-sender.test.ts`, 3 tests against real D1; baseline reproduced the eviction | `test-proven-only` |
| Public pubkeys lookup no longer publishes rotation timing, and registration timing is coarsened | `keyserver-cf/test/integration/pubkeys.test.ts` | `test-proven-only` |
| Migration 0002's security-relevant documentation contradiction is corrected against source | `cipher-store-cf/migrations/0002_fetch_token.sql` | `verified-source` |
| Ordered deploy sequence and rollback exist for both workers | `cipher-store-cf/DEPLOY.md §7b`, `keyserver-cf/DEPLOY.md §11b` | `test-proven-only` |
| Both workers typecheck clean and exceed their baselines | 87/87 and 381/381 + 3/3, quoted above | `verified-local` |
| Both HIGH cipher-store fixes behave correctly on a real Workers runtime, proved by an executable probe rather than by unit tests | `cipher-store-cf/scripts/post-deploy-probe.mjs`, 3/3 PASS against `wrangler dev --local` with all six migrations applied; Probe C admitted 6+18=24 exactly | `runtime-proven` (local) |
| A pre-existing defect that broke **every** attachment upload on the real runtime was found and fixed | workerd `TypeError: Provided readable stream must have a known length`; call sites proved byte-identical to `git HEAD` before fixing; probes A and B now pass | `runtime-proven` (local) |
| The reserve added by the control-inbox fix is enforced atomically, not check-then-act | admission cap carried inside the conditional `INSERT ... SELECT`; 39/381 green | `test-proven-only` |
| The keyserver does **not** share the cipher-store limiter defect | `keyserver-cf/src/lib/rate-limit.ts` uses the native binding; verified by reading the implementation | `verified-source` |
| Executable post-deploy probes exist for both workers, distinguishing decisive checks from regression guards | `scripts/post-deploy-probe.mjs` in each worker; wired into both DEPLOY.md sections | `runtime-proven` (cipher-store, local) / `test-proven-only` (keyserver) |

Not claimed: nothing here is `verified-live`. No deploy has happened. "Runtime-proven"
above means a real workerd on this machine, not production.

---

## Five plain lines

1. Two real security holes in the OSL servers are now fixed — one let anyone switch off file attachments for everybody for a week using sixteen empty requests, and the other meant our request limits could be quietly exceeded. I also stopped one user from silently deleting another user's undelivered messages, and stopped the public key-lookup page from telling strangers when someone signed up or last changed their keys.
2. Separately, I found that **sending any file attachment has been failing outright** on the real server software. It is an old bug, not something these changes caused, and I fixed it — but it means attachments were broken before today regardless of this work.
3. I wrote a script for each server that actually tests the fixes after they go out, instead of assuming a successful deploy means a working fix. Run against a real local copy of the server, all three checks pass, and one of them landed on the exact expected number, which is how the attachment bug surfaced in the first place.
4. Nothing is live yet — deploying is your call. For the attachments server the database change and the server update must go out together in that order, and note it will apply **two** pending database changes, not one; for the other server it is a single ordinary update with an ordinary undo.
5. One more serious problem — a hostile person in a group chat can delete protected messages for everyone — is written up in full here but has to be fixed in the app code, not the servers, so another lane owns it.
