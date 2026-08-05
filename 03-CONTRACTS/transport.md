# Opaque-envelope transport contract

**Status:** NOT frozen. Written 2026-08-05 under W3-8 (defect D-272) to end a
dangling deferral, not to settle a design. **Refreshed 2026-08-05** under W3-7,
after D-273 and D-274 changed the behaviour this document had recorded: every
OBSERVED statement below was re-read against the source rather than carried
forward, and re-derived from that source rather than from the fixing lane's own
account of what it did. Still not frozen.

`spaces.md` (T21-C1, frozen) states that the space-event lane's "envelope and
capability rules are those of [`transport.md`](transport.md) §6b". **This
document did not exist.** The gate that was supposed to hold that deferral
honest — `keyserver-cf/scripts/t21-space-contract.test.mjs` — asserted that the
*link text* was present, so it passed for as long as the link pointed at
nothing. The rules the lane was told it inherited were never written down.

## What this document is, and what it is not

Every rule in §6b below is one of exactly three things, and each is labelled:

| label | meaning |
|---|---|
| **OBSERVED** | a description of what the shipping implementation demonstrably does. Cited to file and line. Descriptive: recording it here does not endorse it. |
| **DERIVED** | a requirement that follows from a rule already binding elsewhere in this repository. The binding rule is cited. |
| **OPEN** | a question this lane cannot answer from the code or its neighbours. **Written down as a question on purpose.** |

Nothing here is an invented rule presented as a settled one. Where the answer
was not derivable, the question is recorded instead — an unanswered question is
honest, a fabricated rule that looks decided is worse than the void it fills,
because the next reader stops looking.

**An OBSERVED label is a perishable claim, and this document has already been
overtaken once.** Within hours of the first draft, D-273 replaced the drain's
delete-on-transmission with a lease plus a new acknowledgement route, and D-274
added a bounded retention sweep and a lifetime ceiling. Between them they
falsified parts of §6b.1, §6b.2 and §6b.3 as first written and **answered two of
the five OPEN questions from the code side.** That is the labelling working, not
failing: a statement marked OBSERVED and cited to a line is one a later reader
can check and find false, which is the property that makes the label worth
having. It also means the citations are the load-bearing part — line numbers in
`space-events.ts` moved when D-273 landed, and every one below has been
re-verified at the revision that carries
`keyserver-cf/migrations/0043_space_event_expiry_hardening.sql`.

### A near-miss that must not be mistaken for the referent

A document named transport.md **does** exist in the private OSL plan repo, at
plan/03-CONTRACTS/ in that repository, and it **does** have a §6b. (That path is
written as prose, not as a link or a backticked file name, because this
document's own rule — stated below — is that a *typed* reference must resolve
inside this repository. An external citation names its repository instead.) It
is not this lane's referent, and citing it as one would be wrong on three
counts:

1. It belongs to a **different, independent contract set**. This repository's
   `03-CONTRACTS/` is not a mirror of that directory — the documents that share
   a name (`entitlement.md`, `lifecycle.md`) are different documents of
   different lengths, not copies.
2. Its `§6b` **contains no capability rules and no envelope rules.** It defines
   a `delivery_tag` derivation, a subscription model, a T1/T6 ownership split,
   and one separation invariant. There are no envelope rules there to inherit.
3. Its tag is **not this lane's tag.** That `§6b` derives a **16-byte**
   `delivery_tag` rendered as 32 hex characters. This lane's `recipient_tag` is
   **32 bytes, base64** (`space-events.ts:41`, `TAG_BYTES = 32`). Different
   length, different encoding, different lane.

The rules were never written, and the nearest same-named section would not have
supplied them.

**A consequence of creating this file, recorded because it is a new hazard and
not an old one.** Until 2026-08-05 the name `transport.md` resolved
unambiguously to the plan-repo document, so other documents here could cite it
in prose without confusion. One does:
[the Space delivery-tag finding](../docs/design/osl-spaces-delivery-tag.md)
opens "`transport.md` section 6b defines a delivery tag from `K_conv_n`". **That
is true of the plan-repo §6b and false of the section below**, which defines no
derivation at all. The sentence was accurate when written and is now ambiguous
because this file exists. It is named here rather than edited: that document is
an analysis contract owned by T1 and T6, and this lane does not rewrite other
owners' findings to make its own filename convenient. **The reference is also
outside the reach of the D-272 gate twice over** — the file is not in
`03-CONTRACTS/`, and it writes "section 6b" rather than `§6b`, which the
resolver's section token does not match. A gate catching this class would have
to be a different, wider gate; it is not this one, and pretending otherwise
would repeat D-272 in a new place.

---

## 6b. Opaque-envelope lanes — envelope and capability rules

This section is numbered `6b` because `spaces.md` is frozen and cites that
number. The numbering gap above it is not a set of reserved placeholders; there
are no other sections, and this label exists to make an existing frozen citation
resolve rather than to imply a hidden `§1`–`§6a`.

Scope: the space-event lane, implemented in
`keyserver-cf/src/endpoints/space-events.ts` over the `space_event_queue` table
(`migrations/0041_space_event_queue_reserved.sql`, as altered by
`migrations/0043_space_event_expiry_hardening.sql`). Since D-273 the lane is
**three** routes and one scheduled job, not two routes: `POST /v1/space-events`,
the `GET` drain, `POST /v1/space-events/ack`, and the hourly retention sweep.

### 6b.1 The envelope — OBSERVED

`POST /v1/space-events` accepts a JSON object. Exactly three fields are read
(`space-events.ts:97-99`); any other field is ignored, not rejected.

| field | rule | source |
|---|---|---|
| `recipient_tag` | base64 that decodes to **exactly 32 bytes** | `space-events.ts:41,112` |
| `ciphertext` | base64 that decodes to **at least 1 and at most 65,536 bytes** | `space-events.ts:42,112` |
| `expires_at` | a **safe integer**, Unix seconds, **strictly greater than** the accept-time clock **and at most 7 days beyond it** | `space-events.ts:60,112` |

- **Accept** is `202` with `{"accepted": true}` (`space-events.ts:118`).
- **Every** envelope violation returns the **same** `400` with the **same**
  message, `invalid opaque Space event envelope` (`space-events.ts:113`). A
  caller cannot tell a bad tag length from a bad ciphertext length from a stale
  `expires_at` from an over-long one. This is a single indistinguishable
  rejection and must stay one: a per-field error message is a probing aid on a
  route whose only secret is the shape of what a caller already holds.

**The upper bound on `expires_at` is new (D-274), and it is a retention control
rather than input hygiene.** `MAX_EVENT_LIFETIME_SECONDS = 7 * 24 * 60 * 60`
(`space-events.ts:60`) is the same ceiling the sibling lane puts on its own
retention (`MAX_WRAPPED_KEY_LIFETIME_MS`,
`keyserver-cf/src/endpoints/wrapped-keys.ts:46`). It is what makes §6b.6 a
policy instead of a gesture: the sweep keys on `expires_at`, so without a
ceiling one accepted `POST` could pin ciphertext in the relay past any horizon
and no sweep would ever reach it. It was folded into the **same**
indistinguishable `400`, so answering a retention problem created no new probing
surface.

### 6b.2 What the relay may store — OBSERVED, and load-bearing

The stored row is now **six** columns, not five: the five of
`0041_space_event_queue_reserved.sql:3-9` — a 16-byte random `id`, the
`recipient_tag`, the `ciphertext`, `expires_at`, and `created_at` — plus
`lease_until INTEGER NOT NULL DEFAULT 0`, added by
`0043_space_event_expiry_hardening.sql:16`. Three indexes:
`(recipient_tag, created_at)` and `(expires_at)` from `0041:10-13`, and
`(recipient_tag, lease_until, created_at)` from `0043:20-21`.

**No account identifier, roster, Space identifier, event kind, or sender
attribute may be added to this table.** The implementation's own header states
the reason (`space-events.ts:3-5`): each such column lets the relay enumerate a
Space rather than merely relay ciphertext. `spaces.md` states the same
prohibition from the other side.

`lease_until` is the first column ever added to this table, and it is a test of
that prohibition rather than an exception to it. It records **when a delivery
attempt stops being visible** and nothing about who is in what Space. The
migration header says exactly that (`0043:12-15`) — which is the author's claim
about the author's change, so it is worth confirming independently: the value is
a Unix second computed from the server clock at drain time
(`space-events.ts:126,138`), written by the relay from data it already had, and
read back only as `lease_until <= now` (`space-events.ts:127`). It admits no
caller-supplied content.

**Enforcement, stated exactly rather than generously — and this is a live gap.**
The first draft of this section said the prohibition "is already enforced in two
places and is not in question". Re-read against the code, **the second place
covers `0041` only.** `keyserver-cf/scripts/t21-space-events.test.mjs:7,11` is
the gate that holds the prohibition against the schema, and it opens exactly one
file — `0041_space_event_queue_reserved.sql` — asserting its non-comment text
does not match `/user_id|space_id|member|roster/i`. A prohibition enforced
against one migration of a three-migration reservation is a prohibition enforced
against the past: `0043` added the first column this table has ever gained and
that gate could not see it, and `0042` could add another tomorrow with the same
result.

**The gap is recorded rather than closed here, and the reason is itself worth
recording.** The obvious fix — have the gate read every migration naming
`space_event_queue` — was written, mutation-proved, and then **reverted**. It
adds two source-text assertions, which moves ledger 10's pin census off its
recorded baseline (4691 → 4693) and turns that ledger RED. The sanctioned way to
land it is one change that both widens the gate *and* re-anchors
`scripts/ledger/pin-baseline.json` with a stated reason, and that baseline
belongs to a lane other than this one. **Widening a gate while leaving a
ratcheted census broken is not an improvement**, so the fix is handed over
whole. Until it lands, read this section's enforcement claim as: the module
header states the rule, and one gate holds it against `0041`.

### 6b.3 The drain — OBSERVED, and no longer destructive

`GET /v1/space-events/:recipient_tag`:

- The tag in the request must decode to exactly 32 bytes, else `400`
  `invalid recipient tag` (`space-events.ts:124`).
- The read selects rows for that **exact** tag with `expires_at > now` **and
  `lease_until <= now`**, ordered by `created_at`, **limited to 64**
  (`MAX_DRAIN`, `space-events.ts:43,127-128`).
- **Every row returned is then leased, not deleted** — a batched
  `UPDATE … SET lease_until = ? WHERE id = ? AND recipient_tag = ?` binding
  `now + LEASE_SECONDS` (`space-events.ts:135-139`), with
  `LEASE_SECONDS = 60` (`space-events.ts:53`).
- The response is `{"events": [{"event_id": …, "ciphertext": …}]}`
  (`space-events.ts:140`). `event_id` is **new** (D-273): the row's own 16-byte
  `id` as 32 lowercase hex characters (`space-events.ts:44-46,74-76`). That id
  is `crypto.getRandomValues` output (`space-events.ts:88-92`), so it is an
  opaque handle rather than a derivation and carries no information about the
  event, the tag, or the queue.
- **The lease write is not best-effort.** A failure in the batch fails the whole
  request (`space-events.ts:141`); the rows stay visible and the caller retries.
  The error direction here is duplicate delivery, never silent loss.

**This supersedes what the first draft of this document recorded**, and the
superseded text is kept for one paragraph because a reader arriving from the
earlier revision needs to know it was overtaken rather than wrong at the time.
As observed on 2026-08-05 before D-273, the drain deleted every row it returned
(then `space-events.ts:62`) before the response was serialized, so a dropped
response destroyed the only copy of a membership event. That is no longer true
of any line in the file.

**No existence oracle** — still held, and now across a wider set of states. A
well-formed tag with nothing queued, a well-formed tag that was never used, and
a well-formed tag whose rows are all currently leased are indistinguishable:
all three return `200` with an empty `events` array. Only a **malformed** tag is
distinguishable, which reveals nothing about any tag's existence. This property
must be preserved — and note that the acknowledgement route was built so as not
to break it (§6b.5).

**"Returns and consumes" (T21-C1) still holds on the reading the code takes:** a
leased row stops being returned. The frozen wording was not changed, and neither
the drain's method nor its path was touched. Whether "consumes" was *meant* to
require destruction is not resolved here and is not this document's to resolve.

### 6b.4 The capability — OBSERVED

**Possession of the current 32-byte `recipient_tag` is the entire capability.**
There is no authentication, no signature, no bearer token, and no capability
check on **any of the three routes**. The drain reads and leases, and the ack
destroys, on the strength of the tag alone.

The acknowledgement adds no capability and subtracts none. Its `DELETE` is
scoped `WHERE id = ? AND recipient_tag = ? AND lease_until > 0`
(`space-events.ts:177-179`), so a leaked `event_id` on its own destroys nothing,
and anyone able to acknowledge an event could already have drained it
(`space-events.ts:152-154`).

After D-260, all three routes sit behind the worker's ingress rate limits — the
mutation limit on both `POST`s (`index.ts:343-356`, routes at `index.ts:477`
and `index.ts:481`), the public-GET limit on the drain (`index.ts:331-341`,
route at `index.ts:403-404`). Those bound the *cost* of probing the tag space;
they are not an authorization check and must not be described as one.

**There is still no invite capability in this repository.** `spaces.md` reserved
migrations `0041`–`0043` for "event queue, invite capability state, and
expiry/index hardening". `0041` and `0043` now exist. **`0042` — the invite
*capability* state — still does not**; `keyserver-cf/migrations/` steps straight
from `0041` to `0043`, and the D-274 lane recorded that it deliberately did not
take it (`0043_space_event_expiry_hardening.sql:4`). So the "capability rules"
this lane was told it inherited still amount to exactly one rule: **the tag is a
bearer capability.**

### 6b.5 The acknowledgement — OBSERVED (new since the first draft)

`POST /v1/space-events/ack` (`index.ts:481`; handler `space-events.ts:156-184`)
is a **new** route, added so that neither frozen T21-C1 route had to change.

- Exactly two fields are read: `recipient_tag` and `event_ids`
  (`space-events.ts:159-160`).
- `recipient_tag` must decode to exactly 32 bytes; `event_ids` must be a
  non-empty array of at most 64 entries (`space-events.ts:161`); each entry must
  be exactly 32 **lowercase** hex characters (`space-events.ts:78-80` — the
  pattern is `/^[0-9a-f]+$/`, so uppercase hex is rejected).
- Every violation returns the **same** `400`,
  `invalid Space event acknowledgement` (`space-events.ts:162,169`) — the same
  single-rejection discipline as §6b.1.
- The response is a **constant** `{"acknowledged": true}`
  (`space-events.ts:180-182`). It does not report how many rows matched, and
  that is the point: a count would turn this route into the existence oracle
  §6b.3 is careful not to be.

**DERIVED — the tag does not go back into a request path.** D81 removed
`GET /v1/usernames/:username` because a handle in a request path is written into
every intermediary's default log (`index.ts:413-417`), and the ack was shaped to
match: the tag rides in the body (`space-events.ts:147-151`). Any future route
in this lane must do the same. The `GET` drain is the standing exception, and it
is an exception only because `spaces.md` T21-C1 freezes it — see OPEN-4.

### 6b.6 Retention — OBSERVED (new since the first draft)

`keyserver-cf/src/lib/space-event-sweep.ts` deletes expired rows and is called
from the worker's `scheduled()` handler (`index.ts:281-294`). It runs on the
**hourly** cron: the five-minute branch returns at `index.ts:183` and the
midnight branch at `index.ts:198`, so the sweep is on the fall-through path,
reached by `0 * * * *` (`keyserver-cf/wrangler.toml:141`).

- Bounded: batches of `SPACE_EVENT_SWEEP_BATCH_SIZE = 500`, at most
  `SPACE_EVENT_SWEEP_MAX_BATCHES = 10` per run — 5,000 rows
  (`space-event-sweep.ts:22-25`).
- **It reports what it could not remove.** `remaining` is counted from the table
  *after* the deletes (`space-event-sweep.ts:87-88`), and the cron logs
  `space event sweep hit its per-run bound: N expired row(s) NOT removed`
  (`index.ts:286-291`). Reaching the bound is not "done", and a sweep that
  quietly stops at its bound reads exactly like one that finished.
- `deleted` is likewise counted from the table rather than read from
  `meta.changes` (`space-event-sweep.ts:78-83`), so a driver that does not
  report `changes` cannot make an inert sweep look successful.

**A row leaves storage in exactly two ways:** an acknowledgement (§6b.5), or
this sweep once `expires_at` has passed. There is no third.

**What is still lost, stated plainly:** an event that is never drained, or
drained and never acknowledged, is deleted at its `expires_at` — at most 7 days
after it was accepted (§6b.1). That is the honest limit of this lane's D15
compliance. The lane errs toward **duplicate delivery**: an unacknowledged event
becomes visible again once its lease lapses and is delivered again for as long
as its `expires_at` allows, and two devices sharing a tag can both receive it
(`space-events.ts:24-30`). Which way a lane errs is a property worth stating
explicitly, because both directions are defensible and only one is recoverable.

### 6b.7 What this lane may not adopt — DERIVED

[The Space delivery-tag finding](../docs/design/osl-spaces-delivery-tag.md), an
analysis contract owned jointly by T1 and T6, states a gate in its own words:
*"No Space fan-out or push-feed implementation may reuse the section 6b
group-shared derivation until T1 and T6 publish and test a recipient-isolated
derivation and lifecycle."* The prohibition is specific and follows from D-SEP:
if a recipient's tag is derived only from state every Space member holds, any
member can compute any other member's tag and observe when that peer receives a
message — the other members are inside the adversary model, not outside it.

Two consequences bind this document rather than merely informing it:

1. **This section may not supply a derivation**, and does not. The referenced
   finding assigns the choice to T1 and T6 and says in terms that T21 "must not
   select either direction, define its key lifecycle, or introduce a
   Space-specific delivery endpoint." OPEN-1 stays a question for that reason as
   well as for the evidentiary one.
2. **Whatever derivation is eventually chosen must show** that a member holding
   the Space group secret, but not another recipient's private derivation input,
   cannot compute that recipient's tag. Two directions are declared eligible —
   pairwise or recipient-device state, or a per-member subscription secret — and
   group-shared secret material alone is prohibited.

---

## OPEN — questions this lane could not answer, recorded rather than invented

Five questions were recorded on 2026-08-05. **Two are now answered by the code**
and are marked as such below with the source that answers them; they are kept
rather than deleted because DEFECTS.md cites them by number, and because a
question that was answered is a different artefact from one that was never
asked. **Three remain open and are left open.** One new question is added, which
did not exist before the mechanism that raises it.

**OPEN-1 — how is `recipient_tag` derived, and when does it rotate? STILL OPEN,
and now bounded on one side.**

`spaces.md` calls it a "rotating delivery tag", and **nothing in this repository
computes one.** That was re-checked rather than assumed: the Rust crates and
`apps/osl-hub` contain no occurrence of `recipient_tag` or `space-events` at
all, the hub's route inventory does not include `/v1/space-events`, and
`apps/osl-hub-ui` has no Space transport surface. **The only callers of this
lane anywhere in the repository are the worker's own tests**, and they use a
constant byte repeated 32 times. The 16-byte HKDF `derive_delivery_tag` in
`crates/crypto/src/pointer.rs` belongs to the cipher-store pointer lane and is
the near-miss described above, not this tag.

D-273 and D-274 **did not touch this and could not have**: both changed what the
relay does with a tag it is handed, and neither the drain, the ack, nor the
sweep computes a tag. The server has never derived one — it accepts 32 bytes and
routes by equality (`space-events.ts:112,127`) — so the derivation, wherever it
lands, is a client property, and there is no client.

What *is* newly recorded here is that the answer space is **constrained even
though the answer is absent**: §6b.7 above cites a T1/T6 analysis contract that
prohibits a group-shared-only derivation, names two eligible directions, and
gates any fan-out implementation on a published, tested, recipient-isolated
derivation. That narrows the question; it does not answer it, and it is not a
schedule. **Rotation in particular is unspecified by anything**: the only
time-bounded quantities on this lane are envelope-level (`expires_at`,
`MAX_EVENT_LIFETIME_SECONDS`, `LEASE_SECONDS`). There is no tag epoch, no tag
TTL, and no rekey trigger. `keyserver-cf/scripts/t21-space-contract.test.mjs`
asserts the *phrase* "rotating delivery tag" appears in `spaces.md`, which is
the entire enforcement of the word.

So "rotating" remains a claim rather than a property, and the security argument
for a bearer capability (§6b.4) — that a tag is short-lived — still cannot be
checked. **This is still the single largest gap in the lane** and is still why
§6b.4 is written as an observation rather than as a rule.

**OPEN-2 — the drain deletes on transmission, and D15 forbids that. ANSWERED in
the code, by D-273.** The question was whether this lane needed the
reservation-plus-ACK shape D15 implies, or an owner-recorded exemption saying why
membership events are exempt from a rule the message lane is held to. **It got
the shape.** The drain leases (`space-events.ts:135-139`) and
`POST /v1/space-events/ack` deletes (`space-events.ts:177-179`); the sibling
lane's own deferred end state — "a reservation window plus a
recipient-authenticated ACK" (`keyserver-cf/src/lib/db.ts:934-938`) — is now
built here first. Relabelled: the mechanism is OBSERVED at §6b.3 and §6b.5, and
the residual loss window is stated at §6b.6. **No exemption was recorded, and
none is needed.** The half this does not answer is the client's — see OPEN-6.

**OPEN-3 — expired envelopes are never deleted. ANSWERED in the code, by
D-274.** `expires_at` was a read filter only and nothing swept
`space_event_queue`. `keyserver-cf/src/lib/space-event-sweep.ts` now does, on
the hourly cron (`index.ts:281-294`), and migration `0043` — inside the block
`spaces.md` reserves for "expiry/index hardening" — was written. Relabelled:
OBSERVED at §6b.6. Two things the question did not ask, and which the answer had
to supply, are recorded there instead of here: the sweep is **bounded and says
what it could not remove**, and a sweep alone would not have been a retention
policy without the `expires_at` ceiling now in §6b.1.

**OPEN-4 — the drain's method is an owner decision (D-260). STILL OPEN.**
`spaces.md` T21-C1 freezes `GET /v1/space-events/:recipient_tag` as the route
that "returns and consumes". A `GET` that mutates what it returns is not safe or
idempotent, and the tag rides in the request path where intermediaries log it.
**D-273 did not relieve this.** The drain still writes on a `GET` — it leases
instead of deleting, which is a smaller write, not no write — and the tag is
still in the path. The D-260 lane ruled that it **should not** be a `GET` and
deliberately did not change it, because the method is frozen by a contract
outside that lane's ownership; the D-273 lane declined it for the same reason
and said so; this lane declines it for the same reason again. Recommended shape,
unchanged: `POST /v1/space-events/drain` with the tag in the body — which is
precisely the shape D-273 chose for the **new** ack route (§6b.5) the moment it
was free to choose. **Client impact in this repository is zero** — no caller
exists outside tests and documentation. **Three lanes have now declined this;
it is owed to an owner, not to another lane.**

**OPEN-5 — a full page is indistinguishable from the last page. STILL OPEN,
narrowed.** The drain returns at most 64 events (§6b.3) with no continuation
signal, so a caller that receives 64 cannot tell whether more remain. **What
changed:** because rows are leased rather than deleted, a second drain returns
the *next* 64 rather than the same 64, so a client can make progress by draining
repeatedly without acknowledging — and draining is no longer the destructive act
that made "re-drain to find out" unacceptable before. **What did not change:**
nothing tells a client to loop, and nothing in the response says whether more
remain. Whether the client rule is "drain until empty" or the response should
carry a `more` flag is still undecided.

**OPEN-6 (NEW) — nothing specifies what a client must do.** The lane's
correctness now depends on client behaviour that no document in this repository
requires. Three obligations are implied by the mechanism and stated nowhere:
(a) a client **must** acknowledge, or every event it receives is redelivered
each time its lease lapses, until `expires_at`; (b) a client **must** dedupe on
`event_id`, because the lane errs toward duplicate delivery by design (§6b.6);
(c) two devices sharing a tag will both receive an event, and nothing says which
of them is expected to acknowledge it, or what the other should do. The
endpoint's own header calls duplication "a client-side dedupe problem"
(`space-events.ts:27-28`), which names the obligation without imposing it.

This is recorded as a question rather than written as a rule for the same reason
the rest of this document exists: **no client of this lane exists yet**, so any
rule written here would be a guess about an unbuilt caller, and the right home
for a client obligation is a contract an owner has frozen rather than a
description of a server. It is the direct cost of answering OPEN-2 — the server
half of D15 is built, and the half that has to acknowledge is unspecified.
