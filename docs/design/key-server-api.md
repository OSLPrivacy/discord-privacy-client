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
  "ik_mlkem768_pub": "...",
  "ik_x25519_signature": "..."   // self-signature; binds user_id to keys
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

Fetch a prekey bundle. Server pops one OPK atomically. See
`prekey-infrastructure.md`.

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

Fetch one wrapped key. v2.3+: anonymous-credential auth. Supports
batched fetch with decoys (see `bucket-fetching.md` in v2.2).

`single_use=true` records: server atomically returns the blob and
deletes the record in the same transaction. Subsequent fetches return
`404 Not Found`.

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
