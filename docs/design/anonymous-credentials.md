# Design: Anonymous credentials (v2.3 plan)

Status: **Draft (v2.3 plan). Highest research-grade risk in the roadmap.**

## Goal

Replace `user_id`-based key-server authentication with unlinkable
tokens. The server learns "an authorized user fetched content X at
time Y," not "user U fetched content X at time Y." Defeats single-
server-side correlation between Discord identity and fetch activity.

## Approach (v2.3 v1 of this feature)

**Privacy Pass tokens with batched issuance.** RFC 9576 / IETF Privacy
Pass architecture; per-token issuance via VOPRF.

- **Issuance**: client obtains a batch of `T` blinded tokens (default
  100) by authenticating with Discord identity (one-shot per batch).
  Server issues without learning unblinded tokens.
- **Spend**: each `/v1/wrapped-keys/:content_id` and
  `/v1/prekey-bundle/:user_id` fetch consumes one unblinded token. The
  server verifies the token signature and records the unblinded
  identifier as spent (single-use).
- **Refill**: client triggers issuance when batch falls below threshold
  (default 25).

Privacy Pass is designed primarily for rate-limit / abuse mitigation,
not arbitrary per-fetch authentication. We adapt it here because:

- Mature Rust libraries exist (`voprf` from RustCrypto).
- Reasonably audited construction.
- Acceptable performance at our scale.

Limitations:

- Server learns the **batch issuance pattern** (which Discord IDs
  request many tokens). Threshold sharing (v2.2) reduces this leakage
  to per-jurisdiction.
- **No selective disclosure.** Privacy Pass tokens are pure unlinkable
  signatures; you cannot prove "I'm allowed to fetch X" without
  revealing the token.

## Why not BBS+, ARC, zk-creds?

These give richer unlinkability and selective disclosure but the Rust
ecosystem is thin and audit posture is weak in 2026. Document in v3+
roadmap if the threat model evolves.

## Wire protocol

```
POST /v1/tokens/issue
  Authorization: <Discord identity proof>
  Body: { blinded_tokens: [b1, b2, ...] }
  Response: { signed_blinded_tokens: [s1, s2, ...] }

GET /v1/wrapped-keys/:content_id
  Authorization: PrivacyPass <unblinded_token> <signature>
  Response: { wrapped_share_blob: ... }
```

Spent tokens stored server-side in a fast-lookup spent-set (Bloom
filter + Postgres confirmation). Tombstone after 7 d (token expiry +
margin).

## Failure modes

- **Token batch exhausted mid-session.** Client refills in background;
  if depleted before refill completes, falls back to identity-bound
  auth temporarily. Degrades unlinkability for that period; surfaced
  in UI.
- **Server-side spent-set lost or rolled back.** Replay of spent tokens
  becomes possible. Mitigation: spent-set replicated within a
  jurisdiction; tokens carry expiry timestamps to bound the window.
- **Issuance correlation.** A server seeing many issuance requests from
  one Discord ID can correlate volume; defeated only if issuance is
  also routed via Tor (it is — see `transport/`).

## Open questions

1. **Privacy Pass library**: `voprf` from RustCrypto is the most mature.
   Confirm it implements the exact construction we need (batched issuance,
   single-use redemption) before committing.
2. **Token expiry**. Long-lived tokens leak less issuance signal but
   complicate revocation. Default 30 d; revisit.
3. **Group fan-out interaction.** A sender uploading to N recipients
   spends N tokens (one per upload). Burns through tokens at high group
   size. Adjust batch size for heavy senders.
4. **Threshold-share interaction.** Each fetch hits 5 servers; does
   this consume 1 token or 5? **Decision: 1 token, presented to all 5.**
   Servers do not need to coordinate spent-set; cross-server replay only
   matters within a single jurisdiction.
5. **Discord identity proof for issuance.** What does the client present?
   Options: signed challenge by `IK_X25519` (binds to identity at
   registration), one-time bearer via Discord OAuth, or a pseudonym
   nonce. Default: signed challenge.

## Review gate

- [ ] Cryptographer review of Privacy Pass spend protocol as adapted
      for per-fetch authentication.
- [ ] Library audit confirmation (`voprf` or chosen alternative).
- [ ] Threat-model entry: what does the server learn at issuance vs at
      spend? Quantify.
- [ ] Operational runbook for spent-set divergence.
