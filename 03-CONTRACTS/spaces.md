# Enclaves role contract

**Status:** frozen by T21-F1. This contract defines the v1 Enclaves role
model. It uses the owner’s final product name, **Enclaves** (D84).

## Space event transport (T21-C1)

Membership propagation is a ciphertext-only, rotating-tag lane.  Its envelope
and capability rules are those of [`transport.md`](transport.md) §6b; this
contract only assigns the Space-specific route names and fields.  In
particular, it does not introduce an account-addressed control inbox.

`POST /v1/space-events` accepts `{ recipient_tag, ciphertext, expires_at }`.
`recipient_tag` is a 32-byte opaque rotating delivery tag encoded as base64;
it is never an account identifier or a Space identifier.  `ciphertext` is an
opaque encrypted membership event.  `POST /v1/space-events/drain` accepts
`{ recipient_tag }` and returns and consumes only records addressed to that
exact current tag.  The recipient tag is a bearer capability and therefore
never appears in a request path on this lane (D81).  The
relay stores no roster, plaintext event, membership count, or account/Space
mapping.  A sender chooses one envelope per recipient tag.

**Amendment, 2026-08-05 — owner ruling on D-260 / OPEN-4, the only change made
to this frozen clause.** The drain was frozen above as
`GET /v1/space-events/:recipient_tag`. That one sentence, and nothing else in
T21-C1, now reads as `POST /v1/space-events/drain` with the tag in the body,
and the D81 sentence above it is new. The `GET` is **removed**, not deprecated:
no caller of this lane exists outside tests and contracts, so there was no
transition to keep it alive for, and a reachable route that writes under a
method every proxy, prefetcher and retry treats as safe and repeatable retains
the whole hazard for nobody. Everything else T21-C1 says is unchanged and
unmoved — the envelope, the 32-byte opaque rotating tag, "returns and consumes",
the relay's stored-nothing rule, one envelope per recipient tag, and the
`0041`–`0043` migration reservation. The drain's own behaviour is likewise
unchanged: same lease, same 64-row page, same `POST /v1/space-events/ack`, same
expiry sweep, and the same indistinguishable rejection that keeps the lane from
being an existence oracle. See `transport.md` §6b.3.

The keyserver migration numbers `0041` through `0043` are reserved for this
lane: event queue, invite capability state, and expiry/index hardening.  No
unrelated schema may use that block.

**Amendment, 2026-08-06 - invite record numbering.** `0042` remains a recorded
skip in the keyserver migration-sequence gate because another local lane already
used that number. The one-use invite record therefore lands as
`0044_one_use_invite_links.sql`: it defines the creator, singular intended use,
expiry, and consumed state for invite links without taking the reserved `0042`
name.

## Boundary

Roles grant governance capability, never visibility. An Enclave role does not
contain a channel identifier, a channel key, a recovery key, or an owner key.
There is no role that reads a channel without being a member of that channel.
Channel membership and possession of the channel's current keys are the sole
basis for reading its future content; they are modeled separately from a role.

An administrator who needs to read a channel joins it under the same rules as
every other member. No role grants historical content: admitting or promoting
a member provides future keys only after rotation. The relay receives neither
the roster nor channel keys.

Role actions are governance requests. Their enforcement classification and
channel-membership/key checks are intentionally owned by T21-F2 and T21-F3,
respectively; this contract does not label a client-side role check as
cryptographically enforced.

## Fixed v1 roles

The complete role set is `member`, `moderator`, and `admin`. Custom roles are
not supported in v1. The permitted governance requests are deliberately small:

| Role | Governance requests | Does the role grant channel visibility? |
| --- | --- | --- |
| `member` | none | No |
| `moderator` | moderate members | No |
| `admin` | moderate members; manage roles; manage channels | No |

The machine-readable contract is the test surface for T21-T31:

```space-role-contract
{
  "roles": [
    { "name": "member", "governance_capabilities": [] },
    { "name": "moderator", "governance_capabilities": ["moderate_members"] },
    { "name": "admin", "governance_capabilities": ["moderate_members", "manage_roles", "manage_channels"] }
  ],
  "custom_roles": false,
  "visibility": {
    "basis": "channel_membership_and_current_key_possession",
    "role_grants_channel_visibility": false,
    "role_grants_history": false,
    "owner_or_recovery_key_exists": false
  }
}
```

## Channels

A channel is an explicit replicated declaration with an opaque channel id, a
`kind`, and a `position`.  `kind` is never inferred from the display name;
renaming a channel therefore cannot change its delivery or key-domain
semantics. `position` is the shared ordering value replicated through the
Space event log, so clients do not sort channels by last activity.

The v1 channel model is flat: categories and category membership are not
represented. `voice` has an explicit kind for convergence, but Voice is not a
v1 delivery feature and must be shown as unavailable until its separate gate
has live proof. A channel membership and its current channel keys, not a role,
are the basis for decrypting that channel's future content.
