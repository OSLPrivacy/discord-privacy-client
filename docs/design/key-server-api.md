# Design: Key server API

Status: **Draft.**

This is the API surface implemented in `discord-privacy-keyserver`.
v1 single-server; v2.2 fans out to 5 jurisdictions with `share_index`
per upload.

## Versioning

All endpoints prefixed `/v1/`. Wire format: JSON over HTTPS (over Tor
in v2.2). Key/blob fields base64url-encoded.

Schema versioned via `wrapped_share_blob_version` and
`bundle_format_version` fields in payloads, plus an `X-Schema-Version`
response header. Mismatch returns 4xx with explicit upgrade guidance.

## Current authentication boundary

The Cloudflare Worker separates operator authority from client traffic:

1. `OSL_KEYSERVER_ADMIN_TOKEN` is operator-only and currently gates the
   crypto-payment confirmation endpoint.
2. Public-client mutations never use a distributed bearer secret: an
   open-source desktop binary could not keep one confidential. They use
   canonical signatures from the registered identity instead.
3. Identity-sensitive operations use canonical Ed25519 signatures. This
   includes registration/rotation, prekey replenishment, consuming prekey
   and one-use wrapped-key GETs, control-inbox operations, unregister, and
   burn. D1 compare-and-swap/transactions bind verification to the key row
   used by the mutation and make consuming reads single-use.
4. Cloudflare native rate-limit bindings add abuse resistance. Cloudflare
   documents these counters as permissive/eventually consistent, so they
   are never treated as authorization or exact accounting.

Public-key and reusable-blob lookups remain public by design. A one-use
wrapped key requires a fresh signature from its intended recipient; a
prekey bundle requires a fresh signature from a registered requester.

**Unresolved release gate:** registration self-signatures prove possession
of the submitted key, but do not prove that the submitter owns the claimed
Discord snowflake. A first registrant can still preclaim another Discord
ID. Production onboarding therefore requires a Discord OAuth authorization
code flow (state + PKCE) whose `identify` result is checked by the server
before binding that snowflake. Until that is implemented, use only isolated
test identities and do not treat the build as safe for real accounts.

### Transport (client side)

The Rust client (`crates/keystore/src/client.rs`,
[`KeyServerClient`]) uses `reqwest` 0.12 blocking + rustls-tls
with the Mozilla `webpki-roots` CA bundle. Both `http://` and
`https://` `base_url`s are accepted; deployed instances use
HTTPS (Railway force-redirects HTTP→HTTPS at the edge), local
dev keyservers use plain HTTP. No system cert chain dependency
— the CA bundle is baked in at compile time.

Certificate pinning is deferred to v1-stable. Until then, the
trust anchor is the standard public-CA chain.

### v1 user_id semantics — Discord ID == OSL user_id

**v1 closed-beta choice (current):** the OSL `user_id`
registered with the keyserver IS the sender's Discord
`user.id`. This is a pragmatic decision that lets Phase 5's
receive hook resolve a sender's pubkey from
`message.author.id` (which Discord puts on every dispatcher
event) without an additional Discord-ID → OSL-UUID mapping
table.

**Privacy trade-off:** the keyserver now has visibility into
"which Discord user_ids have registered for OSL." For closed-
beta dogfood with a small known peer set this is acceptable —
the peers already know each other's Discord IDs by definition.

**v2 plan:** OSL `user_id`s become client-generated UUIDs, with
each peer maintaining a local Discord-ID → OSL-UUID mapping
table out-of-band (e.g. exchanged via QR code or signal-style
safety-number flow at first contact). The keyserver then sees
only opaque OSL UUIDs with no link to the Discord identity
graph. Receive-side decoder consults the local mapping table
to translate `message.author.id` to the OSL UUID before
calling `fetch_pubkeys`.

This isolation is not free — it adds first-contact friction
(peers must exchange UUIDs, not just Discord usernames) and a
new client-side state file. v1 ships the simpler model
deliberately, with the upgrade path documented here.

TLS terminates at Cloudflare and the Rust client trusts the standard
public-CA chain; certificate pinning remains deferred. Discord OAuth
ownership proof is the only known identity-binding gap described above.

## Endpoints

### `POST /v1/register`

Register a user and their public keys, **or re-register an existing
user** with fresh keys after a duress reinstall (see
[`unlock-and-duress.md`](unlock-and-duress.md)).

```json
Request:
{
  "user_id": "discord_user_id_or_pseudonym",
  "ik_x25519_pub": "...",
  "ik_ed25519_pub": "...",
  "ik_mlkem768_pub": "...",
  "ik_ratchet_initial_pub": "...",
  "registration_sig": "...",     // Ed25519(REG_MSG, submitted identity key)
  "rotation": null                // or old-key authorization for a key change
}
Response: 201 Created (initial)
        | 200 OK with key-rotation event recorded (re-registration)
```

**Discord OAuth proof of `user_id` ownership is required** before the
server accepts either initial registration or re-registration. Full
OAuth spec — flow, token verification, outage behaviour, bot vs user
accounts — in [`auth-flow.md`](auth-flow.md).

If the `user_id` already exists with prior keys, this becomes a
**re-registration**:

- Server overwrites the public-key record with the new values.
- Server records a key-rotation event and bumps `last_rotated_at`
  on the user's record.
- **Rate limit (resolved): 3 re-registrations per 24-hour rolling
  window, then exponential backoff** — 4th attempt within 24 h
  requires a 1 h delay, 5th requires 4 h, 6th and subsequent
  require 24 h each. Handles legitimate repeated-duress scenarios
  while preventing automation abuse. Belt-and-suspenders against
  abuse; the actual defence against malicious re-registration is
  the contact-side fingerprint verification described under
  `GET /v1/pubkeys/:user_id` below.

### `GET /v1/pubkeys/:user_id`

Fetch a user's current identity public keys without consuming any
prekey. Used to:

- **Detect key-rotation events** (compare `last_rotated_at` against
  client cache).
- **Display fingerprints** for out-of-band verification.
- **Group-membership pre-check** (does this contact have the app?).

```json
Response:
{
  "user_id": "...",
  "ik_x25519_pub": "...",
  "ik_mlkem768_pub": "...",
  "registered_at": "...",
  "last_rotated_at": "..."   // null on first registration; ISO 8601 on subsequent re-registrations
}
```

Returns 404 if the user has never registered.

**Key-rotation event semantics:**

When a client fetches and finds a `last_rotated_at` newer than its
cached value, this is a key-rotation event. The client treats the
contact's prior keys as no longer valid and surfaces the
verification UI:

```
⚠ Liam re-registered new keys on [date].
Their previous keys are no longer valid.

[Trust new keys]    [Verify fingerprint first]
```

Server **cannot enforce** verification — recipient-side decision;
server only surfaces the timestamp and lets the client compare. See
[`unlock-and-duress.md`](unlock-and-duress.md) for the end-to-end
flow.

Fingerprint format: human-readable formatting of
`SHA-256(ik_x25519_pub || ik_mlkem768_pub)`, in groups of 4
hex digits separated for readability (Signal-style safety numbers).

### `GET /v1/prekey-bundle/:user_id`

Fetch a prekey bundle. The caller supplies its registered requester ID,
the recipient ID, a fresh timestamp, and an Ed25519 signature over the
canonical tuple. After verification and an identity-key CAS, the server
pops one OPK atomically and records the request digest against replay.
See `prekey-infrastructure.md`.

### `POST /v1/prekey-bundle/replenish`

Authenticated upload of new OPK batch and optional new SPK.

### `POST /v1/wrapped-keys`

Upload a wrapped key blob.

```json
{
  "content_id": "uuid",
  "content_type": "text" | "attachment" | "system",
  "system_message_kind": null,            // present iff content_type == "system"
                                          // see "system_message_kind allow-list" below
  "sender_id": "...",
  "recipient_id": "...",
  "session_version": 1,
  "share_index": 0,                       // 0 in v1 single-server, 0..4 in v2.2
  "wrapped_share_blob": "...",
  "blob_version": 1,
  "single_use": false,
  "display_duration_seconds": null,       // present iff single_use
  "expires_at": "..."                     // server-side TTL
}
```

#### `system_message_kind` allow-list

Clients maintain an allow-list of valid `system_message_kind` values.
**Unknown kinds are rejected and not rendered**, ensuring forward
compatibility cannot be exploited to inject novel system-message
behaviour without a client update.

| `system_message_kind` | Purpose                              | Introduced |
| ---                   | ---                                  | ---        |
| `burn-alert`          | Sender opted into alerted burn       | v1         |

Future kinds require a client update to add to the allow-list.
Servers SHOULD reject unknown kinds at upload time as well, but the
authoritative check is client-side.

### `GET /v1/wrapped-keys/:content_id`

Fetch one wrapped key. Reusable records are public. A `single_use=true`
record requires a fresh canonical request signed by the registered key
of the row's intended recipient; the recipient identity is rechecked in
the consuming D1 transaction. v2.3 plans anonymous-credential auth and
v2.2 plans batched fetch with decoys (see `bucket-fetching.md`).

For `single_use=true`, the server atomically returns and deletes the blob
and stores a replay receipt in the same transaction. Subsequent use of
the same signed request is rejected; later fetches return `404 Not Found`.

**404 fallback semantics** (per content_type):

| `content_type` | Rendering on 404                                                |
| ---            | ---                                                              |
| `text`         | Render the original stego'd cover text from Discord as-is. **No "[deleted]" or "[unavailable]" marker.** Observer cannot distinguish burned from cover-text-only messages. |
| `attachment`   | Subtle `[attachment no longer available]` placeholder. Binary content cannot render as readable cover. |
| `system`       | `[notification no longer available]` placeholder. **NOT cover text** — system messages have a known semantic role and gibberish would be conspicuous. |

See `group-messaging.md` for the full text/attachment burn-rendering
rationale.

**410 Gone**: returned for content explicitly tombstoned after expiry
(distinct from 404 which means burned-or-never-existed). Client
treats 410 the same as 404 for rendering purposes.

### `DELETE /v1/wrapped-keys`

Burn endpoint.

```json
{
  "scope": "single" | "to_user" | "all",
  "target_user_id": "...",                // present iff scope == "to_user"
  "target_content_id": "..."              // present iff scope == "single"
}
```

Authenticated by the burning user's identity signature. v2.2: fans out
to all 5 servers; ack when ≥ 3 confirm.

### `POST /v1/sessions/:other_user_id/rotate`

Bump session version after a burn so future messages start a fresh
ratchet.

### Alert messages (silent vs alerted burn)

Burns are **silent by default**. When a sender opts into an alerted
burn (per-action choice; see `group-messaging.md`):

1. Sender's client sends a regular `POST /v1/wrapped-keys` upload
   containing an encrypted system message (e.g.
   `"@sender cleared message history with you on [date]"`), signed
   by sender's `IK_X25519`.
2. The system message uses reserved
   `content_type = "system"` and reserved
   `system_message_kind = "burn-alert"`.
3. Recipient's client renders system messages distinctly from user
   messages.

**No new endpoint required**; alert delivery reuses the wrapped-key
path. The server cannot synthesize alerts because it does not have
the sender's `IK_X25519` signing key — recipients verify the
signature before rendering.

Panic button and duress passphrase **never** generate alerts (silent
burn enforced client-side regardless of any prior preference).

### `POST /v1/tokens/issue` (v2.3+)

Issue a batch of blinded Privacy Pass tokens. See
`anonymous-credentials.md`.

## Re-validation

Recipient clients call `GET /v1/wrapped-keys/:content_id` to check
whether visible-viewport messages have been burned. The server
returns:

- `200 OK` with the blob (still present),
- `404 Not Found` (burned),
- `410 Gone` (explicitly tombstoned after expiry).

### Re-validation triggers

Re-validation fires on **any** of:

1. **5-minute timer** while the conversation is visible.
2. **Conversation becomes visible** from a previously-hidden state
   (panel scroll-into-view, tab change to that conversation, navigation
   from another channel). This catches the "showed someone an old
   conversation that I burned this morning" case where window focus
   never changed.
3. **User interaction** (input, scroll, click) within the visible
   conversation.

### Cache zero triggers

Clients **zero their local wrapped-key cache** on:

- next user interaction (input, focus change, scroll, click), AND
- a 5-minute timer regardless of interaction.

### Focus throttle

Re-validation may consume anonymous-credential tokens (v2.3+); to
avoid token-burn at idle, clients SHOULD throttle re-validation when
the window has been unfocused for 60+ seconds. Throttle pauses the
**5-minute timer** trigger only — visibility-change and user-
interaction triggers still fire (visibility change implies the user
just looked at the conversation). Resumes 5-minute timer on focus.

### v2 roadmap

Server-push burn-event channel for sub-second burn propagation to
online recipients, replacing the 5-minute polling cycle. Reduces
burn exposure window from ~5 min to sub-second for online
recipients.

## What the server sees

- Identity public keys (registered).
- `user_id` ↔ public key mappings (or pseudonym ↔ public key in
  pseudonymous mode).
- Wrapped-share blobs (opaque; cannot decrypt without ratchet state).
- Content IDs.
- Issuance counts per Discord ID (v2.3+); per-fetch identity obscured
  by Privacy Pass.
- Approximate timestamps.

## What the server cannot see

- Plaintext message bodies, file contents, or filenames.
- Ratchet chain state, message keys, session keys.
- Per-fetch identity in v2.3+.
- Group membership directly (sender uploads N wrapped keys per
  content; server cannot tell which recipients are in the same
  conversation without external inference).

## Selector resilience (cross-cuts: client-side only, included here for
discoverability)

The key server hosts a signed manifest file describing current Discord
webpack selector strategies. Client fetches at launch and hourly:

- Manifest signed by the release-signing key (multi-sig in v2.5).
- On manifest fetch failure, signature failure, or staleness > 24 h:
  encryption disabled, banner: "Discord update detected — please
  update Discord Privacy Client to vN.M.O" (link).
- Fail-closed: encryption is **off** until manifest validates.

### Manifest mirror (v1, single-server avoidance)

Even before v2.2 multi-jurisdiction key servers, the manifest is
mirrored to a **static CDN** (Cloudflare Pages or equivalent) signed
by the same release-signing key. Client fetch order:

1. Primary key server.
2. CDN mirror on primary failure.
3. Fail-closed if both unreachable or signatures invalid.

This eliminates the single-point-of-failure where the v1 single key
server going down would simultaneously disable encryption for the
entire user base. CDN hosts the manifest only — no wrapped keys, no
identity data, no anonymous-credential infrastructure. Static and
public; signature verification is the only authenticity check.

In v2.2, the manifest is additionally mirrored across the 5
jurisdictions; the CDN remains as a third tier.

## Open questions

- **Rate limiting.** Per-IP, per-user, per-token? With Tor + Privacy
  Pass, per-IP has limited meaning; per-token works (each token is
  single-use, so the cap is the issuance batch).
- **Storage backend.** Postgres with WAL replication across 5 servers?
  Or 5 independent Postgres instances with no cross-replication?
  **Decision: 5 independent. No cross-replication.** Each server holds
  only its own share; replication would defeat threshold-sharing.
- **Backup.** Per-jurisdiction encrypted backup, controlled by
  operator. v1 documented; revisit when independent operators
  recruited.
- **Manifest hosting redundancy.** Resolved (see "Manifest mirror"
  subsection above): v1 mirrors to a static CDN signed by the same
  release-signing key; v2.2 adds the 5-jurisdiction mirroring.
