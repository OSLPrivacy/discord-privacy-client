# OSL-RN — threat model

> **This crate is unreviewed research and is not wired into anything.**
> See `DESIGN.md`. Nothing below should be read as a guarantee.

This document is deliberately written as a list of **what OSL-RN does
not defend against**. The positive claims live in the claims table in
`DESIGN.md` §10; everything here is a gap, a boundary, or an assumption
someone else has to satisfy.

---

## 1. What it does defend against

Briefly, so the rest has context. Against a network adversary who can
read, drop, reorder, duplicate, delay and inject arbitrary messages on
the carrier:

- Message confidentiality and integrity, with per-message forward
  secrecy.
- Post-compromise security against a classical adversary in one round
  trip, and against a *quantum* adversary within roughly one PQ epoch
  (~82 round trips at defaults).
- Metadata: the ratchet public key, both counters, message type and PQ
  epoch state are encrypted.
- Denial of service via memory exhaustion or unbounded computation.
- Corruption of session state by tampered or injected messages.

---

## 2. Assumptions that must hold, which this crate does not enforce

### 2.1 Peer key authenticity — the big one

`PeerBundle` is **assumed already authenticated**. This crate does not
verify the signature over the signed prekey or the PQ prekey, does not
implement TOFU, does not compute safety numbers, and has no notion of
key change.

**If an attacker substitutes the bundle, they are a full
man-in-the-middle and nothing in this crate will notice.** Every
confidentiality property above is conditioned on the caller's trust
layer being correct. This is the same boundary
`crates/crypto/src/pqxdh.rs` draws, but it must be re-stated at every
adoption.

### 2.2 One-time prekey lifecycle

`LocalPrekeys::one_time_prekeys` is a plain list. This crate never
deletes a used one-time prekey. An integrator that fails to delete on
first use degrades to the no-OPK security level (still X3DH-sound, but
losing the OPK's contribution) and enables replay of the *bootstrap
message* to establish duplicate sessions.

The id `0` is reserved to mean "no one-time prekey". A deployment that
numbers a real prekey `0` will silently take the no-OPK path.

### 2.3 Randomness

`OsRng` is assumed to be a real CSPRNG. A compromised or predictable RNG
breaks the ephemeral handshake key, every ratchet keypair, every ML-KEM
keypair and encapsulation, and every header nonce. Nothing here detects
or mitigates that.

`test_support` deliberately uses a seeded, reproducible RNG. **It must
never be used outside tests.**

### 2.4 State-at-rest protection

`export_state()` emits **every secret the session holds** — root key,
chain keys, header keys, cached message keys, ML-KEM decapsulation keys.
It is not encrypted, not authenticated, and not zeroized after the
caller takes ownership of the `Vec`. Sealing it (TPM, keystore, OS
keychain) is entirely the integrator's job.

---

## 3. Not defended against — endpoint and platform

- **Endpoint compromise.** Malware, a debugger, a keylogger, or a
  screen-capture tool on either device reads plaintext before or after
  this code runs. No cryptography addresses this.
- **Memory disclosure while a session is live.** Secrets zeroize on
  drop, but a live session necessarily holds live keys. Swap files,
  hibernation images, core dumps and cold-boot attacks are out of scope.
- **Compromise of the persisted state blob.** An attacker who reads
  `export_state()` output has the session. Forward secrecy protects
  *past* messages only to the extent that old keys have been dropped —
  and the skipped-key store deliberately retains some, up to its cap.
- **Rollback of persisted state.** Restoring an older state blob
  re-enables message keys the session had already consumed. Nothing here
  detects rollback; a persistence layer that needs this must provide
  monotonic-counter or sealed-storage guarantees itself.
- **Side channels beyond the basics.** Secret comparisons use `subtle`
  and the primitives are constant-time implementations, but no
  claim is made about cache, branch-predictor, speculative-execution or
  power/EM side channels, and none has been measured.

---

## 4. Not defended against — traffic analysis

- **Who is talking to whom.** The carrier is a chat account. Sender and
  recipient are visible to the platform by construction. There is no
  sealed sender and no attempt at anonymity.
- **When they are talking.** Timing is fully exposed.
- **How much they are saying.** Ciphertext length is proportional to
  plaintext length; there is **no padding** in this crate. The product's
  existing `crypto::padding` module is not applied here.
- **Whether a message carries PQ fragments.** A fragment-carrying
  message is ~135 bytes longer. An observer who can measure length
  precisely can identify the ~16% of messages that carry fragments and
  therefore infer the PQ epoch schedule. This leaks no key material but
  is a real distinguisher, and padding would remove it.
- **Which protocol version is in use.** The version byte is plaintext by
  design (it has to be, for routing).
- **Conversation boundaries.** Nothing links messages to a session
  cryptographically for an observer, but the carrier account does.

---

## 5. Not defended against — cryptographic

- **Post-quantum impersonation.** Authentication rests on classical
  X25519 and classical signatures over prekeys. A quantum adversary who
  can break X25519 can impersonate either party going forward. The PQ
  ratchet protects *confidentiality*, not authentication — a KEM
  authenticates the recipient, never the sender. Closing this requires
  PQ signatures at the identity layer, which is out of scope and would
  cost offline deniability. **This is stated as a limitation, not a
  future work item that is quietly assumed solved.**
- **Break of ML-KEM-768 *and* X25519.** The hybrid means both must hold.
  It does not mean either alone suffices — it means neither alone
  failing is fatal, which is a different and weaker statement than
  "unbreakable".
- **A dual-PRF failure in HKDF-Extract.** The combiner's security
  assumes HKDF-Extract behaves as a dual-PRF. That assumption is
  inherited from PQXDH and is not independently justified here.
- **Nonce-misuse.** The body nonce is derived from the message key.
  Safety depends on each message key being used exactly once. The chain
  ratchet and remove-on-use skipped store enforce that structurally, but
  a state-restore bug that resurrected a consumed key would produce
  nonce reuse under a reused key, which is catastrophic for
  XChaCha20-Poly1305. There is a test for the restore case; there is no
  proof.
- **Header nonce collisions.** 96 random bits per header key. The
  birthday bound is 2^48 messages under one header key; chains are
  capped far below that, but the bound is stated rather than eliminated.

---

## 6. Not defended against — availability

- **Carrier-level denial of service.** An adversary who can drop all
  messages stops the conversation. Nothing here helps.
- **PQ healing starvation by traffic shaping.** An adversary who can
  drop selectively can delay PQ epochs. Measured degradation is
  proportional (~2x at 40% loss) and the rotation scheme defeats simple
  periodic patterns, but an adversary with full control over which
  messages are dropped can starve the epoch indefinitely while still
  delivering user messages. Detection of this is **not implemented** —
  an integrator that cares should alarm on `pq_mixed_epoch()` failing to
  advance.
- **One-way conversations never heal post-quantum.** A user who only
  sends and never receives stays on epoch 0 forever. This is inherent:
  the ciphertext needs a return path.
- **Bounded-store eviction as a correctness failure.** When the skipped
  store evicts, the corresponding messages become permanently
  undecryptable. This is a deliberate availability-for-memory trade, and
  it means a peer that sends huge bursts with big gaps can cause
  *legitimate* message loss. Tuning `SkipParams` is a policy decision
  with real user-visible consequences.

---

## 7. Out of scope entirely

- Group messaging, sender keys, multi-recipient blobs.
- Multi-device session synchronisation.
- Message ordering guarantees, delivery receipts, retry logic.
- Key transparency, key directories, contact discovery.
- Attachment encryption, message expiry, view-once, capture protection —
  all of which exist elsewhere in this product and are unaffected.
- The steganographic carrier itself, its detectability, and everything
  about how bytes get into and out of a chat client.

---

## 8. The residual risk that dominates all of the above

**This code has not been reviewed by a cryptographer.** Two
non-obvious correctness bugs were found by its own tests during
development (`DESIGN.md` §9), which is evidence the tests are doing
something — and equally evidence that a design that *looked* right was
twice wrong in ways that would have silently broken real conversations.

Assume there are more. Do not put real traffic through this before an
external review.
