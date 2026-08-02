# OSL-RN pinned-peer recovery

**Status:** design only. Owner decision D78 (2026-07-31) authorizes an
explicit, both-sides-confirmed, out-of-band unpin ceremony. It does not
authorize an automatic fallback, a local-only reset, or an implementation in
this change.

## Problem and boundary

A peer that has successfully authenticated OSL-RN is pinned to it. If that
peer is later rolled back while retaining the same identity key, its pinned
counterpart receives `PinnedToRn` rather than silently selecting v3. The same
refusal is correct when a network attacker strips capability evidence: the
client cannot distinguish that attack from a rollback.

The ceremony is an intentional exception to this monotone floor. It restores
availability only when **both holders of the two identity keys knowingly
authorize the exact same relationship to be lowered**. It is not recovery for
a desynchronised ratchet: `SESSION_RESET` and re-handshaking retain the pin.
It is not a way to upgrade an existing v3 conversation in place.

## Contract test vectors

```json
{
  "version": 1,
  "pinLowering": "explicit_both_sides_confirmed_out_of_band_unpin_only",
  "requires": [
    "same-relationship-binding",
    "two-identity-signatures",
    "independent-local-confirmations",
    "out-of-band-comparison",
    "unexpired-single-use-request"
  ],
  "forbids": [
    "automatic-expiry",
    "local-only-unpin",
    "network-only-confirmation",
    "session-delete-lowers-pin",
    "v3-retry-of-pending-rn-wire"
  ],
  "postApply": {
    "deleteRnSession": true,
    "clearPin": true,
    "preserveQueuedRnPlaintext": true,
    "requiresFreshBootstrapForRn": true
  }
}
```

## Ceremony

1. A client may offer **Request mutual unpin** only after surfacing
   `PinnedToRn`. It creates a durable, single-use request containing a protocol
   version, a 256-bit random request ID, both long-term identity public-key
   fingerprints in canonical order, a relationship binding derived from that
   ordered pair, its creation and short expiry time, and the current pin state.
   The requesting identity signs this exact record. Creating or displaying it
   changes neither the pin nor the session.
2. The requester transfers that signed record to the other person by a channel
   independent of OSL delivery (for example, scanning a QR code in person or
   comparing it on a known-safe call). The app must display both identities,
   the request ID/check words, expiry, and the consequence: future sends may
   use legacy v3 if policy permits. A copied chat message, a carrier receipt,
   or a server response is not out-of-band confirmation.
3. The recipient verifies the requester signature, that the two displayed
   identities and relationship binding are theirs, that the request is fresh
   and unused, and compares the request ID/check words with the requester
   outside OSL. The recipient then makes an independent local confirmation and
   signs an approval over the hash of the whole request. The approval carries
   the recipient identity fingerprint and the same expiry; it grants no
   authority for another request or relationship.
4. Each client applies the unpin only after it has locally verified both
   signatures, the same canonical relationship binding, the match between the
   confirmation it displays and the out-of-band comparison, freshness, and a
   durable unused-request check. A party that has only its own signature stays
   pinned. The second signed record is also exchanged out of band; no OSL wire
   path may silently complete the ceremony.
5. Applying is one durable transaction: mark the request ID consumed, clear
   the RN pin, and delete the RN session material. An interruption must leave
   either the old pinned state or a completed, replay-protected unpin—not a
   partially cleared state. The UI records that this was a mutual security
   downgrade and directs both people to verify capabilities before continuing.

The request binds public identity keys rather than mutable account handles,
display names, devices, or capability advertisements. A new peer identity is
a different relationship and is not accepted as an approval for the old one.
Every accepting client must support this ceremony. A peer running a build too
old to create the matching signed approval must first update to a
ceremony-capable build or establish a new identity; the pinned side must not
offer a local-only workaround.

## After a successful unpin

The completed ceremony intentionally permits normal policy selection again.
That can select v3 for a peer that no longer has verified live RN capability;
this is the explicit security trade both people accepted. RN can return only
through a fresh authenticated bootstrap, which raises a new pin. Existing v3
history remains v3 and in-flight v3 remains decryptable.

Queued RN wires are never retried as v3. Their sealed plaintext stays in the
local outbox until the user either re-establishes RN and re-seals it under the
new session, or explicitly discards it with an honest unsent-message warning.
The unpin transaction must not silently send, discard, or reinterpret it.

## Non-goals and rejection cases

- No timer, retry count, transport failure, capability loss, session deletion,
  recovery control packet, reinstall, or account rollback lowers a pin.
- A single device/person confirmation does not lower a pin, even under a
  claimed emergency or rollback condition.
- An attacker who relays, replays, substitutes, delays, or suppresses OSL
  traffic cannot create the second identity signature or complete the required
  human out-of-band comparison. They can still cause denial of service; this
  design does not claim to prevent that.
- A stale, consumed, malformed, wrong-version, wrong-relationship, expired,
  or signature-invalid request/approval is rejected fail closed and leaves the
  pin unchanged.

## Implementation gate

Before implementation, define the canonical serialization, relationship
binding, signature domain separation, request lifetime, durable transaction
format, local audit retention, and UI copy in a reviewed protocol change.
Tests must cover signature substitution, cross-relationship replay, stale and
double-use requests, one-sided confirmation, crash recovery at every write
boundary, and the invariant that neither queued RN traffic nor any automatic
event falls back to v3.
