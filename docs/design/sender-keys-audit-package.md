# Sender-keys audit package

Status: **custom construction; unaudited; not a stable-security claim.** This
package is the review input for an independent cryptographer. It is a map of
the implementation and its known near misses, not a substitute for an audit.

## Review target

The implementation is [`crates/crypto/src/sender_keys.rs`](../../crates/crypto/src/sender_keys.rs).
The higher-level send/receive and rotation integration is in
[`crates/ipc/src/group_send.rs`](../../crates/ipc/src/group_send.rs) and
[`crates/runtime/src/rotation.rs`](../../crates/runtime/src/rotation.rs).
The intended design and its open questions are in
[`sender-keys.md`](sender-keys.md), particularly its construction and
FS-composition discussion.

Each sender maintains a per-group chain. A fresh, 32-byte `RotationRoot` and
`chain_id` derive `CK_0`; each send derives independent message and header
keys from `CK_n`, then advances one HKDF step. The wire has an encrypted
header (physical device id, chain id, counters, and session version) and an
XChaCha20-Poly1305 payload authenticated with canonical sender/group AD.
Receiver forward search is bounded and its skipped-key cache permits
out-of-order messages.

## Deviations from libsignal sender keys

- Headers are encrypted with a per-message header key; standard libsignal
  sender keys do not use this encrypted-header construction.
- `RotationRoot` is distinct from the chain key and seeds every new chain.
- Message and header keys are separately derived for every message, so the
  receiver performs bounded forward search instead of the usual simpler
  sender-key receive path.
- `(chain_id, RotationRoot)` distribution is performed by higher-layer SKDM
  transport, rather than being intrinsic to this primitive.
- The production distribution channel currently uses stateless v=3 bundles.
  A compromise of its long-term key can expose future SKDMs; sender-chain
  rotation does not provide post-compromise security for that channel. This is
  owned by T19, not solved here.

## Deterministic derivation vectors

These vectors use HKDF-SHA256, 32-byte output, and the exact labels in
`sender_keys.rs`. For an empty salt, HKDF uses RFC 5869's all-zero SHA-256
salt. They cover the chain-init, chain-step, message-key, and header-key
derivations without relying on random nonces or ciphertexts.

| input/output | hex |
| --- | --- |
| `RotationRoot` | `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| `chain_id` input, little-endian (`0x01020304`) | `04030201` |
| `CK_0 = HKDF(root, chain_id_le, "sender-keys/chain-init")` | `2c75ef7492d5d18ca7b44bc7efaebad18e5324538704a92fe2e9dd77bc63de16` |
| `CK_1 = HKDF(zeros, CK_0, "sender-keys/chain-step")` | `271a2293c26e70fde66c3bb730c8ca8a782e1b03cc1e7a91b90b80a4c34dfea0` |
| `MK_0 = HKDF(zeros, CK_0, "sender-keys/msg-key")` | `7ed7bba7b28374ef18936e6a09b8c4b62483f2cafbf052b2814acea39c2a59d0` |
| `HK_0 = HKDF(zeros, CK_0, "sender-keys/header-key")` | `92918a0a6f01f1e51739a522696f8c4f7dde1ad7b77cd753855c958aafad4556` |

An auditor should independently reproduce these values before reviewing the
AEAD framing, canonical AD encoding, cache commit rules, and zeroization.

## Defects and near misses B1–B5

1. **B1 — replay/rewind rotation.** `ReceiverChain::rotate_to` now rejects
   older ids and accepts the current id as a no-op, including when it carries a
   different root. This preserves the five-minute SKDM self-heal re-emit
   without resetting `n`. Regression coverage:
   `crates/crypto/tests/sender_keys_idempotence_test.rs`.
2. **B2 — receiver-chain flooding.** Receiver chains are capped at 32 per
   peer and maintained LRU; `ReceiverChainInstall` exposes the evicted
   physical-device id rather than silently discarding it. The auditor should
   review whether the cap and observable eviction are adequate for the caller
   and its persistence path.
3. **B3 — secrets at rest.** This was identified because persisting a live
   rotation root re-derives historical sender keys and persisting skipped
   message keys leaves raw keys at rest. Review the on-disk conversion code
   line-by-line: current compatibility records still contain
   `rotation_root_b64`, and receiver records retain skipped keys to support
   out-of-order delivery. This is therefore an explicitly **unresolved
   at-rest review finding**, not a claim that sealing alone supplies forward
   secrecy.
4. **B4 — duplicate skipped keys.** Cache insertion de-duplicates by
   `(chain_id, n)` before FIFO eviction, preventing replayed distribution from
   filling the 1,000-entry cache with duplicates.
5. **B5 — false idempotency.** The two regression cases prove that reapplying
   the current `(chain_id, root)` leaves progress intact and that a different
   root under that id cannot reseed the receiver. The test must remain green
   under `cargo test -p crypto --test sender_keys_idempotence_test`.

## Questions the audit must answer

- Does the encrypted-header/key-separation design provide the claimed
  confidentiality and integrity under the actual AD, nonce, and counter
  handling?
- Is forward search plus skipped-key caching safe under adversarial ordering,
  duplicate delivery, chain rotation, and receiver-chain eviction?
- Does state persistence preserve the intended forward-secrecy boundary? The
  B3 finding is a blocking question, not documentation debt.
- Is composition of pairwise-ratchet FS and sender-chain FS sound? This is
  explicitly unverified in `sender-keys.md`; no present implementation or
  rotation claim closes it.
- What security properties are lost while SKDM delivery remains stateless v=3,
  and what changes after T19 supplies a post-compromise-secure channel?

## Evidence to provide with the source tree

- `flock /tmp/osl-cargo.lock cargo test -p crypto --no-fail-fast -- --test-threads=1`
- `crates/crypto/tests/sender_keys_idempotence_test.rs`
- sender-key unit tests in `crates/crypto/src/sender_keys.rs`
- group rotation integration coverage in
  `crates/ipc/tests/phase_a3_integration_sk_roundtrip.rs`
- this document, `sender-keys.md`, and the T19 blocked-properties record when
  it is available.
