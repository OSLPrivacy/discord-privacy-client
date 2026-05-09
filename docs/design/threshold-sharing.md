# Design: Threshold secret sharing (v2.2 plan)

Status: **Draft (v2.2 plan).**

## Goal

Replace single-server wrapped-key storage with K-of-N Shamir's Secret
Sharing across 5 servers in 5 jurisdictions. Decryption requires 3-of-5
shares; burn must reach ≥ 3 servers to succeed. Designed so that
compulsion or compromise of fewer than 3 servers cannot decrypt content,
and compulsion or compromise of fewer than 3 servers cannot prevent
burn.

## Parameters

- N = 5, K = 3.
- Library: `shamirsecretsharing` Rust crate (or audited equivalent;
  `vsss-rs` is an alternative offering verifiable secret sharing — see
  open question 1). Use a library that does not roll its own
  primitives.

## Jurisdictions (v2.2 target)

Sweden, Switzerland, Iceland, Panama, Romania. Privacy-friendly hosts:
Njalla, 1984 Hosting, FlokiNET, Orange Website, BuyVM. Selection
criteria:

- Non-MLAT-cooperating with each other to the maximum extent feasible
  in 2026.
- Operator (host) with a documented record of resisting subpoenas.
- Independent legal entities (no shared parent company).

## Operator model (v2.2)

**Solo operator (you), documented honestly.** All 5 servers operated by
the same person, ideally under different legal entities. Meaningful
protection against:

- External attackers compromising one server.
- Compulsion of one jurisdiction (the others retain the share).

Weak protection against:

- Legal compulsion targeting the operator across all 5 jurisdictions
  simultaneously (e.g. a single warrant in the operator's home
  jurisdiction reaching all server credentials at once).

> **Honest framing in user-facing docs**: "Five servers in five
> countries, currently one operator. Plan to recruit independent
> co-operators as the project matures."

## Wire protocol

The existing `/v1/wrapped-keys` endpoints take a `share_index` field on
upload. The sender splits the wrapped-key blob into 5 shares
client-side and uploads each to a different server. Fetch is parallel
across all 5; the first 3 to respond unlock the wrapped key. Burn fans
out to all 5 in parallel and reports success when ≥ 3 ack.

## Failure modes

- **Server unreachable on fetch.** Per-server timeout 2 s; require 3
  successes within 5 s total. Fall back to error if not met. Surface
  "key unavailable, retry" UI.
- **Server unreachable on burn.** Retry with exponential backoff for 24
  h. Surface "burn pending" UI status until all 5 ack.
- **Server returns malformed share.** Detect via Shamir reconstruction
  with cross-check; flag the misbehaving server in an internal audit
  log. Use any 3 valid shares from the remaining 4.
- **Operator compromise of all 5.** Acknowledged limitation; mitigated
  only by recruiting independent operators (not in v1 or v2.2).

## Open questions

1. **Verifiable secret sharing (VSS)?** `vsss-rs` provides Feldman /
   Pedersen VSS with zero-knowledge proofs of share validity, defeating
   malicious-server share-substitution attacks. Adds complexity and
   dependency weight. Defer unless threat model demands.
2. **Share refresh / proactive secret sharing.** Periodically rotate
   shares without revealing the secret, defeating long-term passive
   compromise. Defer to v3+.
3. **Onion routing per server.** Each server has its own `.onion`
   address. Tor circuits to each are independent (see `transport/`).
4. **Server-side rate limiting and decoys.** Bucket fetching with
   decoys (`bucket-fetching.md`, also v2.2) interacts with threshold
   fetching: each fetch triggers 5 server requests, plus decoys.
5. **Jurisdiction independence test.** Are these 5 jurisdictions
   actually MLAT-independent in 2026? Verify each before launch.

## Review gate

- [ ] Library choice: `shamirsecretsharing` vs `vsss-rs` vs alternative.
- [ ] Cryptographer sign-off on Shamir parameters for the secret size.
- [ ] Operator legal review of 5-jurisdiction setup.
- [ ] Operational runbook for share-divergence incidents.
