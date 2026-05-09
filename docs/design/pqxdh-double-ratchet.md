# Design: Hybrid PQXDH + Double Ratchet

Status: **Draft. Review gate. No implementation until cryptographer review.**

This document covers the **DM (one-to-one)** construction. Group
messaging uses sender keys built on top of this; see
[`sender-keys.md`](sender-keys.md).

## Threat addressed

End-to-end confidentiality and authenticity of Discord text messages
and attachments, with:

- **forward secrecy**: compromise of long-term keys does not retro-
  decrypt past messages,
- **post-compromise security**: compromise of session state heals
  after one successful ratchet step from the uncompromised peer,
- **post-quantum security** against harvest-now-decrypt-later: an
  adversary recording today's ciphertext cannot decrypt it with a
  future quantum computer.

## Construction

Two-layer scheme adapted from Signal:

### Layer 1: hybrid PQXDH handshake

Per Kret & Kotov (2023), with explicit hybridization. On first
contact between Alice (sender) and Bob (recipient):

```
DH1 = X25519(IK_A_priv,  SPK_B_pub)        # Alice identity   ↔ Bob signed prekey
DH2 = X25519(EK_A_priv,  IK_B_pub)         # Alice ephemeral  ↔ Bob identity
DH3 = X25519(EK_A_priv,  SPK_B_pub)        # Alice ephemeral  ↔ Bob signed prekey
DH4 = X25519(EK_A_priv,  OPK_B_pub)        # Alice ephemeral  ↔ Bob one-time prekey  (if available)

(SS_pq, ct_pq) = ML-KEM-768.Encaps(MLKEM_B_pub)   # post-quantum encapsulation

SK = HKDF-SHA256(
    salt = zeros,
    ikm  = DH1 || DH2 || DH3 || DH4 || SS_pq,
    info = "discord-privacy-client/pqxdh/v1"
)
```

The initial message includes `IK_A_pub`, `EK_A_pub`, `ct_pq`,
identifier of `OPK_B` consumed (if any), a `no_opk` flag (see "OPK
exhaustion fallback" below), and an AEAD payload encrypted under a
key derived from `SK`.

**Hybrid security property**: `SK` remains secure if **either** the
elliptic-curve discrete log assumption (X25519) **or** the ML-KEM
lattice assumption holds. Both must break.

#### OPK exhaustion fallback (resolved)

When Bob's one-time prekey pool is empty, the server returns
`opk: null`. The handshake transcript collapses to:

```
SK = HKDF-SHA256(
    salt = zeros,
    ikm  = DH1 || DH2 || DH3 || SS_pq,    // DH4 omitted
    info = "discord-privacy-client/pqxdh/v1"
)
```

The handshake header carries an explicit `no_opk = true` flag. The
recipient logs this for transparency. The sender's UI shows a subtle
indicator: "first message has reduced forward secrecy until recipient
comes online." Default behavior: **send anyway**.

### Layer 2: Double Ratchet

Per Marlinspike & Perrin (2016). On `SK` from Layer 1:

- root chain key `RK` initialized from `SK`,
- DH-ratchet step on every received message containing a fresh DH
  key,
- symmetric chain on each direction (sending / receiving),
- per-message AEAD key `MK_n` derived from chain key via HKDF,
- chain key advanced by single HKDF step after each derivation.

**Skipped-message-key cache**: 1000 keys per chain, 30-day TTL —
matches Signal's defaults.

### Header encryption

**Signal-compatible exactly.** A separate header key is derived
alongside the message key, used to encrypt the ratchet metadata (DH
ephemeral, message number, previous chain length). Trial decryption
against current and previous header keys, with strict caps on out-of-
order tolerance.

> Any deviation from Signal's encrypted-header spec requires explicit
> written justification in this doc and sign-off from the reviewing
> cryptographer.

### AEAD

XChaCha20-Poly1305 from `dryoc`. 192-bit random nonce per message;
nonce never reused under a key by construction (chain advances after
each use).

### Associated data (resolved)

```
AD = canonical_encode(
    sender_ik_x25519_pub
    || sender_ik_mlkem_pub
    || recipient_ik_x25519_pub
    || recipient_ik_mlkem_pub
    || conversation_id
    || message_ordinal
    || prev_chain_length
    || session_version
)
```

All fields length-prefixed (4-byte big-endian length, then bytes).
Receiver rejects any AD with mismatched `session_version`. Exact
byte-level encoding is normative and must be specified in the wire-
format companion doc before any implementation.

### Attachment integration (resolved)

Per-attachment AEAD key wrapped under the current message-chain key:

```
AttachmentKey = HKDF-SHA256(
    salt = MK_n,
    ikm  = "attachment-key-wrap",
    info = content_id || attachment_index
)
```

`MK_n` is the message key for the parent text message. **No separate
attachment chain.** Revoking the parent text message's wrapped key
revokes the attachment as well, by construction.

## Padding (mandatory, all messages)

Plaintext padded to fixed buckets **before AEAD**:

- text: 64 / 128 / 256 / 512 / 1024 bytes
- attachments: 256 KB / 1 MB / 5 MB / 10 MB / 25 MB

Content larger than the largest text bucket is split into chunks with
sequence numbers. Padding is inside the AEAD ciphertext and
authenticated — it cannot be stripped without invalidating the tag.

## Stego encoding constraint (per-message independence)

The ciphertext output of AEAD (post-padding, pre-stego) must be
**self-contained per message**. Stego encoding cannot condition
message N's decoding on prior messages: Discord may reorder, edit, or
delete messages on its CDN, so context-dependent encoding breaks
unrecoverably once any context message is lost.

Architectural rationale: context-dependent stego would require
storing messages on our own server, converting the project from
"privacy layer over Discord" into "Discord-skinned messenger with
separate storage" — defeating the core thesis of using Discord's
existing social graph.

**Hard requirement for all stego modes** (Mode 1 templates, Mode 2
Markov, Mode 3 Meteor LLM). The trade-off — fluent per-message
stego that does not form a coherent multi-message narrative — is
documented in [`../THREAT_MODEL.md`](../THREAT_MODEL.md).

## Library choices

| Component | Library | Rationale | Audit status |
| --- | --- | --- | --- |
| X25519 | `dryoc` | Pure Rust, libsodium reimplementation, reproducible-build friendly | dryoc itself unaudited as of 2025; primitives match libsodium reference vectors |
| XChaCha20-Poly1305 | `dryoc` | Same | Same |
| HKDF / SHA-256 | RustCrypto `hkdf`, `sha2` | Mature, audited | Reasonable provenance |
| ML-KEM-768 | RustCrypto `ml-kem` | Pure Rust; FIPS 203 compliant; reproducible-build friendly | Audit-posture verification process below |
| Zeroization | `zeroize` | Standard | Standard |

### `ml-kem` audit-posture verification (mandatory before ship)

- **FIPS 203 test vectors** run on every CI build.
- **ACVP test vectors** run on every CI build.
- **Library version pinned** to a specific minor; bumps require
  manual review and re-running of test vectors.
- **Vulnerability response plan**: on a published CVE in `ml-kem`,
  ship a hot-patch within 72 h that pins to the patched version,
  force-rotates all sessions (bumps `session_version` for every
  active session), and prompts users to upgrade. See
  `docs/security/vuln-response.md` (TBD) for the full runbook.

## Group construction

v1 ships **sender keys** for group encryption — see
[`sender-keys.md`](sender-keys.md) for the full construction,
rotation rules, and threat model. Sender keys deviate from
libsignal's standard pattern (TPM-sealed rotation roots, suspicious-
event auto-rotation, recipient-initiated rotation); deviations are
flagged in `sender-keys.md` for the audit engagement.

## Test vectors

- X25519: RFC 7748.
- ChaCha20-Poly1305 / XChaCha20-Poly1305: RFC 8439 + IRTF CFRG
  XChaCha20 draft.
- HKDF: RFC 5869.
- ML-KEM-768: FIPS 203 + ACVP.
- Combined PQXDH + Ratchet: cross-implementation vectors against a
  reference implementation (libsignal in test mode is the obvious
  candidate), unless the reviewing cryptographer signs off on
  internally-generated vectors.

## Performance targets

- Send (text, encrypt + ratchet step): < 5 ms p95.
- Receive (text, decrypt + ratchet step): < 5 ms p95.
- Stream encrypt 25 MB attachment: < 500 ms p95 on a modern x86-64.
- Memory: streaming AEAD must hold ≤ 16 KB plaintext window at any
  time.
- DM ratchet state across realistic active conversations: bounded
  at ~10 MB. Sender-keys state additional; see `sender-keys.md`.
  Memory monitoring shipped in v1 to confirm assumptions.

## Failure modes

- **ML-KEM library version mismatch** between sender and recipient:
  sessions versioned in handshake header; mismatch surfaces as
  decrypt failure with an explicit error code, never silent fallback.
- **One-time prekey exhausted**: see "OPK exhaustion fallback" above.
- **Out-of-order or dropped message**: skipped-message-keys cache
  capped at 1000 per chain, 30-day TTL. Beyond cap, decrypt fails
  with an explicit error.
- **Compromised long-term identity key**: PQXDH protects new sessions
  only after re-registration. Prior sessions can be retroactively
  forged but not retro-decrypted (forward secrecy property).

## Defense if this layer is compromised

- Wrapped per-message keys live on key server(s). Burn deletes them.
  Even if the in-flight ratchet construction has a flaw, server-side
  burn renders content permanently undecryptable. After burn, content
  on Discord's CDN renders as the original stego'd cover text — see
  `key-server-api.md` and `group-messaging.md` for burn-rendering
  semantics.
- Padding limits length-leak even if the AEAD itself were broken.

## Duress wipe

If the user enters the duress password — or the failed-attempt
threshold is exceeded — the entire ratchet state is wiped. See
[`unlock-and-duress.md`](unlock-and-duress.md) for the full flow. The
DM-relevant items wiped during Phase 2:

- All Double Ratchet sessions (root keys, sending and receiving
  chains, DH state).
- The full skipped-message-key cache.
- Session-version tracking for all peers.
- Per-attachment key wrapping state.
- TPM-evicted long-term identity keys (X25519 + ML-KEM-768) and any
  keychain-fallback copies.

After duress, no past message can be decrypted by this device, and
no future sessions exist. The user re-registers fresh keys after
reinstall (see `key-server-api.md` and `prekey-infrastructure.md`);
contacts must verify the new keys via fingerprint comparison before
trusting.

## Resolved decisions (this round)

1. **ML-KEM library**: RustCrypto `ml-kem` (pure Rust). Audit-posture
   verification process documented above.
2. **Header encryption format**: Signal-compatible exactly. Any
   deviation requires explicit written justification.
3. **Skipped-message-key cache**: 1000 keys per chain, 30-day TTL.
4. **AD scheme**: documented in "Associated data" subsection.
5. **Group fan-out memory cost**: superseded by sender keys; see
   `sender-keys.md`. ~10 MB worst case across realistic active groups.
6. **Attachment integration**: per-attachment AEAD key wrapped under
   the current message-chain key; no separate attachment chain.
7. **OPK exhaustion fallback**: documented in Layer 1 subsection.
8. **Group construction (reverses previous round)**: v1 uses **sender
   keys**, not pairwise fan-out. See `sender-keys.md`.

## Remaining open items

- Cryptographer review of full construction including AD encoding,
  attachment-key derivation, and `no_opk` handshake handling.
- `ml-kem` and `dryoc` audit posture formally confirmed during the
  paid review engagement.
- Cross-implementation test vectors generated and validated.
- Constant-time review of all secret-dependent code paths during the
  review engagement.
- Wire-format companion doc with byte-level AD encoding spec.

## Review gate

- [ ] Cryptographer review of construction (paid engagement, hard
      prerequisite for v1 ship).
- [ ] Test vectors generated and cross-validated, including FIPS 203
      + ACVP.
- [ ] Vulnerability response runbook (`docs/security/vuln-response.md`)
      written and reviewed.
- [ ] Fuzzing harness for AEAD parsing / header decryption.
- [ ] Documented exact wire format with versioning.
- [ ] Constant-time review of all secret-dependent code paths.

## References

- Kret & Kotov, *The PQXDH Key Agreement Protocol* (2023).
- Marlinspike & Perrin, *The Double Ratchet Algorithm* (2016).
- NIST FIPS 203, *Module-Lattice-Based Key-Encapsulation Mechanism Standard*.
- RFC 7748, RFC 8439, RFC 5869.
