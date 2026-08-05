# Opaque-envelope transport contract

**Status:** NOT frozen. Written 2026-08-05 under W3-8 (defect D-272) to end a
dangling deferral, not to settle a design.

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
   **32 bytes, base64** (`space-events.ts:10`, `TAG_BYTES = 32`). Different
   length, different encoding, different lane.

The rules were never written, and the nearest same-named section would not have
supplied them.

---

## 6b. Opaque-envelope lanes — envelope and capability rules

This section is numbered `6b` because `spaces.md` is frozen and cites that
number. The numbering gap above it is not a set of reserved placeholders; there
are no other sections, and this label exists to make an existing frozen citation
resolve rather than to imply a hidden `§1`–`§6a`.

Scope: the space-event lane, `POST /v1/space-events` and the drain, implemented
in `keyserver-cf/src/endpoints/space-events.ts` over the `space_event_queue`
table (`migrations/0041_space_event_queue_reserved.sql`).

### 6b.1 The envelope — OBSERVED

`POST /v1/space-events` accepts a JSON object. Exactly three fields are read
(`space-events.ts:35-37`); any other field is ignored, not rejected.

| field | rule | source |
|---|---|---|
| `recipient_tag` | base64 that decodes to **exactly 32 bytes** | `space-events.ts:10,44` |
| `ciphertext` | base64 that decodes to **at least 1 and at most 65,536 bytes** | `space-events.ts:11,44` |
| `expires_at` | a **safe integer**, Unix seconds, **strictly greater than** the accept-time clock | `space-events.ts:44` |

- **Accept** is `202` with `{"accepted": true}` (`space-events.ts:50`).
- **Every** envelope violation returns the **same** `400` with the **same**
  message, `invalid opaque Space event envelope` (`space-events.ts:45`). A
  caller cannot tell a bad tag length from a bad ciphertext length from a stale
  `expires_at`. This is a single indistinguishable rejection and must stay one:
  a per-field error message is a probing aid on a route whose only secret is the
  shape of what a caller already holds.

### 6b.2 What the relay may store — OBSERVED, and load-bearing

The stored row is exactly five columns
(`0041_space_event_queue_reserved.sql:3-9`): a 16-byte random `id`, the
`recipient_tag`, the `ciphertext`, `expires_at`, and `created_at`.

**No account identifier, roster, Space identifier, event kind, or sender
attribute may be added to this table.** The implementation's own header states
the reason (`space-events.ts:3-5`): each such column lets the relay enumerate a
Space rather than merely relay ciphertext. `spaces.md` states the same
prohibition from the other side. This is the one rule in this section that is
already enforced in two places and is not in question.

### 6b.3 The drain — OBSERVED

- The tag in the request must decode to exactly 32 bytes, else `400`
  `invalid recipient tag` (`space-events.ts:56`).
- The read selects rows for that **exact** tag with `expires_at > now`, ordered
  by `created_at`, **limited to 64** (`MAX_DRAIN`, `space-events.ts:12,59-60`).
- Every row returned is then deleted (`space-events.ts:62`).
- The response is `{"events": [{"ciphertext": …}]}` (`space-events.ts:63`).

**No existence oracle** — a well-formed tag with nothing queued and a well-formed
tag that was never used both return `200` with an empty `events` array. Only a
**malformed** tag is distinguishable, which reveals nothing about any tag's
existence. This property is currently held and must be preserved.

### 6b.4 The capability — OBSERVED

**Possession of the current 32-byte `recipient_tag` is the entire capability.**
There is no authentication, no signature, no bearer token, and no capability
check on either route. The drain reads and destroys on the strength of the tag
alone.

After D-260, both routes sit behind the worker's ingress rate limits — the
mutation limit on the `POST`, the public-GET limit on the drain. Those bound the
*cost* of probing the tag space; they are not an authorization check and must
not be described as one.

**There is no invite capability in this repository.** `spaces.md` reserved
migrations `0041`–`0043` for "event queue, invite capability state, and
expiry/index hardening". Only `0041` was ever written. `0042` — the invite
**capability** state — does not exist. So the "capability rules" this lane was
told it inherited have, today, exactly one rule: the tag is a bearer capability.

---

## OPEN — questions this lane could not answer, recorded rather than invented

**OPEN-1 — how is `recipient_tag` derived, and when does it rotate?**
`spaces.md` calls it a "rotating delivery tag", and nothing in this repository
says how it is computed or on what schedule it changes. Until that is written,
"rotating" is a claim rather than a property, and the security argument for a
bearer capability (§6b.4) — that a tag is short-lived — cannot be checked. The
same-named section in the private plan repo does **not** supply this (see the
near-miss above: it derives a different tag of a different size). **This is the
single largest gap in the lane** and is the reason §6b.4 is written as an
observation rather than as a rule.

**OPEN-2 — the drain deletes on transmission, and D15 forbids that.**
Owner decision **D15 — "delete on ACKNOWLEDGED receipt, never on
transmission"** — is live and enforced in this same worker for the wrapped-key
lane (`keyserver-cf/test/integration/wrapped-key-reservation.test.ts:1-20`;
`apps/osl-hub/tests/sealed_relay_e2e.rs:78-80`). That lane was specifically
changed because destroying a row as it was handed to the socket meant a lost
HTTP response destroyed the only copy.

`handleSpaceEventDrain` does exactly what D15 forbids: it deletes each row
(`space-events.ts:62`) before the response is serialized. A dropped response
silently and unrecoverably destroys membership events the recipient never saw.
Either this lane needs the reservation-plus-ACK shape D15 implies, or an owner
must record why membership events are exempt from a rule the message lane is
held to. **Not decided here.**

**OPEN-3 — expired envelopes are never deleted.**
`expires_at` is a **read filter only** (`space-events.ts:59`). The worker's
`scheduled()` handler sweeps subscriptions, crypto invoices, Stripe checkout
claims, payment alerts, privacy rows, and control-inbox rows — `index.ts` names
each — and **nothing sweeps `space_event_queue`**; a repository-wide search for
that table finds only the two handlers and the migration. Migration `0043`,
reserved for "expiry/index hardening", was never written. Expired ciphertext is
therefore retained indefinitely, which is a retention property no document
states and no test checks.

**OPEN-4 — the drain's method is an owner decision (D-260).**
`spaces.md` T21-C1 freezes `GET /v1/space-events/:recipient_tag` as the route
that "returns and consumes". A GET that destroys what it returns is not safe or
idempotent, and the tag rides in the request path where intermediaries log it.
The D-260 lane ruled that it **should not** be a GET and deliberately did not
change it, because the method is frozen by a contract outside that lane's
ownership. Recommended shape: `POST /v1/space-events/drain` with the tag in the
body. **Client impact in this repository is zero** — no caller exists outside
documentation. This document does not change the method either; it records the
decision as still open and still owed.

**OPEN-5 — a full page is indistinguishable from the last page.**
The drain returns at most 64 events (§6b.3) with no continuation signal. A
caller that receives 64 cannot tell whether more remain. Because rows are
deleted as they are returned, re-draining is the only way to find out, and
nothing in `spaces.md` or here tells a client to loop. Whether the client rule
is "drain until empty" or the response should carry a `more` flag is not
decided.
