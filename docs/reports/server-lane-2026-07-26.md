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
SUPERSEDED — these were measured before the pool-workers migration and are kept
only to show the baseline they were compared against. The authoritative,
current figures are in "Final verification state" at the end of this report.
Do not quote the two lines below.

cipher-store-cf   tsc --noEmit: exit 0    vitest (Node + doubles): 10 files / 87   (baseline 8 / 75)
keyserver-cf      tsc --noEmit: exit 0    vitest-pool-workers:     39 files / 381  (baseline 38 / 377)
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

Historical snapshot, superseded by the exact-version listing at the end of this
report: both Workers had been deployed by the owner as **cipher-store
`3374d057`**, **keyserver `3f92f0f5`**. Three different confidence levels applied
at that point and should not be collapsed:

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

## Second correction: the content TTL does NOT start at completion

The audit's suggested fix said "begin the user-selected content TTL only after
successful completion." I listed that as layer 2 of the HIGH-1 fix and wrote a
comment and a test title saying completion is where the TTL starts. **That is not
what the code does, and it cannot be.**

A delegated lifecycle description caught it by disagreeing with the code
(`docs/reports/cipher-store-attachment-lifecycle-2026-07-26.md`). The absolute
expiry instant is computed at *session creation*
(`attachment.ts:263-277`); completion merely copies it (`:451-458`). It has to
work that way: the shipping Rust client rejects a completion receipt whose
`expires_at` differs from the session receipt's, and its response structs are
`deny_unknown_fields`, so the two receipts must carry the same value and it must
be fixed before the first one is sent.

**What is actually delivered, precisely:** what defers is the *reclaim deadline*,
not the content TTL. An incomplete session's `expires_at` holds the short
15-minute hold, sliding on progress, and completion moves it to the promised
instant. The security property the audit wanted — a bodyless session cannot park
capacity for seven days — is fully delivered by the short reclaim deadline. The
literal instruction is not, and could not be, implemented without breaking every
deployed client.

**A consequence I had not stated:** because the instant is fixed at creation, a
slow upload receives *less* ready-state lifetime, not a fresh TTL. A 512 MiB
upload taking twenty minutes gets twenty minutes less content lifetime than it
asked for. That is a real, if minor, product behaviour nobody had written down.

I am flagging my own overstatement because "we implemented the audit's fix" and
"we implemented something that delivers the same security property, and here is
why the literal fix is impossible" are different claims, and only the second one
is true.

Two further contradictions from the same review, both being corrected: the
module header still advertises "bounded streamed parts" when uploads are now
buffered, and `attachment-limits.ts` claims the aggregate quota is enforced by
CHECK constraints and an authoritative D1 trigger when migration 0004 creates
neither — the only enforcement is the Worker's conditional INSERT. That last one
is the same class of security-relevant false comment as the migration 0002 defect
this lane already corrected.

## OPEN, NOT FIXED: the keyserver has the same unbounded-sweep class

Recorded for whoever picks this up, because I ran out of window rather than out
of confidence that it is real.

`keyserver-cf` scheduled cleanup is unbounded in six places. `index.ts:172` calls
`sweepExpiredPrivacyRows`, which issues
`DELETE FROM wrapped_keys WHERE unixepoch(expires_at) <= ? RETURNING content_id`
at `lib/db.ts:283` with no `LIMIT`, no id batching, and `RETURNING` materialising
every deleted row purely to count them (`:305`), plus five further unbounded
receipt deletes at `:287`. Also unbounded: subscription expiry
(`lib/subscriptions.ts:160`), crypto invoice cleanup
(`endpoints/crypto-settlement.ts:452`), Stripe checkout claims
(`lib/stripe-checkout-claims.ts:304`), payment-alert retention
(`lib/payment-alert-outbox.ts:195`), and the direct control-inbox and link-grant
deletes at `index.ts:192` and `:216`. The payment-alert *delivery* drain is
correctly bounded with `LIMIT ?` at `lib/payment-alert-outbox.ts:152`, which
shows the right pattern already exists in the file.

This is the same failure shape as the two sweeps fixed tonight: exceed a limit,
throw, get swallowed by the scheduled handler's `try/catch`, and every subsequent
tick reissues the identical all-or-nothing statement — so it never makes
progress and the affected table is never reclaimed again.

Two caveats I want stated rather than glossed. It is **less** likely to bite than
the cipher-store cases, because these tables have no aggregate cap forcing a
large simultaneous cohort. And `RETURNING` on a large delete is the part most
likely to fail first, inside a 128 MB isolate.

**How this was found is worth more than the finding.** An analysis named it with
the wrong file path. I could not locate it, marked it `unverified` rather than
dismissing it, and had it independently searched for — it was real, one directory
away. A wrong citation is not a wrong finding, and "I could not confirm this"
is the correct verdict rather than "this is not a defect."

## MY REGISTRATION CONTRACT WAS WRONG — superseded, do not implement it

The 6-step contract above is **retained for the record and must not be built**.
An adversarial review of it (`docs/reports/server-lane-contract-review-2026-07-26.md`)
found four CRITICAL flaws. I dispatched that review precisely because I am the
wrong person to review my own spec, and it was the right call.

**The one that matters most: my rollout preserved the vulnerability it existed to
remove.** Step 6 deploy 1 said "server accepts both schemes and records which."
Because the server accepts any bounded non-snowflake string
(`register.ts:110-116`), an attacker can submit *Bob's future derived
identifier* as an ordinary scheme-0 registration during the rollout window. Bob's
later legitimate scheme-1 registration then collides with the squatter's row. I
designed a migration against first-claim squatting whose first phase permitted
first-claim squatting. **The namespace must be syntactically reserved from the
first server deploy**, before any client can emit it.

The other three:

- **The final deploy is unavoidably fail-closed** for a client that never
  upgrades. I claimed no step would be. That is impossible: refusing scheme 0
  must reject a never-upgraded first launch. It needs a stated
  minimum-version/adoption gate and an explicit acceptance of that boundary, not
  a claim it does not exist.
- **Root proof must be ADDITIVE, never sufficient.** My step 3 said root signs
  the bundle. If a root-signed bundle alone authorises rotation, an old
  root-signed bundle replays and rolls an identity *back* to a superseded
  operational key. Existing rotation's stored-previous-key equality, new-key
  possession and SQL CAS (`register.ts:298-370`) must all survive.
- **A forever-online root is an unrevocable credential.** I made it a routine
  co-signer for every rotation, putting the highest-value secret on the ordinary
  online path with no epoch, successor or revocation story. It should authorise
  first enrollment and a separately specified recovery path — nothing routine.

And one factual error of mine: I wrote that legacy rows are "permanently unable
to enable lookup." They are not. Migration 0029 explicitly allows re-registration
to enable lookup, and `enableIdentityLookup` (`lib/db.ts:399-415`) does exactly
that. I asserted a property of a migration without reading its behaviour through.

I also missed that an identifier derivation **already exists** —
`native_user_id` (`crates/keystore/src/identity.rs:194-224`) hashes the
*operational* keys, with a second copy in
`apps/osl-hub/src/password_lifecycle.rs:339-351`. Any implementation must name
every generation path and share one canonical function, or one path keeps
emitting the old rotatable-key identifier.

**Use the reviewer's 7-step replacement**, in the review document, as the
contract of record. It is more precise than mine in the way that matters for two
teams implementing independently: exact prefix, exact alphabet, exact framing,
and mirrored known-answer vectors.

### What I have implemented now, and why only this

Namespace reservation only: migration 0030 adding `identity_scheme` and
`ik_root_ed25519_pub`, and a guard refusing any registration whose `user_id`
matches `^osl1_[a-z2-7]{32}$`, because root-proof verification does not exist
yet. This is the CRITICAL fix and it is safe to ship immediately precisely
because **nothing emits that namespace yet** — reserving it breaks no client and
closes the squatting window before it can open. Everything else in the corrected
contract needs the client change and a separately reviewed protocol.

## Two more platform-contract findings, recorded with mechanism

Full review: `docs/reports/server-platform-limit-review-2026-07-26.md`. These are
the two I had not written down before the window closed. Neither is fixed.

**1. Read-bucket rate limits are not a ceiling during a burst.** `LOW`, and
fail-open by design — the defect is the claim, not the behaviour.
`src/lib/rate-limit.ts:181-194` writes one KV key per admitted read. KV permits
**one write per second to the same key** and rejects the rest with 429
(<https://developers.cloudflare.com/kv/api/write-key-value-pairs/#limits-to-kv-writes-to-the-same-key>).
One address issuing two same-bucket reads inside a second therefore makes `put()`
throw; the catch fails read buckets open on purpose, and those requests are
admitted **without being counted**. The file's own header advertised "fetches:
3600 / hour" as a budget. Corrected in place: read buckets are a steady-state
cost control, not a bound, and only the D1-backed mutation buckets are a real
ceiling. To confirm against real KV: two same-bucket reads from one address
inside one second, and observe both admitted plus the limiter-unavailable line.

**2. `meta.changes` exact 0/1 semantics — RESOLVED, measured not assumed.**
Was recorded here as `UNKNOWN`. Cloudflare documents `meta.changes` only as a
rough indication and specifies no branch values for the two statement shapes
three admission decisions depend on. Now measured against real D1 under
vitest-pool-workers and pinned by `test/d1-meta-changes-contract.test.ts`:

| case | `meta.changes` | `RETURNING` via `.first()` |
|---|---|---|
| `INSERT … SELECT … WHERE` predicate true | 1 | row |
| `INSERT … SELECT … WHERE` predicate false | 0 | null |
| `INSERT … ON CONFLICT DO UPDATE … WHERE` true | 1 | row |
| `INSERT … ON CONFLICT DO UPDATE … WHERE` false | 0 | null |

Every value matches what the three production sites assume — quota admission
(`attachment.ts:200-231`), part reservation (`:336-360`) and the atomic limiter's
admit/deny (`rate-limit.ts:167-175`). **No defect.** The assumption was correct;
it was simply unverified, which is a different thing from wrong and was worth the
twenty minutes to separate. The test asserts real table state alongside each
count, so it cannot pass if the semantics were inverted, and it pins the
behaviour against a future D1 change rather than leaving it implicit.

This is worth stating plainly because it is the same shape as the two defects
already confirmed tonight: correct-looking code resting on a platform behaviour
nobody had read the contract for.

## Keyserver privacy sweep: bounded, but this one is PRECAUTIONARY not proven

`sweepExpiredPrivacyRows` (`keyserver-cf/src/lib/db.ts`) is now incremental —
`LIMIT`-ed selects in batches of 100, chunked deletes respecting the measured
100-bound-parameter D1 ceiling, and a per-invocation cap so one cron tick cannot
run unbounded. Previously it issued
`DELETE FROM wrapped_keys WHERE unixepoch(expires_at) <= ? RETURNING content_id`
with no limit, plus five further unbounded deletes, and counted via `RETURNING`,
which SQLite buffers in full before emitting inside a 128 MB isolate.

**The honest status, and it differs from the other two sweeps fixed tonight.**
The new test — "drains a multi-batch expired privacy backlog without deleting
live rows", with a positive control asserting live rows survive — **passes
against the unbounded code as well.** I checked by stashing the fix. It guards
the batching behaviour; it does **not** reproduce the original risk, because
miniflare does not enforce a 128 MB isolate or D1 transaction size at test
scale.

So this is a **defensive bound taken on the platform's documented limits**, not a
demonstrated defect. That is a weaker claim than the attachment sweep, where the
old code failed in front of me with `too many SQL variables`, and the two should
not be reported as the same kind of thing. What would settle it is a real-D1 run
at a row count near the documented ceiling, which is not reachable from this
harness.

Scope was deliberately narrow: `lib/db.ts` only. The other unbounded scheduled
cleanups — `subscriptions.ts:160`, `endpoints/crypto-settlement.ts:452`,
`lib/stripe-checkout-claims.ts:304`, `lib/payment-alert-outbox.ts:195`,
`index.ts:192` and `:216` — were left alone and remain open. Bounding six paths
through billing code at the end of a window is how you break something you cannot
verify. `lib/payment-alert-outbox.ts:152` already shows the bounded pattern.

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

## Single-use wrapped-key reads: the other half of the property is now tested

`wrapped-keys.test.ts` "concurrent valid reads return a single-use row at most
once" counted response **statuses only**. An implementation returning one `200`
with an empty or wrong body plus three `404`s passed it — proving "at most once"
while proving nothing about the read having delivered the share. Single-use
wrapped-key reads are the mechanism preventing a key share being fetched twice,
so half the property was untested.

Now additionally asserted, without touching any existing assertion:
`content_id`, `recipient_id` and `wrapped_share_blob` of the winning response
must equal the seeded values *read from the fixture rather than hardcoded*, so
the assertion cannot drift; the losers must be refused for the **right** reason
rather than merely being non-200, so a 500 cannot masquerade as single-use
enforcement; and the row's post-state is queried directly in D1 rather than
inferred from the responses.

Concretely now caught, and previously green: one `200` whose
`wrapped_share_blob` is empty or wrong, a loser returning an unrelated `404`, and
a path that reports the correct statuses but leaves the row in `wrapped_keys`.
Title unchanged, `expect(` 61 → 67, additive only.

This was the last entry on the vacuous-assertion inventory rated worth fixing.

## The link-grant replay race: real, but currently unreachable — verified

I have asserted "bounded because the lane is dark" several times tonight without
re-checking it. Checked now, and it holds on **three independent gates**, not one:

| gate | mechanism |
|---|---|
| cipher-store secret | `LINK_GRANT_PUBKEY_B64` unset ⇒ creation refused 503 *before any D1 access* (`cipher-store-cf/src/lib/link-grant.ts:73`) |
| cipher-store route | the `[[routes]]` block is commented out in `cipher-store-cf/wrangler.toml` |
| keyserver flag | `env.LINK_GRANT_ENABLED !== "true"` ⇒ issuance refused (`keyserver-cf/src/index.ts:321`) |

**The defect itself is real.** `link-grant.ts:164-172` enforces single use by
reading a KV key and then writing it. KV is eventually consistent and permits one
write per second per key, so two concurrent presentations of the *same* grant
both observe null and both mint a link. It is the same shape as the audit's
non-atomic rate limiter, except the protected thing is a single-use credential,
so the consequence is grant replay rather than over-admission.

**Severity, stated precisely:** unreachable in the deployed configuration, and
therefore not a live defect today. The fix in flight — claiming the `jti` by
INSERT success against a primary key, with no read first — is **preparatory
hardening**, not a production repair. That is a weaker claim than the attachment
defects and should not be reported alongside them.

What makes it worth doing now rather than later: all three gates are single
edits. Whoever opens one is unlikely to be the person who knows the consumption
path is racy, and the fix costs nothing while the lane is dark.

## Link-grant single-use is now atomic, and the race was demonstrated first

Consumption claimed by INSERT success against a primary key — no prior read:

```sql
INSERT INTO link_grant_consumed (jti, expires_at) VALUES (?, ?)
ON CONFLICT(jti) DO NOTHING RETURNING jti
```

A null result means the `jti` was already claimed, so the grant is refused. New
migration `0007`, a bounded sweep following the shape used for the other two, and
a database error returns **503 `grant_store_unavailable`** rather than admitting
— because without replay suppression one captured grant becomes an unbounded
creation capability, which `wrangler.toml` names as a release invariant.

**Proven, not asserted.** With the old KV check-then-put temporarily restored,
the new concurrency test fails `expected 2 to be 1` — both presentations of the
*same* grant admitted. The atomic implementation was then restored and the test
passes. That is the race demonstrated rather than argued.

Severity is unchanged from the entry above: the lane is dark behind three
independent gates, so this is preparatory hardening, not a production repair.

## Acceptance rows this earns

Truth judges and applies these; I have not touched the checklist. An event was
submitted to scope `d2` only, because that is the one row I can map my work to
with confidence — inventing a scope would put points somewhere they do not
belong.

**D2 · Encrypted attachment transport.** Three defects closed on this row.
Attachment upload was failing for every user on every attempt for the entire
committed life of the feature (the body was piped through a `TransformStream`;
R2 requires a known length). Expired attachment storage stopped being reclaimed
entirely once ~100 attachments expired together, which would have filled the
shared quota permanently with no attacker involved. And a caller could reserve
the whole shared attachment space with a handful of empty requests and hold it
for a week. Status: `runtime-proven` against a real local workerd by this lane;
the production confirmation for the upload fix came from the owner, not from
here, and I have not independently probed production.

**Not claimed, and why.** The two HIGH audit findings, the control-inbox
cross-sender fix, the pubkeys minimisation and the identifier-namespace
reservation do not map to a checklist row I can identify. They are described in
full above with file:line; if truth can place them, the evidence is there, but I
am not asserting a row for them.

**Explicitly not earned.** Nothing here is `verified-live` by this lane. Every
number quoted in this report is from vitest-pool-workers against real D1 —
gate stated beside the count, per the lesson that `741 passed` means nothing
alone. The keyserver privacy-sweep bound is precautionary, not a proven-defect
fix: its test passes against the unbounded code too, and I say so above rather
than reporting it alongside the sweeps that were demonstrably broken.

### Final verification state

```
cipher-store-cf   vitest-pool-workers, real D1/R2/KV   12 files / 100 tests   tsc clean
keyserver-cf      vitest-pool-workers, real D1         40 files / 389 tests   tsc clean
```

### Still open, all with mechanism and file:line above

1. Six unbounded scheduled cleanups in `keyserver-cf` outside `lib/db.ts`
   (billing/commerce paths), deliberately not touched.
2. The link-grant KV single-use replay race — **must be fixed before
   `LINK_GRANT_ENABLED` is ever flipped**, not after.
3. Read-bucket rate limits are not a ceiling during a burst (KV allows one write
   per second per key); the header claiming otherwise is corrected, the
   behaviour is unchanged and fail-open by design.
4. `meta.changes` exact 0/1 semantics are an undocumented dependency under three
   admission decisions — recorded as **unknown**, being measured now.
5. Migration `0028` is committed but its link-grant worker code is not, so HEAD
   has the schema without the logic. Harmless only while the lane is dark.
6. Local branch is ahead of `origin` and unpushed; it carries other lanes' work,
   so pushing is not mine to do.

---

## Continuation at HEAD 5822a8f — local D1 contract and control-inbox sweep

This section supersedes stale items 1, 2, and 4 immediately above. No live
infrastructure was read or changed during this continuation.

### `meta.changes` 0/1 semantics: resolved

Status: `test-proven-only`.

The existing real-D1 contract test was rerun under
`@cloudflare/vitest-pool-workers`, using Wrangler 4.114.0 and Node 24.14.0:

```text
✓ A: reports one change when INSERT SELECT admits a row
✓ B: reports zero changes when INSERT SELECT predicate rejects a row
✓ C: reports one change when conflict update predicate applies
✓ D: reports zero changes when conflict update predicate skips
Test Files  1 passed (1)
Tests       4 passed (4)
```

The positive and negative paths cannot pass from metadata alone. The test
asserts `meta.changes` and then independently reads table state for each admitted
and rejected statement (`cipher-store-cf/test/d1-meta-changes-contract.test.ts:48`,
`:70`, `:92`, `:127`). It also exercises the corresponding `RETURNING` shape and
asserts the total row count (`:58-67`, `:80-89`, `:112-124`, `:147-159`).

Observed local-D1 contract:

| statement branch | `meta.changes` | persisted state |
|---|---:|---|
| `INSERT ... SELECT ... WHERE` true | 1 | inserted row exists |
| `INSERT ... SELECT ... WHERE` false | 0 | rejected row absent |
| conflict update predicate true | 1 | seeded row changed |
| conflict update predicate false | 0 | seeded row unchanged |

This resolves the earlier `unknown`; it does not establish `verified-live`.

### Highest-risk remaining scheduled cleanup: control inbox is bounded

Status: `test-proven-only`.

The control inbox was selected ahead of commerce cleanup because it is a live,
publicly fillable privacy path whose per-recipient quotas do not bound global
cardinality. The dark link-grant lane was deliberately not used to justify a
production-repair claim.

The failing-first real-D1 test seeded and positively counted 101 expired inbox
rows, 101 expired replay receipts, one live inbox row, and one live replay
receipt (`keyserver-cf/test/integration/control-inbox-sweep.test.ts:12-68`,
`:111-123`). Against the unbounded implementation, the first cron invocation
deleted all expired rows:

```text
[cron] control_inbox sweep deleted 101 expired row(s)
FAIL expected expiredInbox=1, expiredReceipts=1
     received expiredInbox=0, expiredReceipts=0
Test Files  1 failed (1)
Tests       1 failed (1)
```

That is a non-vacuous proof that the scheduled operation had no per-tick ceiling.
It does not prove that 101 rows exhaust a production isolate; that failure mode
remains `unknown`.

The wired implementation now deletes at most 100 oldest expired rows from each
table per invocation, using bounded primary-key subqueries and no `RETURNING`
materialisation (`keyserver-cf/src/lib/control-inbox-sweep.ts:1-46`). The
scheduled handler calls the bounded helper and reports both counts
(`keyserver-cf/src/index.ts:187-202`).

The same real-D1 test now observes exactly 100+100 deleted on tick one, 1+1 on
tick two, and both live controls present after each tick
(`keyserver-cf/test/integration/control-inbox-sweep.test.ts:125-139`):

```text
[cron] control_inbox sweep deleted 100 row(s) and 100 request receipt(s)
[cron] control_inbox sweep deleted 1 row(s) and 1 request receipt(s)
Test Files  1 passed (1)
Tests       1 passed (1)
```

Focused worker/type gates:

```text
keyserver-cf vitest control-inbox + cron set  Test Files 6 passed (6)
                                                Tests 35 passed (35)
keyserver-cf npm run typecheck                 exit 0
cipher-store meta contract                    Test Files 1 passed (1)
                                                Tests 4 passed (4)
```

The still-unbounded scheduled functions are subscription expiry
(`keyserver-cf/src/lib/subscriptions.ts:160`), anonymous crypto cleanup
(`keyserver-cf/src/endpoints/crypto-settlement.ts:452`), Stripe checkout claims
(`keyserver-cf/src/lib/stripe-checkout-claims.ts:304`), and delivered-payment
alert retention (`keyserver-cf/src/lib/payment-alert-outbox.ts:195`). Whether any
currently reaches a platform failure is `unknown`.

Link-grant atomic consumption at HEAD 5822a8f remains
`implemented-unwired`: it is preparatory hardening behind the existing dark
gates, never a production repair. Nothing in this continuation changes that
classification.

## Acceptance rows this earns

No checklist row is claimed. The `meta.changes` result closes an evidence gap,
and the control-inbox change bounds server housekeeping, but this lane has no
verified mapping from either result to an acceptance row. Nothing is claimed as
`verified-live` or `runtime-proven`.

---

## Audit follow-up — cross-boundary gates

No live infrastructure was read or changed. Status for every result in this
section is `test-proven-only`, except the caller boundaries explicitly marked
`implemented-unwired`.

### Independent reproduction before changing tests

The store audit's three negative controls were rerun locally rather than accepted
on report:

1. Removing the `LINK_GRANT_ENABLED` branch from the full Worker route left the
   old deployment-gate tests green: 3 passed, 16 skipped. The missing issuer key
   produced another 503, so the test did not establish which guard fired
   (`keyserver-cf/src/index.ts:304-320`,
   `keyserver-cf/test/integration/link-grant.test.ts:100-110`).
2. Changing only the keyserver request domain to
   `discord-privacy-client/link-grant/NEGATIVE-CONTROL` left 49/49 keyserver
   tests green. The request signer and verifier both used
   `canonicalLinkGrantBytes`, so they agreed with each other while disagreeing
   with the shipping Rust client (`keyserver-cf/src/lib/canonical.ts:370-405`,
   `crates/ipc/src/cipher_store_client.rs:879-900`).
3. Changing only the keyserver issuer domain to
   `OSL-LINK-GRANT-NEGATIVE-CONTROL` left 32/32 issuer-side tests green. None
   passed the minted authorization to the cipher-store verifier
   (`keyserver-cf/src/lib/link-grant-issuer.ts:59-61`,
   `cipher-store-cf/src/lib/link-grant.ts:33-66`).
4. Adding a redundant `UPDATE users SET rn_capabilities = rn_capabilities` on
   equal-bitmap replay left the old “write-free” test green: 1 passed, 13
   skipped. It asserted response and final bitmap only
   (`keyserver-cf/test/integration/rn-capability.test.ts:228-275`).

These runs prove gaps in the tests. They do not prove current wire drift: the
unmodified request domains and grant domains agree.

### Gates added, with failing mutations

The route test now asserts the exact default-off response and drives the full
Worker with a valid issuer environment to prove the explicit enabled hand-off
(`keyserver-cf/test/integration/link-grant.test.ts:100-124`). Removing the
feature flag now fails:

```text
Expected  {"error":"link_grant_not_enabled"}
Received  {"error":"link grant issuance is not enabled on this deployment"}
Test Files  1 failed (1)
Tests       1 failed | 3 passed | 16 skipped
```

A disposable Node-side cross-repo test read the dirty working tree's Rust domain
constant and compared the keyserver's actual canonical bytes with the Rust
contract vector. Changing only the keyserver domain failed at the first
length-prefixed field:

```text
expected Uint8Array ... LP length 50 ...
to deeply equal Uint8Array ... LP length 36 ...
Test Files  1 failed (1)
Tests       1 failed (1)
```

That gate is `blocked`, not committed. Clean HEAD `e83a487` does not contain
`LINK_GRANT_DOMAIN` or `request_link_grant`; both exist only in another lane's
uncommitted `crates/ipc/src/cipher_store_client.rs` work, outside this lane's
write scope. Applying the staged server diff to a clean worktree made the test
fail with `Rust LINK_GRANT_DOMAIN constant is missing`. Committing a gate against
an absent dependency would make clean CI red rather than protect a shipped
boundary. Crypto must land the Rust side first; then the disposable comparison
should become a standing Node gate.

The cipher-store test now imports the real keyserver issuer, mints a real
authorization, and passes it to the real cipher-store verifier backed by local
D1 (`cipher-store-cf/test/link-grant.test.ts:12-16`, `:78-92`). Changing only
the issuer domain now fails:

```text
Expected  {"ok":true}
Received  {"ok":false,"status":401,"code":"grant_signature",
           "message":"grant signature did not verify"}
Test Files  1 failed (1)
Tests       1 failed | 12 skipped
```

The RN replay test installs a real-D1 `BEFORE UPDATE` guard for its exact
identity before replaying the signed body
(`keyserver-cf/test/integration/rn-capability.test.ts:243-274`). A redundant
`users` update now fails:

```text
expected 500 to be 200
Test Files  1 failed (1)
Tests       1 failed | 13 skipped
```

Unmutated focused gates:

```text
keyserver Worker link-grant + issuer       2 files / 33 tests passed
keyserver RN write-free replay             1 test passed / 13 skipped
keyserver tsc --noEmit                     exit 0
cipher-store link-grant + link lane        2 files / 34 tests passed
cipher-store tsc --noEmit                  exit 0
```

### Boundary reported to crypto

Status: `implemented-unwired`.

Production registration still calls legacy `reg_msg` and builds a
`RegisterRequest` with no `rn_capabilities`
(`crates/keystore/src/client.rs:599-625`). The keyserver can store and serve the
bitmap, but the shipping registration path does not advertise it. No keystore
file was changed here.

`request_link_grant` exists only in the dirty working copy at
`crates/ipc/src/cipher_store_client.rs:903-975`; repository search finds only
its definition, with no production caller, and clean HEAD `e83a487` does not
contain it. Status is `implemented-unwired` in the working tree and `blocked` at
the commit boundary. The keyserver and cipher-store work therefore remains
preparatory, never a production repair. The default-off keyserver flag, missing
issuer secret, and missing cipher-store public key still keep the lane dark.

## Acceptance rows this earns

No checklist row is claimed. These changes turn four previously vacuous or
same-author assertions into mutation-sensitive cross-boundary gates. They do not
make RN advertisement or view-once link creation reachable, and nothing is
claimed as `verified-live` or `runtime-proven`.

---

## B5 point-qualification audit — committed HEAD `0b8f47c`

This audit made no production mutation, did not deploy, and did not inspect or
change link-grant work. All repository references below are from committed HEAD
`0b8f47c618c5b6d85c41cbcba4959046a6f8a554`, not the shared dirty worktree.

### Production reachability map

| Surface | Registered/calling edge | Client-to-Worker edge | Classification |
|---|---|---|---|
| Registration | The legacy Tauri `register` command delegates to `cmd_register` and is registered in `generate_handler!` (`src-tauri/src/main.rs:99-107`, `:3098-3104`). More importantly, production bootstrap/unlock calls the shared automatic path in both shells (`src-tauri/src/bootstrap.rs:1282-1297`, `apps/osl-hub/src/core_bridge.rs:301-310`, `apps/osl-hub/src/password_lifecycle.rs:382-385`). | `ensure_keyserver_registered` calls `KeyServerClient::register` (`crates/ipc/src/commands.rs:7846-8133`, especially `:8059`); that client sends `POST /v1/register` (`crates/keystore/src/client.rs:716-727`); the Worker dispatches it to `handleRegister` (`keyserver-cf/src/index.ts:297-300`). | **live-called**, with the call chain `test-proven-only`. An actual successful registration was not probed because that mutates identity state, so runtime success during this audit is `unknown`. |
| Prekeys | No registered Tauri command, IPC command, broker call, bootstrap hook, or non-test caller reaches either prekey operation. Repository search finds production definitions only. | Signed fetch and replenish clients exist (`crates/keystore/src/client.rs:771-796`, `:847-931`), and the Worker dispatches GET and replenish POST (`keyserver-cf/src/index.ts:274-277`, `:301-303`). | Client methods and server handlers are **implemented-unwired**; the production orchestration boundary is **absent**. |
| Wrapped keys | No registered Tauri command, IPC command, broker call, send path, receive path, or burn path calls `post_wrapped_key`, `fetch_wrapped_key`, or `KeyServerClient::burn`. Their non-test caller count is zero. The shipping burn command only wipes the local store (`crates/ipc/src/commands.rs:6508-6583`). | Authenticated fetch and post clients exist (`crates/keystore/src/client.rs:798-845`), as does the delete client, and the Worker dispatches GET/POST/DELETE (`keyserver-cf/src/index.ts:270-273`, `:300`, `:363`). | Client methods and server handlers are **implemented-unwired**; the production orchestration boundary is **absent**. |
| Control inbox | Legacy Tauri post/drain commands are registered (`src-tauri/src/main.rs:1360-1407`, `:3171-3172`) and invoked by injected production code (`src-tauri/src/injection/boot.js:1901`, `:17478`). The Hub broker also posts directly (`apps/osl-hub/src/broker.rs:2460`) and its text and attachment receive paths drain directly (`:2665-2668`, `:3614-3617`). | The called client uses unfiltered POST/GET/DELETE (`crates/keystore/src/client.rs:1016-1126`, `:1129-1154`, `:1227-1245`), and all three Worker routes are dispatched (`keyserver-cf/src/index.ts:278-279`, `:299`, `:366-367`). | Ordinary control inbox is **live-called**. The signed per-sender client is **implemented-unwired**: `get_control_inbox_from` exists (`crates/keystore/src/client.rs:1156-1225`) but has no caller, while both production Hub drains still call the unfiltered method. |

The words “live-called” above describe a production call edge, not a runtime
success claim. No desktop identity was opened or used during this audit.

### Read-only production probes

Only GETs that cannot insert, update, delete, consume an OPK, consume a wrapped
key, or authenticate as an identity were sent:

```text
GET https://keyserver.oslprivacy.com/v1/healthz
200 {"ok":true}

GET /v1/prekey-bundle/osl_b5_probe_missing
401 {"error":"fresh signed requester authorization required"}

GET /v1/wrapped-keys/osl_b5_probe_missing
401 {"error":"fresh signed recipient authorization required"}

GET /v1/control-inbox/osl_b5_probe_missing?ts=<current>&sig=AAAA&sender=
400 {"error":"sender must be a bounded identifier when present"}
```

These are `verified-live` only for health, route dispatch, and the deployed
empty-`sender` validation branch. They do not prove a successful authenticated
prekey fetch, wrapped-key fetch, or control-inbox drain. Those results remain
`unknown`. Registration was deliberately not probed.

### Clean-HEAD gates

A detached worktree at `0b8f47c` produced:

```text
keyserver-cf control-inbox-sender-filter
Test Files  1 passed (1)
Tests       7 passed (7)

keyserver-cf npm run typecheck
exit 0

osl-cargo test -p ipc --test register_after_unlock -- --nocapture
test result: ok. 5 passed; 0 failed

osl-cargo test -p keystore --test client_test \
  prekey_fetch_carries_registered_identity_signature -- --exact --nocapture
test result: ok. 1 passed; 0 failed; 25 filtered out

osl-cargo test -p keystore --test client_test \
  wrapped_key_fetch_binds_recipient_and_content_id -- --exact --nocapture
test result: ok. 1 passed; 0 failed; 25 filtered out
```

The client tests establish correct request construction only; because their
methods have no production caller, they remain `test-proven-only` and
`implemented-unwired`.

The current production-wiring negative control is non-vacuous:

```text
git show HEAD:apps/osl-hub/src/broker.rs |
  grep -cF '.get_control_inbox_from('
0

git show HEAD:apps/osl-hub/src/broker.rs |
  grep -cF '.get_control_inbox('
3

B5_FILTER_WIRING=FAIL (no production broker caller)
```

Two unfiltered calls are production text/attachment drains; the third is an
ignored read-only live test (`apps/osl-hub/src/broker.rs:9274`).

### Smallest honest next B5 boundary

The smallest boundary that can support the next B5 point is:

> Both conversation-bound Hub receive paths call
> `KeyServerClient::get_control_inbox_from(&identity,
> &manual.peer_osl_user_id)` rather than `get_control_inbox`.

The exact owner is **Tab 5 / Discord broker receive lane**, which owns
`apps/osl-hub/src/broker.rs` and
`apps/osl-hub/tests/native_discord_receive_e2e.rs`. The keyserver lane has no
code fix to make: the Worker filter is dispatched, its real-D1 starvation suite
passes, and its validation branch is `verified-live`. The keystore method also
already exists. Changing either owned Worker directory would not connect the
production caller.

The failing-first behavioral test is already present as a defect
characterization:
`foreign_sender_rows_head_of_line_block_the_drain_at_the_page_boundary`
(`apps/osl-hub/tests/native_discord_receive_e2e.rs:1146-1254`). It creates 64
older rows from two other legitimate senders, then one valid active-peer row,
and currently asserts that the active row is invisible. The owner must invert
it to require immediate delivery without deleting a blocker:

```text
cd apps/osl-hub
osl-cargo test --features core --test native_discord_receive_e2e \
  foreign_sender_rows_head_of_line_block_the_drain_at_the_page_boundary \
  -- --exact --nocapture
```

The repaired test must observe one opened active-peer message while all 64
foreign rows remain pending, and a request recorder must observe
`sender=<active peer OSL id>`. Removing the filtered call or reverting either
drain to the unfiltered method must fail. A companion assertion should exercise
the attachment drain, because it has the same unfiltered call at
`apps/osl-hub/src/broker.rs:3616`.

No external mutation is required for that `test-proven-only` boundary. A later
two-identity production qualification still requires real identities and state
and is therefore `blocked` on its owner-approved rig; this audit did not perform
it.

## Acceptance rows this earns

No checklist edit and no point are claimed. B5 remains 1/4. This audit narrows
the next honest point to one already-deployed server capability and one missing
broker call boundary; it does not round route existence or local tests up to an
end-to-end production contract.

---

## Read-only production Worker version truth — 2026-07-26 22:17 PDT

Probe window: `2026-07-26T22:14:05-07:00` through
`2026-07-26T22:17:09-07:00`
(`2026-07-27T05:14:05Z` through `2026-07-27T05:17:09Z`).
Repository HEAD observed after the probes:
`245564999e57c388459c71f91a9779a89a05a78d`.
Wrangler was `4.110.0` on Node 24.

This run made only Cloudflare listing/detail API reads and public GETs. It did
not deploy, migrate, register, POST, PUT, DELETE, consume a grant, upload
content, query or mutate D1/R2, or read secret values.

### Exact active deployments

Wrangler returns these arrays oldest-first; `.[-1]`, not `.[0]`, is the current
entry. The exact commands and selected output were:

```text
$ cd keyserver-cf
$ npx wrangler deployments list --json |
    jq '.[-1] | {id, created_on, versions}'
{
  "id": "8fa82d0b-9593-46dd-8e01-bdeb9bf79342",
  "created_on": "2026-07-27T00:34:24.935251Z",
  "versions": [
    {
      "version_id": "3f92f0f5-c6ac-4426-9a83-1555f5c6394b",
      "percentage": 100
    }
  ]
}

$ npx wrangler versions view \
    3f92f0f5-c6ac-4426-9a83-1555f5c6394b --json |
    jq '{id, number, created_on: .metadata.created_on,
         source: .metadata.source,
         triggered_by: .annotations["workers/triggered_by"],
         etag: .resources.script.etag,
         handlers: .resources.script.handlers,
         last_deployed_from: .resources.script.last_deployed_from,
         runtime: .resources.script_runtime}'
{
  "id": "3f92f0f5-c6ac-4426-9a83-1555f5c6394b",
  "number": 169,
  "created_on": "2026-07-27T00:34:24.489404Z",
  "source": "wrangler",
  "triggered_by": "version_upload",
  "etag": "4a9b3f0555fa352995a70f5a5b6629dc3978c2a12e37376f492a4030619409d8",
  "handlers": ["fetch", "scheduled"],
  "last_deployed_from": "wrangler",
  "runtime": {
    "compatibility_date": "2026-07-15",
    "compatibility_flags": ["nodejs_compat"],
    "usage_model": "standard"
  }
}

$ cd ../cipher-store-cf
$ npx wrangler deployments list --json |
    jq '.[-1] | {id, created_on, versions}'
{
  "id": "785314a9-f273-4eb5-8fcf-bbbcd0d345a7",
  "created_on": "2026-07-27T01:13:19.214428Z",
  "versions": [
    {
      "version_id": "0a17547d-577e-4f70-8159-f5d90e9c9e31",
      "percentage": 100
    }
  ]
}

$ npx wrangler versions view \
    0a17547d-577e-4f70-8159-f5d90e9c9e31 --json |
    jq '{id, number, created_on: .metadata.created_on,
         source: .metadata.source,
         triggered_by: .annotations["workers/triggered_by"],
         etag: .resources.script.etag,
         handlers: .resources.script.handlers,
         last_deployed_from: .resources.script.last_deployed_from,
         runtime: .resources.script_runtime}'
{
  "id": "0a17547d-577e-4f70-8159-f5d90e9c9e31",
  "number": 15,
  "created_on": "2026-07-27T01:13:18.684701Z",
  "source": "wrangler",
  "triggered_by": "version_upload",
  "etag": "48c427bdd725a12e4257e0877417acc929708143fdc4680c129e392064fc3023",
  "handlers": ["fetch", "scheduled"],
  "last_deployed_from": "wrangler",
  "runtime": {
    "compatibility_date": "2026-05-13",
    "usage_model": "standard"
  }
}
```

| Claim | Tier | Evidence |
|---|---|---|
| Keyserver's active Worker is version 169, exact UUID `3f92f0f5-c6ac-4426-9a83-1555f5c6394b`, at 100% | `verified-live` | Current deployment and version-detail output above |
| Cipher-store's active Worker is version 15, exact UUID `0a17547d-577e-4f70-8159-f5d90e9c9e31`, at 100% | `verified-live` | Current deployment and version-detail output above |
| Coordinator-state's `3f92f0f5` and `0a17547d` are Worker-version abbreviations | `verified-live` | Each is the exact prefix of the active Cloudflare UUID |
| Keyserver deployed runtime configuration matches `keyserver-cf/wrangler.toml:3-4` | `verified-live` | Cloudflare reports `2026-07-15` + `nodejs_compat`; local config has the same values |
| Cipher-store deployed runtime configuration matches `cipher-store-cf/wrangler.toml:3` | `verified-live` | Cloudflare reports `2026-05-13`; local config has the same value |

### Harmless runtime probes

These are the exact requests and complete status/body output:

```text
$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    https://keyserver.oslprivacy.com/v1/healthz
{"ok":true}
HTTP 200

$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    https://keyserver.oslprivacy.com/v1/pubkeys/12345678901234567
{"error":"Discord identifiers are not OSL identities"}
HTTP 400

$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    'https://keyserver.oslprivacy.com/v1/control-inbox/osl_live_truth_missing?ts=1785129429522&sig=AAAA&sender='
{"error":"sender must be a bounded identifier when present"}
HTTP 400

$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    https://ciphers.oslprivacy.com/v1/healthz
{"ok":true,"ts":1785129429}
HTTP 200

$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    https://ciphers.oslprivacy.com/robots.txt
User-agent: *
Disallow: /

HTTP 200

$ curl --silent --show-error --max-time 20 --write-out \
    "\nHTTP %{http_code}" \
    https://ciphers.oslprivacy.com/v1/attachment/not-hex
{"error":"not_found","message":"no such route or blob"}
HTTP 404
```

| Claim | Tier | Evidence and bound |
|---|---|---|
| Both production hostnames currently dispatch their health routes | `verified-live` | Both health GETs returned 200 |
| The active keyserver refuses Discord snowflakes | `verified-live` | The public pubkeys refusal returned the exact branch at `keyserver-cf/src/endpoints/pubkeys.ts:37-40` |
| The active keyserver treats a present-but-empty control-inbox sender filter as invalid rather than silently dropping the filter | `verified-live` | The 400 matches `keyserver-cf/src/endpoints/control-inbox.ts:792-795`; this validation occurs before the D1 lookup at line 798 |
| The active cipher-store serves the deny-all robots body represented locally | `verified-live` | Live bytes match `cipher-store-cf/src/lib/landing.ts:531-539` |
| The active cipher-store falls through a malformed attachment path without reaching attachment fetch/rate-limit state | `verified-live` | `/v1/attachment/not-hex` does not match the hex route and returned the `cipher-store-cf/src/index.ts:240` not-found response |

No authenticated success path was exercised. These probes do not establish that
an inbox filter affects returned D1 rows, that pubkeys omit lifecycle fields for
a real identity, or that attachment quota and R2 writes work.

### Mapping the Worker versions to repository source

The exact Git commit for **both** Worker versions is `unknown`.

Cloudflare's version records provide a Worker UUID, upload time, script etag,
Wrangler source marker, handlers, and runtime settings. They provide no Git
commit/tag annotation. A Cloudflare script etag is not a Git object ID, so it
cannot be compared to the current keyserver tree
`3551ac56a761d2d971b04db2b4470d7ff9f33651` or cipher-store tree
`3bb26488890396348e9c36bb066435c37dfbdc53`.

For the keyserver there is positive evidence that a commit-time mapping would
be false precision:

```text
$ git log --all -S'Discord identifiers are not OSL identities' \
    --format='%H %cI %s' -- keyserver-cf/src/endpoints/pubkeys.ts
b6f456e032b3b40b8046344190520eef6933dfea 2026-07-26T18:00:51-07:00 server lane: fix both HIGH Cloudflare audit findings + a runtime-broken upload path
```

The Worker version was uploaded at `17:34:24-07:00`, 26 minutes before that
source branch first entered a commit, yet the live Worker returns its exact
refusal. This maps that specific deployed behavior to source now present at
`keyserver-cf/src/endpoints/pubkeys.ts:37-40`; it does **not** map the whole
bundle to `b6f456e`. The deploy could have used uncommitted bytes, and the
version metadata cannot distinguish those bytes from any later committed tree.

Cipher-store version `0a17547d...` was uploaded at `18:13:18-07:00`, after
`b6f456e` (`18:00:51-07:00`) and before the next cipher-store commits
(`f8161ac` at `18:27:46-07:00`, `d5ea447` at `18:29:39-07:00`). Timing alone
does not establish that its bundle equals `b6f456e`; an uncommitted working
tree could have been uploaded. The health and robots probes map only those
route bytes to current local source, not the entire bundle.

### Coordinator-state reconciliation

| Coordinator claim | Reconciled tier | Result |
|---|---|---|
| Keyserver Worker `3f92f0f5` | `verified-live` | Correct short form; expanded above to the exact UUID and version 169 |
| Cipher-store Worker `0a17547d` | `verified-live` | Correct short form; expanded above to the exact UUID and version 15 |
| Earlier report header calling cipher-store `3374d057` current | `verified-live` | Superseded. Live listing says `0a17547d...`; the earlier paragraph is now explicitly marked historical |
| Keyserver per-sender filter is in the active Worker | `verified-live` | Empty-filter negative branch is live. Actual filtered D1 selection remains `unknown` because this run had no authenticated identity and did not query state |
| Keyserver `429 recipient_inbox_full` behavior is in the active Worker | `unknown` | A decisive probe requires state-mutating inbox posts and was forbidden |
| Keyserver pubkeys minimisation is in the active Worker | `unknown` | Snowflake refusal is live, but no permitted probe can inspect a real identity response without choosing an identity |
| Cipher-store R2 known-length fix is in `0a17547d` | `verified-live` | The earlier recorded production probe at this report's lines 641-644 returned `201 {"part_number":1,"size_bytes":1024}` after the `0a17547d` redeploy, and the current listing proves no later version replaced it. This run did not repeat the upload |
| Cipher-store attachment session quota fix is in the active Worker | `unknown` | Earlier live evidence established the fixed TTL split on a deployment, but the allowed GETs do not prove the current bundle's admission accounting |

There is no live-output conflict with the coordinator's two current short
Worker IDs. The only direct conflict was the report's older `3374d057`
cipher-store snapshot, which is now explicitly superseded. Feature claims not
reachable by harmless GETs remain `unknown`; they are not inferred from version
timing or from sharing a source file.

## Acceptance rows this earns

No checklist edit and no acceptance point are claimed. This earns a
`verified-live` version identity and bounded safe-route evidence for both
Workers. Exact Git commits, authenticated keyserver success paths, current
inbox-cap enforcement, current pubkeys field minimisation, and current
cipher-store quota accounting remain `unknown`.

---

## Production D1 aggregate health — 2026-07-26 22:28 PDT

Canonical snapshot windows:

- keyserver: `2026-07-26T22:28:02-07:00` through
  `2026-07-26T22:28:05-07:00`
  (`2026-07-27T05:28:02Z` through `2026-07-27T05:28:05Z`);
- cipher-store: `2026-07-26T22:28:27-07:00` through
  `2026-07-26T22:28:29-07:00`
  (`2026-07-27T05:28:27Z` through `2026-07-27T05:28:29Z`);
- repository HEAD immediately before the canonical snapshots:
  `79f12eb79cf9e6baa374b33de7a5caf7f8991a19`.

Only aggregate `SELECT` statements and Wrangler's remote migration listing
were used. No identifier, key, payload, ciphertext, capability, IP, row body,
or R2 object was selected. Every canonical D1 result reported:

```json
{"changes":0,"changed_db":false,"rows_written":0}
```

Every evidence command used `WRANGLER_WRITE_LOGS=false`, so Wrangler did not
write the query/result to its disk log. An earlier schema-only trial used the
wrong suppression variable and created one ordinary sanitized Wrangler log;
it contained only the aggregate schema query and no row data. No later
evidence command retained a Wrangler log.

### Exact keyserver command

The SQL below is whitespace-formatted for readability; its selected columns
and predicates are exact.

```sh
cd keyserver-cf
PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
WRANGLER_WRITE_LOGS=false \
npx wrangler d1 execute osl-keyserver-prod --remote --json --command "
SELECT COUNT(*) AS applied_migration_count,
       COALESCE(MAX(id), 0) AS latest_migration_id,
       SUM(CASE WHEN name LIKE '0026_%' THEN 1 ELSE 0 END)
         AS migration_0026_record_count,
       SUM(CASE WHEN name LIKE '0027_%' THEN 1 ELSE 0 END)
         AS migration_0027_record_count,
       SUM(CASE WHEN name LIKE '0028_%' THEN 1 ELSE 0 END)
         AS migration_0028_record_count,
       SUM(CASE WHEN name LIKE '0029_%' THEN 1 ELSE 0 END)
         AS migration_0029_record_count,
       SUM(CASE WHEN name LIKE '0030_%' THEN 1 ELSE 0 END)
         AS migration_0030_record_count
  FROM d1_migrations;

SELECT (SELECT COUNT(*) FROM pragma_table_info('users'))
         AS users_column_count,
       (SELECT COUNT(*) FROM pragma_table_info('users')
         WHERE name = 'identity_lookup_enabled'
           AND type = 'INTEGER' AND \"notnull\" = 1 AND dflt_value = '0')
         AS migration_0029_column_shape_count,
       (SELECT COUNT(*) FROM pragma_table_info('control_inbox')
         WHERE name = 'kind') AS migration_0027_kind_column_count,
       (SELECT COUNT(*) FROM pragma_table_info('control_inbox')
         WHERE name = 'collapse_key') AS migration_0027_collapse_column_count,
       (SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND lower(name) LIKE '%dead%letter%')
         AS server_dead_letter_table_count;

SELECT COUNT(*) AS registered_identity_rows,
       COALESCE(SUM(CASE WHEN identity_lookup_enabled = 0
                         THEN 1 ELSE 0 END), 0)
         AS quarantined_identity_rows,
       COALESCE(SUM(CASE WHEN identity_lookup_enabled = 1
                         THEN 1 ELSE 0 END), 0)
         AS lookup_enabled_identity_rows,
       COALESCE(SUM(CASE WHEN identity_lookup_enabled NOT IN (0, 1)
                         THEN 1 ELSE 0 END), 0)
         AS invalid_lookup_status_rows,
       COALESCE(SUM(CASE WHEN length(user_id) BETWEEN 17 AND 20
                          AND user_id NOT GLOB '*[^0-9]*'
                         THEN 1 ELSE 0 END), 0) AS snowflake_shape_rows,
       COALESCE(SUM(CASE WHEN length(user_id) BETWEEN 17 AND 20
                          AND user_id NOT GLOB '*[^0-9]*'
                          AND identity_lookup_enabled = 0
                         THEN 1 ELSE 0 END), 0)
         AS quarantined_snowflake_shape_rows,
       COALESCE(SUM(CASE WHEN length(user_id) BETWEEN 17 AND 20
                          AND user_id NOT GLOB '*[^0-9]*'
                          AND identity_lookup_enabled = 1
                         THEN 1 ELSE 0 END), 0)
         AS enabled_snowflake_shape_rows
  FROM users;

SELECT (SELECT COUNT(*) FROM prekey_bundles) AS prekey_bundle_rows,
       (SELECT COUNT(*) FROM opk_pool) AS one_time_prekey_rows,
       (SELECT COUNT(*) FROM wrapped_keys) AS wrapped_key_rows,
       (SELECT COUNT(*) FROM wrapped_keys
         WHERE unixepoch(expires_at) >= unixepoch()) AS wrapped_key_live_rows,
       (SELECT COUNT(*) FROM wrapped_keys
         WHERE unixepoch(expires_at) < unixepoch()) AS wrapped_key_expired_rows;

SELECT COUNT(*) AS control_inbox_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch()
                         THEN 1 ELSE 0 END), 0) AS control_inbox_live_rows,
       COALESCE(SUM(CASE WHEN expires_at < unixepoch()
                         THEN 1 ELSE 0 END), 0) AS control_inbox_expired_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch() AND kind = ''
                         THEN 1 ELSE 0 END), 0) AS ordinary_live_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch()
                          AND kind = 'revocation'
                         THEN 1 ELSE 0 END), 0) AS revocation_live_rows,
       COALESCE(SUM(CASE WHEN kind NOT IN ('', 'revocation')
                         THEN 1 ELSE 0 END), 0) AS unknown_kind_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch()
                          AND length(sender_id) BETWEEN 17 AND 20
                          AND sender_id NOT GLOB '*[^0-9]*'
                         THEN 1 ELSE 0 END), 0) AS live_snowflake_sender_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch()
                          AND NOT EXISTS (
                            SELECT 1 FROM users u
                             WHERE u.user_id = control_inbox.sender_id
                               AND u.identity_lookup_enabled = 1
                          )
                         THEN 1 ELSE 0 END), 0)
         AS live_sender_not_lookup_enabled_rows
  FROM control_inbox;

SELECT COUNT(*) AS control_inbox_receipt_rows,
       COALESCE(SUM(CASE WHEN expires_at >= unixepoch()
                         THEN 1 ELSE 0 END), 0)
         AS control_inbox_receipt_live_rows,
       COALESCE(SUM(CASE WHEN expires_at < unixepoch()
                         THEN 1 ELSE 0 END), 0)
         AS control_inbox_receipt_expired_rows,
       COALESCE(SUM(CASE WHEN NOT EXISTS (
                            SELECT 1 FROM control_inbox i
                             WHERE i.id = control_inbox_requests.inbox_id
                          )
                         THEN 1 ELSE 0 END), 0)
         AS receipts_without_current_inbox_row
  FROM control_inbox_requests;

SELECT (SELECT COUNT(*) FROM consuming_get_receipts)
         AS consuming_get_receipt_rows,
       (SELECT COUNT(*) FROM consuming_get_receipts
         WHERE expires_at >= unixepoch()) AS consuming_get_receipt_live_rows,
       (SELECT COUNT(*) FROM consuming_get_receipts
         WHERE expires_at < unixepoch()) AS consuming_get_receipt_expired_rows,
       (SELECT COUNT(*) FROM wrapped_key_post_receipts)
         AS wrapped_key_post_receipt_rows,
       (SELECT COUNT(*) FROM wrapped_key_post_receipts
         WHERE expires_at >= unixepoch())
         AS wrapped_key_post_receipt_live_rows,
       (SELECT COUNT(*) FROM wrapped_key_post_receipts
         WHERE expires_at < unixepoch())
         AS wrapped_key_post_receipt_expired_rows;
" | jq '[.[] | {
  results,
  audit_meta: {
    changes: .meta.changes,
    changed_db: .meta.changed_db,
    rows_written: .meta.rows_written
  }
}]'
```

Exact retained result values:

```json
[
  {
    "applied_migration_count": 30,
    "latest_migration_id": 30,
    "migration_0026_record_count": 1,
    "migration_0027_record_count": 1,
    "migration_0028_record_count": 1,
    "migration_0029_record_count": 1,
    "migration_0030_record_count": 0
  },
  {
    "users_column_count": 10,
    "migration_0029_column_shape_count": 1,
    "migration_0027_kind_column_count": 1,
    "migration_0027_collapse_column_count": 1,
    "server_dead_letter_table_count": 0
  },
  {
    "registered_identity_rows": 172,
    "quarantined_identity_rows": 111,
    "lookup_enabled_identity_rows": 61,
    "invalid_lookup_status_rows": 0,
    "snowflake_shape_rows": 2,
    "quarantined_snowflake_shape_rows": 2,
    "enabled_snowflake_shape_rows": 0
  },
  {
    "prekey_bundle_rows": 0,
    "one_time_prekey_rows": 0,
    "wrapped_key_rows": 0,
    "wrapped_key_live_rows": 0,
    "wrapped_key_expired_rows": 0
  },
  {
    "control_inbox_rows": 23,
    "control_inbox_live_rows": 23,
    "control_inbox_expired_rows": 0,
    "ordinary_live_rows": 23,
    "revocation_live_rows": 0,
    "unknown_kind_rows": 0,
    "live_snowflake_sender_rows": 0,
    "live_sender_not_lookup_enabled_rows": 23
  },
  {
    "control_inbox_receipt_rows": 0,
    "control_inbox_receipt_live_rows": 0,
    "control_inbox_receipt_expired_rows": 0,
    "receipts_without_current_inbox_row": 0
  },
  {
    "consuming_get_receipt_rows": 0,
    "consuming_get_receipt_live_rows": 0,
    "consuming_get_receipt_expired_rows": 0,
    "wrapped_key_post_receipt_rows": 0,
    "wrapped_key_post_receipt_live_rows": 0,
    "wrapped_key_post_receipt_expired_rows": 0
  }
]
```

All seven result objects carried the exact audit metadata shown at the start of
this section.

### Keyserver findings

| Claim | Tier | Bound |
|---|---|---|
| Migration 0029 is recorded and its `identity_lookup_enabled INTEGER NOT NULL DEFAULT 0` column shape is present | `verified-live` | One migration-name record and one exact column-shape match; local DDL is `keyserver-cf/migrations/0029_authoritative_osl_identity.sql:12-14` |
| Production has 172 registered identity rows: 111 quarantined, 61 lookup-enabled, zero invalid statuses | `verified-live` | Aggregate `users` result; no identity was selected |
| Two rows have Discord-snowflake shape; both are quarantined and zero are lookup-enabled | `verified-live` | Aggregate shape/status predicates only |
| Migration 0027's `kind` and `collapse_key` columns are present; every live inbox row is ordinary | `verified-live` | Schema count 1+1; 23 ordinary live, zero revocation, zero unknown-kind |
| Production has 23 control-inbox rows, all live; zero expired | `verified-live` | Aggregate expiry predicates only |
| All 23 live inbox rows currently name a sender with no lookup-enabled `users` row | `verified-live` | Correlated existence count only. This is not an actual dead-letter count |
| Production has zero control-inbox request receipts, zero consuming-GET receipts, and zero wrapped-key POST receipts | `verified-live` | Aggregate receipt counts; all live/expired splits are zero |
| Production has zero prekey bundles, zero OPKs, and zero wrapped keys | `verified-live` | Aggregate B5 storage counts only |
| Production D1 has no server dead-letter table | `verified-live` | Schema-name aggregate is zero |
| Actual client dead-letter count | `unknown` | Dead-letter state is client-local (`crates/ipc/src/control_inbox_dead_letter.rs:1-7`), not in production D1. This audit did not read the local ledger because it contains row identifiers |

The internal D1 migration id `30` is **not** evidence that local migration
`0030` ran. The name aggregate says 0030 has zero records, and the read-only
Wrangler listing confirms it is pending:

```text
$ WRANGLER_WRITE_LOGS=false \
  npx wrangler d1 migrations list osl-keyserver-prod --remote
Migrations to be applied:
0030_reserve_derived_identity_namespace.sql
```

### Exact cipher-store command

```sh
cd cipher-store-cf
PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
WRANGLER_WRITE_LOGS=false \
npx wrangler d1 execute osl-cipher-store-prod --remote --json --command "
SELECT COUNT(*) AS applied_migration_count,
       COALESCE(MAX(id), 0) AS latest_migration_id,
       SUM(CASE WHEN name LIKE '0006_%' THEN 1 ELSE 0 END)
         AS migration_0006_record_count,
       SUM(CASE WHEN name LIKE '0007_%' THEN 1 ELSE 0 END)
         AS migration_0007_record_count
  FROM d1_migrations;

SELECT (SELECT COUNT(*) FROM pragma_table_info('attachment_objects'))
         AS attachment_object_column_count,
       (SELECT COUNT(*) FROM pragma_table_info('attachment_objects')
         WHERE name = 'state') AS state_column_count,
       (SELECT COUNT(*) FROM pragma_table_info('attachment_objects')
         WHERE name = 'upload_id') AS upload_id_column_count,
       (SELECT COUNT(*) FROM pragma_table_info('attachment_objects')
         WHERE name = 'content_expires_at') AS content_expiry_column_count,
       (SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name = 'attachment_parts')
         AS attachment_parts_table_count;

SELECT COUNT(*) AS attachment_object_rows,
       COALESCE(SUM(CASE WHEN state = 'uploading'
                         THEN 1 ELSE 0 END), 0) AS uploading_rows,
       COALESCE(SUM(CASE WHEN state = 'completing'
                         THEN 1 ELSE 0 END), 0) AS completing_rows,
       COALESCE(SUM(CASE WHEN state = 'ready'
                         THEN 1 ELSE 0 END), 0) AS ready_rows,
       COALESCE(SUM(CASE WHEN state NOT IN
                              ('uploading', 'completing', 'ready')
                         THEN 1 ELSE 0 END), 0) AS invalid_state_rows,
       COALESCE(SUM(CASE WHEN state IN ('uploading', 'completing')
                         THEN 1 ELSE 0 END), 0) AS incomplete_rows,
       COALESCE(SUM(CASE WHEN state IN ('uploading', 'completing')
                          AND expires_at >= unixepoch()
                         THEN 1 ELSE 0 END), 0) AS incomplete_live_rows,
       COALESCE(SUM(CASE WHEN state IN ('uploading', 'completing')
                          AND expires_at < unixepoch()
                         THEN 1 ELSE 0 END), 0) AS incomplete_expired_rows,
       COALESCE(SUM(CASE WHEN state = 'ready'
                          AND expires_at >= unixepoch()
                         THEN 1 ELSE 0 END), 0) AS ready_live_rows,
       COALESCE(SUM(CASE WHEN state = 'ready'
                          AND expires_at < unixepoch()
                         THEN 1 ELSE 0 END), 0) AS ready_expired_rows,
       COALESCE(SUM(CASE WHEN state IN ('uploading', 'completing')
                          AND NOT EXISTS (
                            SELECT 1 FROM attachment_parts p
                             WHERE p.attachment_id = attachment_objects.id
                          )
                         THEN 1 ELSE 0 END), 0)
         AS incomplete_without_part_rows,
       COALESCE(SUM(CASE WHEN state IN ('uploading', 'completing')
                          AND EXISTS (
                            SELECT 1 FROM attachment_parts p
                             WHERE p.attachment_id = attachment_objects.id
                          )
                         THEN 1 ELSE 0 END), 0)
         AS incomplete_with_part_rows,
       COALESCE(SUM(CASE WHEN content_expires_at IS NULL
                         THEN 1 ELSE 0 END), 0) AS null_content_expiry_rows,
       COALESCE(SUM(CASE
         WHEN (state = 'ready' AND upload_id IS NOT NULL)
           OR (state IN ('uploading', 'completing') AND upload_id IS NULL)
         THEN 1 ELSE 0 END), 0) AS invalid_state_upload_shape_rows
  FROM attachment_objects;

SELECT COUNT(*) AS attachment_part_rows,
       COALESCE(SUM(CASE WHEN NOT EXISTS (
                            SELECT 1 FROM attachment_objects o
                             WHERE o.id = attachment_parts.attachment_id
                          )
                         THEN 1 ELSE 0 END), 0)
         AS orphan_attachment_part_rows
  FROM attachment_parts;
" | jq '[.[] | {
  results,
  audit_meta: {
    changes: .meta.changes,
    changed_db: .meta.changed_db,
    rows_written: .meta.rows_written
  }
}]'
```

Exact retained result values:

```json
[
  {
    "applied_migration_count": 6,
    "latest_migration_id": 6,
    "migration_0006_record_count": 1,
    "migration_0007_record_count": 0
  },
  {
    "attachment_object_column_count": 9,
    "state_column_count": 1,
    "upload_id_column_count": 1,
    "content_expiry_column_count": 1,
    "attachment_parts_table_count": 1
  },
  {
    "attachment_object_rows": 1,
    "uploading_rows": 1,
    "completing_rows": 0,
    "ready_rows": 0,
    "invalid_state_rows": 0,
    "incomplete_rows": 1,
    "incomplete_live_rows": 1,
    "incomplete_expired_rows": 0,
    "ready_live_rows": 0,
    "ready_expired_rows": 0,
    "incomplete_without_part_rows": 1,
    "incomplete_with_part_rows": 0,
    "null_content_expiry_rows": 1,
    "invalid_state_upload_shape_rows": 0
  },
  {
    "attachment_part_rows": 0,
    "orphan_attachment_part_rows": 0
  }
]
```

All four result objects carried the exact audit metadata shown at the start of
this section.

### Cipher-store findings

| Claim | Tier | Bound |
|---|---|---|
| Migration 0006 is recorded; the attachment schema has `state`, `upload_id`, `content_expires_at`, and `attachment_parts` | `verified-live` | Migration/schema aggregate counts are each one; local DDL is `cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:8-36` and `0006_session_budget_and_atomic_rate_counters.sql:34-42` |
| Production D1 has one attachment object: uploading, live, incomplete, with zero part rows | `verified-live` | Fixed-state and expiry counts only |
| The one incomplete object has no part receipt and has null `content_expires_at` | `verified-live` | Aggregate predicates only |
| Production D1 has zero orphan attachment-part rows and zero invalid state/upload-id shape rows | `verified-live` | Aggregate referential/status checks only |
| Why the live incomplete row has null `content_expires_at`, who created it, and whether it is transient | `unknown` | No row, identifier, timestamp, capability, or write history was read. Current local source supplies this value on both session and direct inserts (`cipher-store-cf/src/endpoints/attachment.ts:203-206`, `:269-284`, `:510-520`), but the exact deployed source commit is already `unknown` |
| D1-object-to-R2-object consistency, including orphaned R2 multipart uploads or missing ready objects | `unknown` | Proving it requires R2 object/multipart reads, explicitly forbidden |

The read-only migration listing reports local migration 0007 pending:

```text
$ WRANGLER_WRITE_LOGS=false \
  npx wrangler d1 migrations list osl-cipher-store-prod --remote
Migrations to be applied:
0007_link_grant_consumption.sql
```

That is schema/version metadata only. It does not promote the dark link-grant
lane or turn preparatory work into a production repair.

### Coordinator-state reconciliation

| Coordinator statement | Tier | Current aggregate truth |
|---|---|---|
| Migration 0029 is applied | `verified-live` | One migration record and the exact column shape are present |
| “All 111 identities quarantined until re-registration” | `verified-live` for the current count; original-row continuity `unknown` | Exactly 111 rows remain quarantined, but production now has 172 total rows and 61 lookup-enabled rows. Aggregate-only evidence cannot prove whether the current 111 are exactly the migration-time cohort |
| Snowflakes are refused/quarantined | `verified-live` for stored status | Two stored rows have snowflake shape; both are quarantined and zero are lookup-enabled. The earlier harmless HTTP probe separately verified the refusal branch |
| Keyserver migrations 0027 and 0028 are applied | `verified-live` | Each has one migration record; 0027's two schema columns are present |
| Cipher-store attachment fixes are deployed | `verified-live` only for previously recorded R2 part success and the 0006 schema; current row origin remains `unknown` | This audit confirms 0006 schema support and zero D1 orphan part rows. It cannot re-prove R2 behavior, and the single live incomplete row's null promised-expiry value is an unresolved aggregate anomaly |

### B5 boundary

No acceptance boundary moves.

The 61 lookup-enabled identity rows are `verified-live` registration state, not
a successful two-identity protocol qualification. Production currently has
zero prekey bundles, zero OPKs, zero wrapped keys, and zero relevant receipts.
Its 23 live control-inbox rows are all ordinary, all have a sender that is not
lookup-enabled, and no authenticated drain was performed. Consequently this
snapshot cannot earn a registration/prekey/wrapped-key/control-inbox
end-to-end point or repair the implemented-unwired client boundaries recorded
above.

## Acceptance rows this earns

No checklist edit and no point are claimed. B5 remains 1/4. This audit earns
`verified-live` aggregate health evidence for migration 0029, identity status,
the production control-inbox backlog, receipt emptiness, and the D1-visible
attachment state. Actual client dead-letter count, authenticated delivery,
prekey/wrapped-key success, the incomplete attachment's origin, and all R2
orphan/missing-object questions remain `unknown`.

---

## Migration 0030 deploy-readiness and security audit

Timestamp: `2026-07-26T22:44:44-07:00`.

Scope was local source and local Miniflare D1 only. No Cloudflare API, remote
D1, registration, migration, or deployment command was run. The trace began at
committed `2bae4ede29930ee66ec9ca9cf0154fd776e4ce89`; before reporting, the
relevant paths were compared through committed
`aca9dae54dd8496b0af780bbe9f7ee96920f0454` and were byte-unchanged. This
section contains no `verified-live` claim.

### Exact path trace

The complete tracked-source search was:

```sh
git grep -n -E 'identity_scheme|ik_root_ed25519_pub' HEAD -- \
  keyserver-cf cipher-store-cf crates/keystore crates/ipc apps/osl-hub
git grep -n -F 'osl1_' HEAD -- \
  keyserver-cf cipher-store-cf crates/keystore crates/ipc apps/osl-hub
```

On the audited committed source, `identity_scheme` and
`ik_root_ed25519_pub` occurred only in migration 0030. No Worker or client
request/response type read or wrote either field. The current client
registration body has no such member
(`crates/keystore/src/client.rs:63-87`), and its pubkeys response has no such
member (`crates/keystore/src/client.rs:338-370`). Therefore scheme 1 remains
`implemented-unwired`: 0030 reserves durable shape; it does not implement a
derived-identity protocol.

The active boundary is instead the reserved identifier:

- `keyserver-cf/src/lib/validation.ts:9,29-32` defines exactly
  `^osl1_[a-z2-7]{32}$`.
- `keyserver-cf/src/endpoints/register.ts:111-123` rejects that namespace with
  400 before any D1 read or write.
- Existing Worker inserts and rotations enumerate only legacy columns
  (`keyserver-cf/src/lib/db.ts:103-128,138-170`); they neither depend on nor
  alter the two 0030 columns.
- The corrected migration fixes every durable row at exactly
  `(identity_scheme = 0, ik_root_ed25519_pub = NULL)` until a later reviewed
  proof-verification migration deliberately replaces the guards
  (`keyserver-cf/migrations/0030_reserve_derived_identity_namespace.sql:18-40`).

### Defects proved failing-first

The original committed migration was applied to a real local Miniflare D1
created from the exact 0001, 0026, and 0029 SQL. The positive compatibility
control passed, while both security controls failed:

```text
RUN v4.1.10
✓ preserves a legacy row and old-worker explicit writes as scheme 0
× refuses a flag-only promotion to scheme 1
  promise resolved ... changes:1, rows_written:1 instead of rejecting
× refuses storing a root on a scheme-0 row
  promise resolved ... changes:1, rows_written:1 instead of rejecting
Test Files 1 failed
Tests 2 failed | 1 passed
```

This is `runtime-proven`, not an inferred schema concern. A direct D1 update
could promote a row merely by flipping the flag, and an independent update
could attach unverified root bytes to scheme 0.

A second deploy-readiness defect was source-proved: the original migration
said to migrate first, but an old Worker has no reserved-namespace refusal.
Migration-first is SQL-compatible because omitted fields take the new
defaults, yet it leaves a window in which an attacker can register a future
`osl1_...` identifier as scheme 0. The migration cannot reserve an identifier
namespace by itself.

The fix adds insert and update triggers that refuse every non-preparatory
state and corrects the rollout comment to make the Worker refusal the first
security boundary. It does not claim or enable root proof verification.

### Non-vacuous runtime and mutation evidence

Permanent cross-boundary coverage is
`keyserver-cf/scripts/migration-0030.test.ts`. It uses Wrangler's SQL splitter
and a real Miniflare D1, calls the actual `handleRegister`, and independently
proves:

1. an existing row survives migration as scheme 0/null;
2. old-Worker-style explicit insert and update remain valid after migration;
3. the current Worker on a pre-0030 schema returns 400 for the reserved
   namespace, even with attacker-supplied scheme/root fields, and inserts zero
   rows;
4. that same pre-0030 Worker still returns 201 for ordinary signed scheme-0
   registration;
5. current signed scheme-0 registration returns 201 then 200 after migration
   and remains scheme 0/null even if unknown request flags are supplied;
6. direct flag-only update, root-only update, and scheme-1 insert all abort.

Candidate result:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
  ./node_modules/.bin/vitest run --config vitest.node.config.ts \
  scripts/migration-0030.test.ts --reporter=verbose
Test Files  1 passed (1)
Tests  6 passed (6)
```

Four isolated mutations then killed the intended assertions:

```text
reserved Worker check disabled:
  expected 201 to be 400
  Tests 1 failed | 5 skipped

update trigger disabled:
  flag-only promotion ... resolved ... changes:1, rows_written:1
  unverified root ... resolved ... changes:1, rows_written:1
  Tests 2 failed | 4 skipped

insert trigger disabled:
  direct scheme-1 insert ... resolved ... changes:1, rows_written:1
  Tests 1 failed | 5 skipped

identity_scheme default changed from 0 to 1:
  migrated legacy row was scheme 1, not scheme 0
  ordinary registration aborted at the guard
  Tests 2 failed | 4 skipped
```

The baseline was rerun in the same disposable archived tree before mutation:
six of six passed. These are `runtime-proven` negative controls: neither the
positive compatibility path nor any refusal path can pass solely because the
harness failed to exercise D1 or the Worker.

Focused Worker and type gates:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
  ./node_modules/.bin/vitest run \
  test/integration/register.test.ts test/integration/pubkeys.test.ts \
  --reporter=dot
Test Files  2 passed (2)
Tests  22 passed (22)

$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH npm run typecheck
> tsc --noEmit
```

### Compatibility, failure, and rollback verdict

| Claim | Tier | Exact bound |
|---|---|---|
| Migration-first is backward-compatible for existing scheme-0 rows and old explicit SQL | `runtime-proven` | Existing row, post-migration old-style insert, and old-style update all retain scheme 0/null |
| Migration-first is safe as a deployment order | `test-proven-only` refusal | It is **not** safe: disabling the Worker check registers the reserved ID with 201 on the pre-0030 shape, and the same SQL remains accepted after migration by default |
| Worker-first behavior before 0030 | `runtime-proven` | Reserved ID returns 400 and creates zero rows; ordinary signed registration returns 201 |
| A request flag or missing root proof can reach scheme 1 | `runtime-proven` refusal | Unknown request fields remain scheme 0/null; direct flag-only/root-only writes and direct scheme-1 insert abort |
| Existing scheme-0 registration is stable after 0030 | `runtime-proven` | First signed request returns 201, replay returns 200, stored aggregate is exactly scheme 0/null |
| Scheme-1 registration/root proof exists | `implemented-unwired` | No Worker/client field, canonical proof, verifier, or scheme-1 response exists |
| Exact deployed 0030 state | `unknown` | This task intentionally made no live probe; the preceding read-only snapshot reported 0030 pending |

D1 migration rollback is not an application rollback. The additive columns and
triggers have no reverse migration and must not be manually removed. After
0030, rolling back to any Worker that retains the exact reserved-namespace 400
is compatible. Rolling back to a Worker predating that refusal is
security-forbidden because it reopens namespace squatting while D1 silently
defaults the new row to scheme 0/null. Recovery from a bad 0030 rollout is
therefore forward-only: keep or restore a refusal-capable Worker, diagnose,
then ship a new numbered corrective migration. A later scheme-1 launch must
ship a reviewed canonical root proof and verifier plus a forward migration
that deliberately replaces these guards; flipping a flag is not an upgrade
path.

### Exact safe deployment order

> **Superseded 2026-07-27 for the current Scheme-1 lineage.** The `0030`
> Worker-first sequence below is retained as historical deploy-readiness evidence for the reserved
> namespace boundary. It is not the current rollout. Exact server/admission candidate
> `e273436dfaf735daab44adbbeb205a3c95ecd4cc` plus keystore construction lineage
> `a1b82a008f53d4864459d353bb6a89fc8753a446` now provide
> Scheme-1 canonical proof/client shapes at `test-proven-only`; the missing product boundary is the
> shipping register/fetch/replenish caller. The current blocked rollout is migrations `0033`, then
> `0034`, then the matching Worker. None is deployed or live-proven.

1. Build and release the Worker that contains the exact
   `isReservedDerivedId` refusal.
2. Verify that exact artifact/version through the release lane, including a
   harmless reserved-ID refusal probe if that lane authorizes it. Do not
   register any identity.
3. Apply `0030_reserve_derived_identity_namespace.sql`.
4. Run schema-only/read-only health checks for both new columns and both guard
   triggers.
5. Keep the refusal-capable Worker deployed. Do not enable scheme 1; it remains
   preparatory and unwired.

## Acceptance rows this earns

No checklist edit and no B5 point are claimed. This earns
`runtime-proven` deploy-readiness for the scheme-0 compatibility and
fail-closed preparatory D1 boundary, plus `test-proven-only` exact safe
rollout/rollback evidence. Derived identities remain `implemented-unwired`;
production migration state remains `unknown` in this task.

---

## Legacy multipart reservation cleanup

Timestamp: `2026-07-26T22:52:39-07:00`.

This follow-up was local-only at committed base
`cab3d3ec170e2aa933371e88c1051c94d1d68d63`. No production D1/R2 row, object,
multipart upload, migration, Worker, or deployment was read or mutated.

The defect lead came from the preceding aggregate-only production audit: one
row was `uploading`, live, had `content_expires_at IS NULL`, and had no
`attachment_parts`. Those aggregate facts remain `verified-live` only at that
earlier timestamp. This section does not infer that row's identifier, creator,
age, exact expiry, upload id, or R2 state.

### Failing-first proof

The current scheduled sweep selected only `expires_at < now`
(`cipher-store-cf/src/lib/sweep.ts` before this change). A real local
Miniflare D1/R2 test created:

- a valid R2 multipart upload and `upload_id`;
- an old `state = 'uploading'` metadata row;
- `content_expires_at = NULL`;
- a future legacy `expires_at`;
- zero D1 part receipts.

The test also uploaded one R2 part directly without creating a D1 receipt, so
the multipart handle was non-vacuous while the metadata shape exactly exercised
the no-part predicate. Before the fix:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
  ./node_modules/.bin/vitest run test/attachment-sweep.test.ts \
  -t "stale legacy no-part" --reporter=verbose
× expires a stale legacy no-part reservation and aborts R2 before metadata removal
  expected +0 to be 1
Test Files  1 failed (1)
Tests  1 failed | 5 skipped (6)
```

This is `runtime-proven`: a legacy-shaped reservation older than the current
15-minute incomplete-session hold was not reclaimed while its long
`expires_at` remained in the future.

### Smallest bounded fix

At the start of each existing attachment sweep batch, one atomic metadata-only
statement now marks at most `ATTACHMENT_SWEEP_BATCH_SIZE` rows expired when all
of these are true:

- `state = 'uploading'`;
- `content_expires_at IS NULL`;
- `created_at` is older than `INCOMPLETE_SESSION_TTL_SECONDS`;
- the row is not already expired;
- no `attachment_parts` receipt exists.

The exact predicate and batch limit are
`cipher-store-cf/src/lib/sweep.ts:96-127`. The marked row then enters the
unchanged cleanup path: `removeAttachmentStorage` resumes and aborts the
multipart upload first (`cipher-store-cf/src/lib/sweep.ts:129-146`;
`cipher-store-cf/src/endpoints/attachment.ts:562-575`), and only afterward can
the conditional D1 delete run (`cipher-store-cf/src/lib/sweep.ts:147-155`).
There is no new direct-delete branch. If R2 abort fails, the now-expired D1
metadata remains retryable for the next scheduled run.

### Runtime and mutation evidence

The focused real-D1/R2 file passes eight tests:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
  ./node_modules/.bin/vitest run test/attachment-sweep.test.ts \
  --reporter=verbose
Test Files  1 passed (1)
Tests  8 passed (8)
```

The new positive control records the D1 row count from inside the real resumed
multipart handle's `abort`: it observes `1`, the subsequent upload-part call
fails because R2 was aborted, and only then is the final D1 count `0`
(`cipher-store-cf/test/attachment-sweep.test.ts:161-220`).

The R2-failure control makes `abort` throw, proves metadata was present at that
moment, then proves the row remains present and marked expired and the real
multipart upload is still usable
(`cipher-store-cf/test/attachment-sweep.test.ts:222-284`). Negative controls
also prove that a fresh legacy row, a current-schema row with non-null promised
expiry, and a legacy row with a D1 part receipt are not marked or aborted
(`cipher-store-cf/test/attachment-sweep.test.ts:286-352`).

Candidate baseline in a disposable archive:

```text
Test Files  1 passed (1)
Tests  3 passed | 5 skipped (8)
```

Independent semantic mutations failed as intended:

```text
legacy mark disabled:
  stale row: expected 0 to be 1

content_expires_at IS NULL predicate removed:
  protected controls: expected 1 to be 0

created_at age predicate neutralized while retaining its bind:
  protected controls: expected 1 to be 0

NOT EXISTS attachment_parts predicate removed:
  protected controls: expected 1 to be 0

R2 abort call bypassed while D1 deletion remained:
  expected resumeMultipartUpload call, received 0 calls
  abort-failure path resolved 1 instead of rejecting
```

These mutations make both selection safety and R2-before-D1 ordering
non-vacuous. The controls fail on the exact missing guard rather than on test
setup or SQL binding errors.

Full Worker and type gates:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH npm test
Test Files  12 passed (12)
Tests  104 passed (104)

$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH npm run typecheck
> tsc --noEmit
```

### Status boundary

| Claim | Tier | Exact bound |
|---|---|---|
| Current local source detects old null-expiry/no-part reservations after 15 minutes | `runtime-proven` | Real D1 selection and real valid multipart handle |
| Multipart abort precedes metadata removal | `runtime-proven` | Abort callback observes D1 count 1; post-sweep count is 0 |
| Abort failure retains retryable metadata | `runtime-proven` | Throwing abort leaves count 1 and expired metadata |
| Fresh, current-schema, and part-receipted rows are excluded | `runtime-proven` | Three nonempty negative controls, each with a valid multipart handle |
| Local scheduled cleanup path has this behavior | `runtime-proven` | `scheduled()` already calls this exact sweep; real local D1/R2 exercised the called function |
| Production cleanup has this behavior | `unknown` | No deploy or post-deploy probe was performed |
| Exact production row origin or R2 multipart state | `unknown` | No row content or R2 state was read |

No migration is needed for this Worker-only repair. A future authorized release
would deploy the Worker first and then use aggregate-only D1 health counts on a
later scheduled cycle; it must not directly delete the observed row or manually
abort an unknown multipart upload.

## Acceptance rows this earns

No checklist edit and no B5 point are claimed. This earns
`runtime-proven` local cleanup safety for the legacy reservation shape and
R2-before-D1 failure ordering. Production remediation, and the specific
production row's age, origin, identifier, and R2 state, remain `unknown`.

---

## Control-inbox disabled-sender retention (migration 0031)

Timestamp: `2026-07-27T00:01:15-07:00`.

This work was local-only. The failing-first run began at committed HEAD
`f85b0810b09429d24db3d7951d85b30c02ff9d25`; final verification ran while
unrelated lanes had advanced HEAD to
`83e3c831dd72b8c2671f3e0bd1ed48952c0238b1`. No production Worker, D1
migration, D1 row, R2 object, identity, or provider state was read or mutated.
No cipher-store path changed.

### Failing-first proof

The pre-0031 hourly sweep selected expired rows using only `expires_at`.
A real local Miniflare D1 fixture registered a sender and recipient, disabled
the sender through the exact `identity_lookup_enabled` column, inserted a
nonempty opaque bundle, and invoked the scheduled Worker. Before the fix:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH \
  ./node_modules/.bin/vitest run \
  test/integration/control-inbox-sweep.test.ts \
  -t "does not silently delete" --reporter=verbose
[cron] control_inbox sweep deleted 1 row(s) and 0 request receipt(s)
× expected null not to be null
Test Files  1 failed (1)
Tests  1 failed | 1 skipped (2)
```

This is `runtime-proven`: the fixture first asserted a row count of one and
used a nonempty bundle, so the deletion failure could not pass or fail
vacuously.

### Forward schema and state machine

Migration `0031_control_inbox_sender_retention.sql` adds:

- `delivery_status` (`live`, `retryable`, `quarantined`, `retired`);
- a closed reason vocabulary;
- attempt, first-seen, next-retry, and bounded-retention metadata;
- D1 state-shape, lookup-truth, and legal-transition guards;
- physical-lane quota indexes and replacement quota triggers;
- a schema capability marker; and
- a migration-first DELETE guard.

The columns/defaults are
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:18-47`.
Physical quota backstops are at
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:78-154`;
the exact capability marker is at
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:156-165`;
and the old-Worker DELETE guard is at
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:167-197`.
Lookup/transition authority and exact seven-day state guards occupy
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:199-424`.

The scheduled reconciler examines at most 100 rows per tick
(`keyserver-cf/src/lib/control-inbox-sweep.ts:96-135`):

- an exact 17-20 digit Discord snowflake retires immediately with
  `sender_discord_snowflake`, even if a legacy users row claims lookup is
  enabled; retirement is terminal;
- every other disabled/missing sender receives three attempts, one hour apart,
  then quarantines;
- malformed sender identifiers follow that same bounded retry path but retain
  the distinct `sender_identifier_malformed` reason;
- a re-enabled sender is restored to `live`, with its original opaque bundle
  untouched and its delivery window extended to the recorded retention
  deadline; and
- the retention deadline is exactly first-seen plus 604800 seconds; and
- compare-and-swap metadata updates recheck lookup state inside SQL
  (`keyserver-cf/src/lib/control-inbox-sweep.ts:100-295`).

Cleanup first reconciles, then deletes at most 100 rows. An expired live row is
eligible only when its sender is currently lookup-enabled. A retryable row is
never a cleanup candidate, including after scheduler downtime; only a recorded
quarantined/retired row becomes eligible after its exact retention deadline
(`keyserver-cf/src/lib/control-inbox-sweep.ts:298-349`). The Worker never
selects or updates `bundle` during reconciliation.

### Endpoint, quota, and privacy boundary

All control-inbox routes require the exact schema capability and answer 503 on
a pre-0031 schema
(`keyserver-cf/src/endpoints/control-inbox.ts:250-261`,
`:759-776`, `:956-966`). Health is 503/capability 0 before the migration and
200/capability 1 only when both the marker and zero-row column projection pass
(`keyserver-cf/src/endpoints/healthz.ts:1-20`;
`keyserver-cf/src/lib/control-inbox-sweep.ts:39-75`).

Both filtered and unfiltered drains select only live rows
(`keyserver-cf/src/endpoints/control-inbox.ts:843-873`). A signed filtered
drain adds only four aggregate counts:

```json
{"live":0,"retryable":1,"quarantined":0,"retired":0}
```

It does not expose disposition reasons, attempts, first-seen/retention
timestamps, identifiers beyond the already signed sender echo, or hidden
payload bytes (`keyserver-cf/src/endpoints/control-inbox.ts:908-951`).
This additive response is `implemented-unwired`: the server schema is present,
but the broker/client owner must consume these counts in its own lane.

Retained non-live rows are excluded from sender recycling but count toward the
hard physical ordinary and revocation caps in both Worker prechecks and D1
race-backstop triggers
(`keyserver-cf/src/endpoints/control-inbox.ts:365-461`;
`keyserver-cf/migrations/0031_control_inbox_sender_retention.sql:78-154`).
A real-D1 endpoint test holds 512 future-expiry quarantined rows across 16
senders, posts a fresh authenticated bundle from a seventeenth sender, and
proves an explicit `429 recipient_inbox_full` with exactly 512 quarantined rows
and zero live rows afterward. A separate 32-row pair test proves
`sender_recipient` refusal and byte-for-byte preservation of every held bundle
(`keyserver-cf/test/integration/control-inbox-retention.test.ts:587-693`).

### Runtime, old-Worker, and mutation evidence

The focused real-Worker/D1 controls cover live, authenticated disabled,
re-enabled, enabled-legacy snowflake, malformed, mixed, recipient/pair physical
quota, retained-expiry, scheduler downtime, lookup truth, transition legality,
exact upper retention, and malformed metadata paths
(`keyserver-cf/test/integration/control-inbox-retention.test.ts:177-927`).
The 101-row scheduled fixture proves exactly 100 classifications on the first
tick and retains the unclassified tail
(`keyserver-cf/test/integration/control-inbox-sweep.test.ts:194-257`).

The exact forward-migration suite proves three non-vacuous boundaries:

1. pre-existing rows plus old INSERT/UPDATE/SELECT SQL preserve exact bundle
   bytes and take live/empty defaults;
2. the new Worker refuses POST/GET/DELETE and health with 503 before 0031,
   then reports capability 1 and reaches ordinary input validation after 0031;
3. marker-only, wrong-version, and missing-column mixed states remain 503; and
4. an old drain query returns a post-classification retryable payload, proving
   rollback is unsafe, while the D1 DELETE guard prevents the old cleanup or
   drain from erasing it.

Those controls are
`keyserver-cf/scripts/migration-0031.test.ts:125-375`.

Independent disposable-copy mutations all failed the named control:

```text
snowflake retirement removed:       1 failed
re-enabled selection removed:       1 failed
malformed reason collapsed:         1 failed
live lookup predicate inverted:     1 failed
batch bound 100 -> 101:              1 failed
drain live-status filter removed:    1 failed
retained quota filters removed:      1 failed
old-Worker default live -> retryable:1 failed
migration DELETE guard disabled:     1 failed
endpoint schema gate bypassed:       1 failed
health capability gate bypassed:     1 failed
```

The final adversarial mutations also failed non-vacuously:

```text
exact retention weakened to a lower bound:
  expected rejection, update resolved with changes=1
physical recipient cap 512 -> 513:
  expected 429, received 201
retryable cleanup enabled + D1 retry guard removed:
  expected 0 deletions after attempt 2, received 1
snowflake Worker retirement disabled:
  D1 state guard rejected generic retryable state
marker accepted without six-column projection:
  expected schemaReady false, received true
```

Changing only one of the redundant retry/snowflake guards can still be caught
by the other D1/Worker guard. The paired retry mutation proves the lifecycle
test itself fails if both independent protections regress.

Full Worker, Node, and type gates:

```text
$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH npm test
Test Files  42 passed (42)
Tests  406 passed (406)
Test Files  4 passed (4)
Tests  15 passed (15)

$ PATH=/home/liamw/.nvm/versions/node/v24.14.0/bin:$PATH npm run typecheck
> tsc --noEmit
```

| Claim | Tier | Exact bound |
|---|---|---|
| Disabled rows are retained and classified rather than silently TTL-deleted | `runtime-proven` | Real local Worker + Miniflare D1, nonempty bundle |
| Snowflakes retire; opaque/malformed senders retry then quarantine | `runtime-proven` | Positive and negative real-D1 state transitions plus mutations |
| Re-enabled sender payload becomes live without byte change | `runtime-proven` | Exact bundle comparison and authenticated filtered drain |
| Reconciliation and deletion are bounded at 100 each | `runtime-proven` | 101-row tail plus batch-bound mutation |
| Retention has an exact seven-day upper bound | `runtime-proven` | D1 rejects deadline +1; weakening mutation resolves and fails |
| Scheduler downtime cannot skip retryable state | `runtime-proven` | Overdue attempt 1 advances to attempt 2 with zero deletion, then records quarantine before cleanup |
| Retained rows cannot create a second physical quota | `runtime-proven` | Authenticated recipient/pair endpoint refusals preserve 512/32 nonempty rows |
| Migration-first old-Worker writes remain compatible | `runtime-proven` | Pre-row, post-migration old INSERT/UPDATE/SELECT |
| Migration-first old cleanup cannot erase disabled rows | `runtime-proven` | Exact old DELETE affects zero; guard mutation deletes and fails |
| Old Worker is safe after classification | `test-proven-only` refusal | It is not: exact old SELECT exposes retained bytes; rollback is forbidden |
| Marker-only or wrong-version schema is accepted | `test-proven-only` refusal | Zero-row six-column projection and exact version are both required |
| Broker distinguishes aggregate disposition | `implemented-unwired` | Additive fixed counts exist; no broker path was edited here |
| Production migration/Worker behavior | `unknown` | No deploy, migration, or live probe occurred |

### Exact safe deployment order

1. Before any pending D1 migration, verify the deployed Worker already contains
   the exact 0030 reserved-namespace refusal from `8802225`. If it does not,
   deploy and verify that pre-0031-compatible refusal Worker first.
2. Apply pending migrations in numeric order: 0030 (if still pending), then
   `0031_control_inbox_sender_retention.sql`. The 0031 DELETE guard protects the
   brief old-Worker window, but that window must not be prolonged.
3. Immediately deploy the exact Worker commit containing this section.
   Worker-first is not supported: its capability and drain queries name
   nonexistent 0031 schema and its routes deliberately answer 503.
4. Require `GET /v1/healthz` from that exact Worker version to return HTTP 200
   with `capabilities.control_inbox_sender_disposition = 1`. The read-only
   `node scripts/post-deploy-probe.mjs --host "$KS"` gate rejects legacy
   health, wrong versions, and 503
   (`keyserver-cf/DEPLOY.md:672-740`).
5. Treat the release as failed until that exact capability is observed.
   Subsequent health review may use aggregate counts only; do not inspect row
   contents.

There is no down migration. Before the first reconciliation, old SQL remains
write-compatible but lacks the required health capability. After any row is
classified, rollback to a pre-0031 Worker is security-forbidden: its old SELECT
ignores disposition and returns retained bytes. Recovery is forward-only with
the 0031 schema kept in place.

## Acceptance rows this earns

No checklist edit and no B5 point are claimed. This earns `runtime-proven`
local server retention, bounded-cleanup, old-Worker migration-window, physical
quota, privacy, mixed-schema refusal, and forward-only downgrade evidence. The
broker count consumer is `implemented-unwired`; migration 0031 and the Worker
are not deployed, so this B5 server boundary remains non-live and production
status remains `unknown`.
