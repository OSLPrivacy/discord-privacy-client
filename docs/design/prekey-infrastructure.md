# Design: Prekey infrastructure (Signal-style)

Status: **Draft.**

## Purpose

Enable asynchronous first contact: Alice can encrypt to Bob while Bob
is offline. Without prekeys, both parties must be online for the first
PQXDH exchange, which is incompatible with Discord usage patterns.

## Per-user prekey bundle

Stored on key server(s), tied to `user_id` (or pseudonym). Contains:

- **Identity public keys**, both:
  - `IK_X25519`: long-term X25519 identity public key.
  - `IK_MLKEM768`: long-term ML-KEM-768 public key (used in PQXDH
    encapsulation).
- **Signed prekey** `SPK`:
  - X25519 prekey rotated every 7 days (matches Signal default).
  - Detached signature over `SPK` by `IK_X25519`.
  - Rotation timestamp.
  - Previous `SPK` retained for **at least one rotation period** to
    accept in-flight messages from clients with stale bundles.
- **One-time prekey pool** `OPK_*`:
  - Pool of single-use X25519 prekeys.
  - Server pops one per fetch, atomically.
  - Client replenishes the pool when it falls below a threshold.
  - **v1 fixed thresholds: pool target 100, replenish trigger 25.**
    Adaptive sizing based on observed consumption deferred to v2 if
    real-world data demands.
- **PQ one-time prekey pool**: not in v1 — identity-only ML-KEM
  coverage. See "Resolved decisions" Q1 below.

## Bundle fetch

`GET /v1/prekey-bundle/:user_id` (anonymous-credential authenticated in
v2.3) returns:

```json
{
  "user_id": "...",
  "ik_x25519": "...",
  "ik_mlkem768": "...",
  "spk": "...",
  "spk_signature": "...",
  "spk_rotated_at": "...",
  "opk": "..."          // null if pool exhausted
}
```

The server pops the OPK atomically; double-fetches return different
OPKs.

## Replenishment

`POST /v1/prekey-bundle/replenish` from authenticated client uploads:

- New OPK batch (signed by `IK_X25519` over the batch hash; binds the
  batch to the identity even when the request body is anonymized in
  v2.3).
- Optional new `SPK` if the previous is older than 7 days.

Client logic:

- On launch, check OPK pool size. Replenish if below threshold.
- Background timer rotates `SPK` weekly.

## Failure modes

- **OPK pool exhausted.** Server returns `opk: null`. Sender's PQXDH
  proceeds without an OPK (DH4 omitted). Documented in
  `pqxdh-double-ratchet.md` "OPK exhaustion fallback" subsection.
- **Stale SPK.** Server retains the previous `SPK` for one rotation
  period. Beyond that, the message is rejected; sender re-fetches the
  bundle and retries. Defense against indefinite ciphertext-replay
  across rotations.
- **Replay of `(IK_A, EK_A, OPK_B)` triple.** OPK is consumed atomically
  on first fetch; re-fetch returns a different OPK. Recipient detects
  the duplicate via OPK identifier and rejects. Document the exact
  rule.
- **Server forced to issue stale or attacker-chosen prekeys.** Threshold
  sharing (v2.2) reduces single-server compulsion. Beyond v2.2, a
  transparency log of prekey bundles considered for v3+.

## Re-registration on reinstall (post-duress)

After a duress wipe (see [`unlock-and-duress.md`](unlock-and-duress.md))
all local prekey state is destroyed:

- Long-term identity X25519 + ML-KEM-768 private keys (TPM-evicted).
- Signed prekey + signature.
- Local copy of OPK pool.
- Cached prekey bundles fetched from other users.

Reinstallation generates fresh identity keys, fresh signed prekey,
and a fresh OPK pool. The user re-registers via `POST /v1/register`
with the new public keys. The key server marks the `user_id` as
having undergone a key-rotation event and bumps its `last_rotated_at`
timestamp. Contacts of this user, on their next
`GET /v1/pubkeys/:user_id` (or implicit lookup at next message
attempt), see the new keys, treat this as a key-rotation event, and
surface the verification UI described in
`unlock-and-duress.md` and `key-server-api.md`.

The contact's choice to trust new keys (or verify by fingerprint
first) is a recipient-side decision; the server cannot enforce it.
This is the standard MITM-prevention pattern (Signal safety
numbers).

## Resolved decisions

1. **PQ one-time prekeys**: identity-only ML-KEM in v1 (vanilla PQXDH
   spec). PQ-OPK extension deferred to v2 — see THREAT_MODEL.md
   v2 roadmap. Trade-off: improves PQ post-compromise of the long-
   term ML-KEM key but adds ~1 KB per OPK storage cost.
2. **Prekey upload authentication**: signed batch upload by
   `IK_X25519`, identity-bound. Anonymous credentials (v2.3) do not
   apply to replenishment.
3. **SPK rotation cadence**: 7 days, matching Signal default. No
   deviation for marginal load reduction.
4. **OPK pool sizing**: fixed pool target 100, replenish trigger 25
   in v1. Adaptive sizing based on observed consumption rate
   deferred to v2 if real-world data demands.
5. **Cross-device**: single device per identity in v1. Sesame-style
   multi-device deferred to v2+.

## Review gate

- [ ] Cryptographer review of prekey lifecycle (covered by the PQXDH
      review engagement).
- [ ] Replay protection test: send same message twice with same OPK
      identifier; recipient rejects.
- [ ] Replenishment under network partition: client replenishes
      reliably after offline period.

## References

- Kret & Kotov (2023), §3.
- libsignal-protocol prekey lifecycle.
