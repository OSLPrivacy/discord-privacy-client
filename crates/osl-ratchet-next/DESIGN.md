# OSL-RN — design, state machine, and claims

> ## STATUS: UNREVIEWED RESEARCH. NOT WIRED IN. MUST NOT CARRY REAL TRAFFIC.
>
> This crate is reachable **only from its own tests**. Nothing in this
> workspace depends on it. It has had **no external cryptographic
> review**, no formal analysis, and no deployment. Every argument below
> is the author's own reasoning, checked by tests, not by a
> cryptographer and not by a proof assistant.
>
> A clean swap-in boundary (see `MIGRATION.md`) makes adoption
> *possible* later. It does not make it *safe* yet. Do not put a real
> user's message through this code before an external review has
> happened.

---

## 1. What problem this is solving

The product's shipping crypto is already better than average: PQXDH
handshake on `x25519-dalek` + `ml-kem` (ML-KEM-768), a Double Ratchet
for OSL Chats (`v=4`/`v=5`), and a PQ-hybrid key-wrap for the Discord
overlay path (`v=3`, no ratchet). There is no libsignal dependency.

Three things about the situation shape this design:

1. **PQXDH is post-quantum at the handshake only.** After the
   handshake, every ratchet step is X25519. An adversary recording
   traffic today and holding it for a quantum computer eventually
   recovers everything past the point where the one PQ secret's
   protection is exhausted. This is true of shipped Signal too. It is
   the single biggest real gap.

2. **The carrier is hostile to protocol overhead.** Messages ride
   steganographic cover text inside a chat client. Bytes are expensive
   — every wire byte becomes ~4/3 bytes of base64 before the stego
   layer expands it further. An ML-KEM-768 ciphertext is 1088 bytes and
   an encapsulation key is 1184. Naively running ML-KEM at every
   ratchet step is not affordable.

3. **The carrier drops messages permanently.** Not "reorders", not
   "delays" — *drops, forever*. Any design that stalls waiting for a
   specific message to arrive is unusable here. This constraint is what
   rules out the obvious PQ-ratchet designs, and it drives most of what
   follows.

---

## 2. The design in one page

**OSL-RN is a pairwise, header-encrypted Double Ratchet whose root
chain is hybrid.** Two ratchets run at different speeds:

```
        +-------------------- classical DH ratchet -------------------+
 root --> step --> step --> step --> step --> step --> step --> ...
          ^                                   ^
          | ss_pq[1]                          | ss_pq[2]
          +-- PQ epoch ratchet ---------------+
              ML-KEM-768 keys and ciphertexts fragmented across many
              messages, retransmitted until acknowledged, and folded
              into the root at a step both sides announce explicitly.
```

- **The classical ratchet** is Signal's, with header encryption. It
  steps on every direction change, gives per-message forward secrecy,
  and tolerates permanent loss because the ratchet public key rides in
  *every* header rather than only the first message of a chain.

- **The PQ epoch ratchet** is decoupled. It never blocks anything.
  Fragments of an ML-KEM encapsulation key travel one-per-message from
  the epoch's owner; the peer reassembles, encapsulates, and fragments
  the ciphertext back; the owner decapsulates. Both now hold `ss_E`.
  Once each side knows the other has it, the next DH ratchet step
  announces `mix_epoch = E` in its header and both sides fold `ss_E`
  into the root KDF alongside the X25519 output.

The crucial property: **losing a fragment costs latency, never
liveness.** Messages keep flowing, the classical ratchet keeps
stepping, and the fragment stream simply retransmits.

### Prior art, stated plainly

Chunking ML-KEM material across messages to build a PQ ratchet on a
constrained carrier is **not novel**. Signal's own SPQR / "Triple
Ratchet" work and Apple's PQ3 (periodic PQ rekey rather than
per-message) are in the same family, and PQ3 in particular established
the periodic-rekey idea. No novelty is claimed here. What is claimed is
(a) that this is a materially stronger posture than the classical
Double Ratchet that ships behind PQXDH, and (b) that the specific
ack-then-announce mixing rule in §4 makes the healing point unambiguous
for both parties under arbitrary loss, reordering and duplication —
which is the part that actually took work to get right (see §9).

---

## 3. Key schedule

Every HKDF call in the protocol lives in `src/kdf.rs`. All labels are
prefixed `OSL-RN/v1/` and are pairwise distinct (asserted by a test).

### Handshake (PQXDH-shaped, `src/handshake.rs`)

```
DH1 = DH(IK_A,  SPK_B)
DH2 = DH(EK_A,  IK_B)
DH3 = DH(EK_A,  SPK_B)
DH4 = DH(EK_A,  OPK_B)              omitted when no one-time prekey
(ct, ss) = ML-KEM-768.Encaps(PQSPK_B)

SK = HKDF-SHA256(salt = 0, ikm = DH1||DH2||DH3||DH4||ss,
                 info = "OSL-RN/v1/pqxdh-root")

root       = HKDF(SK, info = "OSL-RN/v1/root")            [derived once]
HK_A, HK_B = HKDF(SK, info = "OSL-RN/v1/header-key-init") [64 bytes, split]
ss_pq[0]   = HKDF(SK, info = "OSL-RN/v1/pq-epoch-0")
session_id = HKDF(SK, info = "OSL-RN/v1/session-id")      [16 bytes, public]
```

This is deliberately the same shape as `crates/crypto/src/pqxdh.rs`, so
adopting OSL-RN does not also mean replacing prekey publication or
identity management.

### Root step (one per DH ratchet step)

```
ikm = dh_out || ss_pq[mixed+1] || ... || ss_pq[mix_epoch]
okm = HKDF-SHA256(salt = RK, ikm, info = "OSL-RN/v1/root" || LE32(mix_epoch))
    -> RK' (32) || CK (32) || HK_next (32)
```

The X25519 output and every PQ epoch secret being folded in enter the
**same HKDF-Extract**. Recovering `RK'` therefore requires breaking
X25519 *and* ML-KEM-768 for every mixed epoch. Concatenation into a
single extract is the combiner NIST SP 800-56C Rev.2 and PQXDH both
use; it is secure when HKDF-Extract behaves as a dual-PRF, which is the
assumption PQXDH already relies on. **This crate inherits that
assumption and does not independently justify it.**

`mix_epoch` is bound into `info` so two root steps with the same DH
output but different mixing decisions can never collide.

### Chain step

```
MK  = HKDF(salt = CK, ikm = "mk", info = "OSL-RN/v1/message-key")
CK' = HKDF(salt = CK, ikm = "ck", info = "OSL-RN/v1/chain-key")
```

Signal uses `HMAC(CK, 0x01)` / `HMAC(CK, 0x02)`. HKDF with distinct info
labels is the same PRF-on-a-fixed-key construction with explicit domain
separation. **No security difference is claimed.**

### Nonces

- **Body nonce** is derived: `HKDF(MK, info = "OSL-RN/v1/body-nonce")`.
  Each message key is used for exactly one seal — the chain ratchet and
  the skipped-key store (which *removes* a key on use) both structurally
  prevent reuse — so a deterministic nonce is safe and saves 24 wire
  bytes per message.
- **Header nonce** is 12 random bytes on the wire, expanded to
  XChaCha's 192-bit nonce with a 12-byte constant prefix. A header key
  covers one chain, so the birthday bound is 2^48 messages per chain
  against a per-chain counter capped well below that. This trades 12
  bytes of wire for a stated, bounded collision probability.

---

## 4. The PQ epoch ratchet

### Ownership

Epoch 0 is the handshake KEM, owned by the responder (it used the
responder's PQ prekey). Thereafter **epoch `E` is owned by the responder
when `E` is even and by the initiator when `E` is odd.** No negotiation;
both sides compute ownership from role and parity.

Alternating ownership matters: it means both parties periodically
contribute a *fresh* ML-KEM keypair, so compromise of either side's
long-term state is healed, not just one side's.

### The four-phase epoch

| Phase | Owner | Peer |
| --- | --- | --- |
| 1 | after `rekey_interval` sends, generate a fresh ML-KEM keypair; fragment `ek` (1184 B) one fragment per outgoing message | — |
| 2 | keep retransmitting until phase 3 | reassemble `ek`, `Encaps` -> `(ct, ss_E)`, fragment `ct` (1088 B) back |
| 3 | reassemble `ct`, `Decaps` -> `ss_E`, set `pq_have = E`, advertise it in every header | keep retransmitting `ct` until `pq_have >= E` is seen |
| 4 | — | on seeing owner's `pq_have = E`, promote own `pq_have = E` |

The peer holds `ss_E` from phase 2 but cannot advertise it: it does not
yet know the owner received the ciphertext. Phase 4 is the ack.

### Mixing — announced, never inferred

When a party creates a new sending chain it writes

```
mix_epoch = min(self.pq_have, last_seen_peer_pq_have)
```

into the chain's header, **the same value in every message of that
chain**. A receiver replaying that step folds every secret in
`(already_mixed, mix_epoch]` into the root KDF.

Two properties make this safe:

- **The receiver always holds what is announced.** The sender only
  announces up to what the peer itself advertised, `pq_have` is
  monotone, and secrets are pruned only once `mixed` passes them.
- **Both sides perform the same root steps in the same order.** The
  Double Ratchet alternates strictly: `A_1, B_1, A_2, B_2, ...`. Each
  party commits its own chain's mix when it creates the chain and takes
  the peer's when it replays the step, so `mixed` stays in lockstep. A
  transient skew of at most one step is inherent (the creator commits
  before the peer has seen the chain) and is asserted as a bound by
  `tests/healing_latency.rs`.

If a secret really is missing, `take_mix` returns
`Error::MissingPqEpoch` and the message is rejected. It **fails closed**
rather than mixing nothing and silently diverging.

### Fragment retransmission is rotated, not round-robin

Fragments cycle with an extra rotation each pass:
`index = (cursor + cursor / total) % total`.

A plain `cursor % total` has period exactly `total`, so a carrier that
drops on any period dividing `total` aliases perfectly and the same
fragment indices are lost **forever** — the epoch never completes even
though half the messages arrive. This was a real bug found by the "every
other message is dropped" test; see §9.

---

## 5. Wire format (version `0x10`)

```
DPC0::base64(
    version(1)  = 0x10
    flags(1)                    bit0 = BOOTSTRAP preamble present
  [ preamble                    only when BOOTSTRAP:
        initiator_identity(32) initiator_ephemeral(32)
        one_time_prekey_id(varint) mlkem_ciphertext(1088)     ]
    header_nonce(12)
    header_ct_len(varint) header_ct(..)
    body_ct(..)                 to end of buffer
)
```

Encrypted header:

```
msg_type(1) dh_pub(32) pn(varint) n(varint)
pq_have(varint) mix_epoch(varint)
has_fragment(1) [ kind(1) epoch(varint) index(varint) total(varint) data(varlen) ]
```

Associated data chains the regions together:

```
AD_header = "OSL-RN/v1/header" || version || flags || preamble?
AD_body   = "OSL-RN/v1/body"   || version || flags || preamble?
                               || header_nonce || header_ct
```

Every byte on the wire is covered by at least one AEAD tag; the version
and flag bytes are covered by both. `tests/negative.rs` flips every
single bit of a real message and asserts all of them are rejected *and*
that the session is bit-for-bit unchanged afterwards.

Counters are canonical LEB128 varints, not fixed `u32`s: typical values
are small, saving ~12 bytes per message. The decoder rejects
non-canonical and over-long encodings, because the header bytes are the
AEAD's own associated data and malleability there would be a
correctness hazard.

### Version coexistence

`ipc::wire_v2` defines `0x02`..`0x05` and its decoders switch on the
first decoded byte. `0x10` is free in **both** the wire-version and the
message-type namespace (see `WIRE_VERSION_RN`'s doc comment for the full
map and for why "the next free wire version", `0x06`, was rejected —
`wire_v2::MSG_TYPE_SKDM_REQUEST` is also `0x06`, in the other
namespace, and this product inspects raw carrier bytes at fixed offsets
without knowing the version). `Error::WrongVersion` is
returned *before* any length check, so a v=3 blob handed to this
decoder reports "wrong version" rather than "corrupt" and a router can
fall through. `peek_wire_version()` reads the byte without decrypting.
**No flag day is required**; see `MIGRATION.md`.

---

## 6. Ciphertext budget (measured, not estimated)

From `tests/pq_healing.rs::pq_wire_cost_is_amortised` and
`tests/kat.rs`, at default parameters (`fragment_bytes = 128`,
`rekey_interval = 64`), over a 1000-message run:

| Quantity | Bytes (pre-base64) |
| --- | --- |
| Steady-state overhead per message | **85** |
| Overhead on a fragment-carrying message | 220 |
| Mean overhead across the run | **105** (so ~20 amortised for PQ) |
| Messages carrying a fragment | 162 / 1000 |
| Bootstrap (first) message | **1241** |

For scale, a libsignal Double Ratchet message costs roughly 50–60 bytes
of overhead. **OSL-RN is meaningfully more expensive**: ~25 bytes for
header encryption and a full 16-byte tag instead of a truncated 8-byte
MAC, plus ~20 amortised for post-quantum ratcheting.

**Correction (measured, `tests/carrier_budget.rs`): base64 does *not*
multiply the carrier cost.** `ipc::commands` strips the `DPC0::` prefix
and base64-*decodes* before calling `stego::chunk_payload`, so the
carrier chunks the raw wire bytes. The earlier claim that "base64
multiplies by 4/3 and the stego layer multiplies again" was wrong for
this path.

The unit that actually matters is **Discord messages**, not bytes, and
the two carrier modes differ enormously. See §6a.

Tuning knobs: raising `rekey_interval` lowers amortised cost and slows
PQ healing linearly; raising `fragment_bytes` shortens epochs but makes
individual messages spikier.

## 6a. Carrier cost in Discord messages (measured)

From `tests/carrier_budget.rs`. Carrier capacities verified in
`crates/stego`: chunked mode carries
`MODE1_MAX_RAW_LEN (100) - CHUNK_HEADER_BYTES (14)` = **86 payload bytes
per Discord message** (`mode1_chunking.rs:42-46`); token mode puts a
`TOKEN_ID_BYTES (8) + TOKEN_MAC_BYTES (4)` = **12-byte pointer** in the
Discord text and sends the ciphertext to the recipient's key-server
inbox (`mode1.rs:254,259`).

| Quantity | Chunked mode (86 B/msg) | Token mode (12 B pointer) |
| --- | --- | --- |
| Steady-state message (2–60 B plaintext, 87–145 B wire) | **2** Discord messages | **1** Discord message |
| Bootstrap message (1251 B wire) | **15** Discord messages | **1** Discord message |
| Carrier amplification, whole run at defaults | **2.15×** | **1.00×** |
| **Added cost of one full PQ epoch** | **~187 extra Discord messages** at defaults (349 total vs 162 OSL messages); information-theoretic floor is **27** for the 2272 B of ML-KEM material | **0 — free** |

**Token mode makes the PQ epoch ratchet free in carrier terms**, because
the ciphertext never rides the Discord text at all; ciphertext size is
effectively unconstrained and only the 12-byte pointer is billed. This is
the mode the PQ ratchet should run in.

**Chunked mode is where the cost lives**, and it is dominated by the
steady-state 2× amplification rather than by the fragments: at defaults
only 27 of 162 messages carry a fragment.

### A hypothesis the measurement killed

It looks obvious that `fragment_bytes` should be sized so a
fragment-carrying message does not spill into an extra chunk — a
steady-state message already fills 2 chunks (172 B) with ~85 B spare, so
the 128-byte default must spill. **That is wrong.** Smaller fragments need
proportionally more fragments to move the same 2272 bytes, each needing
another round trip, and a round trip is a whole extra message costing 2
chunks. Round trips dominate:

| `fragment_bytes` | OSL messages to heal | Chunked Discord messages | Amplification |
| --- | --- | --- | --- |
| 64 | 196 | 392 | 2.000× |
| 72 | 190 | 426 | 2.242× |
| **128 (default)** | **162** | **349** | **2.154×** |
| 256 | 144 | 325 | 2.257× |

So **larger fragments heal faster and cost fewer total Discord
messages**, and the default of 128 is on the right side of the trade.
Raising `rekey_interval` is the effective lever for chunked mode
(`rekey_interval = 16` heals in 66 messages instead of 162, at 2.38×
amplification).

---

## 7. State machine

### Session states

```
        Session::initiate(peer_bundle)
                 |
                 v
  [INITIATOR, BOOTSTRAPPING]  -- sending chain live, preamble attached
                 |                to every outgoing message
                 |  first inbound message decrypts
                 v
  [ESTABLISHED] <---------------------+
     |  encrypt: chain_step, ++Ns     |
     |  decrypt: see below            |
     +-------------------------------- +

        Session::accept(bootstrap wire)
                 |
                 v
  [RESPONDER, ESTABLISHED]  -- DH-ratchets immediately on the accepted
                               message, so a sending chain exists at once
```

### Receive path

```
parse framing (version, flags, preamble?, nonce, header_ct, body_ct)
  |
  +-- trial-decrypt header against, in order:
  |     HKr (current receiving chain)
  |     NHKr (next receiving chain)
  |     each retained chain's header key   [<= max_chains]
  |   -> no candidate opens: AuthFailed. NOTHING has been mutated.
  |
  +-- clone the session state  (only now: the peer is authenticated)
  |
  +-- absorb header.pq_have and header.fragment into the PQ ratchet
  |
  +-- opened with...
  |     current chain -> n < Nr ? take from skipped store
  |                            : derive+cache the gap, then this key
  |     next chain    -> DH RATCHET:
  |                        bound-check the two skips FIRST
  |                        drain old chain up to header.pn into the store
  |                        take_mix(header.mix_epoch)
  |                        root step 1 -> new receiving chain
  |                        new DH keypair
  |                        root step 2 -> new sending chain, commit own mix
  |                      then as "current chain"
  |     retained chain -> take from skipped store
  |
  +-- open the body with the message key
  |     failure -> AuthFailed, clone discarded, session UNCHANGED
  |
  +-- tick the skipped-key clock, clear bootstrap, COMMIT the clone
```

Decryption is **transactional**: the ratchet advances on a clone and is
committed only when the body tag verifies. A tampered ciphertext cannot
advance, corrupt or wedge a session. The clone is taken only after the
header AEAD authenticates, so an attacker who cannot forge a header
cannot even induce the allocation.

---

## 8. Bounds (all enforced, all tested)

| Bound | Default | What it stops |
| --- | --- | --- |
| `max_skip_per_message` | 512 | A header claiming `counter = 2^32-1` becoming a four-billion-iteration loop. Checked **before any mutation**, so a refusal is a pure no-op. |
| `max_keys_per_chain` | 512 | One chain monopolising the store. |
| `max_total_keys` | 2048 | Global memory. Worst case ≈ 200 KiB of message keys. |
| `max_chains` | 5 | The number of **trial header decryptions** per inbound message: at most `2 + max_chains` = 7 AEAD opens. Without this, header encryption would itself become the DoS vector. |
| `max_age` | 100 000 accepted messages | Stale keys. A *logical* clock, not wall-clock: deterministic (hence testable), immune to clock skew, and correct on a device that was offline for a month. |
| `MAX_RETAINED_EPOCHS` | 8 | Unmixed PQ secrets; exceeding it stops new epochs rather than growing the map. |
| `MAX_FRAGMENTS` / `MAX_FRAGMENT_BYTES` | 64 / 512 | Reassembly buffer ≤ 32 KiB. |
| `MAX_EPOCH_LOOKAHEAD` | 2 | A peer racing arbitrarily far ahead. |
| `MAX_WIRE_BYTES` | 64 KiB | Work before any crypto happens. |

Total worst-case session memory is bounded at roughly 300 KiB
regardless of peer behaviour.

**The genuinely better part is not the caps themselves** — libsignal has
comparable caps. It is that the header is AEAD-authenticated *before*
any key derivation happens. In the non-header-encrypted Double Ratchet
Signal ships, the header is plaintext, so anyone who can inject a
message can name a counter and force the receiver to derive that many
chain keys before the tag check fails. Here an attacker without the
header key causes **zero** derivations and **zero** insertions. The caps
then only constrain a genuine — possibly buggy or hostile — *peer*.

---

## 9. Two real bugs the tests caught

Recorded because they are the two places where "obviously correct"
was wrong, and both are now regression tests.

**1. Periodic loss starved the fragment stream.** The first
implementation emitted fragments round-robin, `cursor % total`. A
carrier dropping every second message aliases exactly with a 10-fragment
stream: the receiver got indices 1,3,5,7,9 forever and the epoch never
completed, even at a 50% delivery rate. Fixed by rotating the emission
order one extra step per pass. Test:
`pq_healing_survives_periodic_loss` (periods 2, 3, 5 and 10).

**2. A duplicated encapsulation-key stream forked the epoch secret.**
Encapsulation is randomised. A late duplicate of an already-assembled
fragment stream re-completed the assembler, the non-owner encapsulated a
*second* time, and the two sides ended up holding different `ss_E`. The
symptom was an unrecoverable `AuthFailed` tens of messages later. Fixed
with an `answered` high-water mark making the non-owner's response
idempotent. Test:
`duplicate_ek_stream_does_not_fork_the_epoch_secret`.

Both are exactly the class of bug that a design "obviously robust to
loss and reordering" hides until it is actually simulated against loss
and reordering.

---

## 10. Claims table

Legend: **BETTER** = stronger than shipped Signal with an argument
given. **EQUAL** = same property, no improvement claimed. **WEAKER** =
worse, stated plainly. **UNPROVEN** = plausible but not rigorously
argued and not to be relied on.

| # | Property | Signal (shipped) | OSL-RN | Verdict | Argument |
| --- | --- | --- | --- | --- | --- |
| 1 | Asynchronous handshake (send to an offline peer) | Yes (X3DH/PQXDH) | Yes | **EQUAL** | Same construction, same prekey-bundle assumptions. |
| 2 | Post-quantum **handshake** | Yes (PQXDH) | Yes | **EQUAL** | Same ML-KEM-768 leg in the same HKDF combiner. |
| 3 | Post-quantum **ratcheting** / harvest-now-decrypt-later resistance | **No** — DH ratchet is X25519 | Yes | **BETTER** | Fresh ML-KEM-768 secrets are folded into the root repeatedly (measured: every ~82 round trips at defaults). An adversary recording the session must break ML-KEM-768 for *every* mixed epoch, not just the handshake, because each epoch's secret enters the same HKDF-Extract as the DH output. Caveats in row 4 and §11. |
| 4 | PQ healing under adverse conditions | n/a | Degrades proportionally | **BETTER** (vs. no PQ ratchet at all) | Measured: ~82 round trips at 0% loss, ~105–126 at 40% periodic loss, ~34 at `rekey_interval=16`. **Requires bidirectional traffic**: a purely one-way conversation never completes an epoch (asserted by `unidirectional_traffic_cannot_complete_a_pq_epoch`). Signal has no PQ ratchet in either case, so this is still strictly ahead — but it is not unconditional. |
| 5 | Per-message forward secrecy | Yes | Yes | **EQUAL** | Same symmetric chain; keys deleted on use. |
| 6 | Classical post-compromise security | Yes, heals in one round trip | Same | **EQUAL** | Identical DH ratchet. |
| 7 | Header encryption | Specified, **not shipped** | Yes, mandatory | **BETTER** | Ratchet public key, both counters, message type and PQ state are inside an AEAD. A passive observer sees version, flags, lengths and random bytes. `wire_v2`'s v=3 path additionally leaves the sender's identity key and message type in the clear; OSL-RN does not. |
| 8 | Metadata: sender identity per message | Hidden from the server by sealed sender | Hidden from a passive observer after bootstrap; exposed in the bootstrap preamble | **EQUAL** at the protocol layer | Signal's PreKeyMessage likewise exposes the initiator's identity key. Neither hides it during bootstrap. |
| 9 | Sealed sender / server-blind delivery | Yes | **Not applicable** | **WEAKER / N/A** | There is no server in this design, and the carrier (a chat account) reveals who is talking to whom by construction. This crate makes **no anonymity claim** and does not attempt one. Honest row: Signal wins here and OSL-RN does not compete. |
| 10 | Out-of-order delivery | Yes, skipped keys | Yes | **EQUAL** | Same mechanism, plus bounded trial header decryption. |
| 11 | Permanent message loss | Tolerated (ratchet key in every header) | Tolerated, including for PQ material | **EQUAL** classically, **BETTER** than a naive PQ ratchet | The classical property is Signal's and is preserved verbatim. The non-obvious part is keeping it *while* adding PQ: `mix_epoch` is fixed at chain creation and repeated in every header, and ML-KEM bytes never ride the ratcheting header. A design putting the ciphertext in the first message of a chain would stall forever on one drop. |
| 12 | Bounded skipped-key storage | Spec: no bound. libsignal: caps (~2000/chain, 5 chains) | Explicit 5-way policy, all tested | **BETTER than the spec, roughly EQUAL to libsignal** in cap strength; **BETTER** in attack surface | The caps themselves are comparable. What differs is that header authentication precedes key derivation, so an attacker who cannot forge a header causes zero derivations — in the plaintext-header design the caps are the *only* defence. |
| 13 | Replay resistance | Yes | Yes | **EQUAL** | Skipped keys are removed on use; in-order keys are consumed by chain advance. Tested immediately and after long delays. |
| 14 | Tampered message cannot corrupt session state | Yes (libsignal commits only on success) | Yes (transactional clone-and-commit) | **EQUAL** | Same idea. Asserted here by exporting state before and after every single-bit tamper of a real message. |
| 15 | Key-compromise impersonation (classical) | Resistant | Same | **EQUAL** | The DH1/DH3 legs use the responder's signed prekey secret, so compromise of a long-term identity key alone does not permit impersonation *to* its owner. Inherited from X3DH; no improvement. |
| 16 | KCI / authentication against a **quantum** adversary | No | No | **EQUAL — and explicitly NOT SOLVED** | Authentication in both designs rests on classical DH and classical signatures over prekeys. A quantum adversary who can break X25519 can impersonate, and the PQ ratchet does not help: KEMs authenticate the *recipient*, not the sender. Fixing this needs PQ signatures on the identity layer, which is out of this crate's scope. **Confidentiality heals post-quantum; authentication does not.** |
| 17 | Offline deniability | Yes | Yes | **EQUAL** | Nothing is signed: not the transcript, not the message, not anything derived from them. `SK` is a function of DH outputs and a KEM shared secret either party could have produced. The prekey-bundle signature (which this crate does not verify — see row 21) is over the signer's own key, not a transcript, exactly as in X3DH. |
| 18 | Online / judge deniability | No | No | **EQUAL** | Neither design defends against an adversary interacting with a judge in real time. Not attempted. |
| 19 | Group / multi-recipient messaging | Yes (sender keys) | **No** | **WEAKER** | This is a pairwise protocol. `wire_v2::encrypt_v3` wraps one body key to N recipients in one blob; a ratchet has no equivalent. N recipients means N sessions and N blobs, or a separate sender-key layer. Deliberately out of scope. |
| 20 | Multi-device / session sync | Yes | **No** | **WEAKER** | Not designed for. State export exists but nothing reconciles two devices ratcheting the same session. |
| 21 | Prekey-bundle authentication | Verified signatures + safety numbers | **Assumed, not verified** | **WEAKER (as a crate)** | `PeerBundle` is taken as ground truth. This matches the boundary `crates/crypto/src/pqxdh.rs` draws, but it means an unauthenticated bundle is an undetected MITM. Called out again in `THREAT-MODEL.md`. |
| 22 | Ciphertext overhead | ~50–60 bytes/message | **85 bytes steady, ~105 mean** | **WEAKER** | ~25 bytes for header encryption + full-length tag, ~20 amortised for PQ ratcheting. Measured, not estimated. Whether this is affordable on the cover-text carrier is the main adoption question. |
| 23 | Session state size | Small | Larger (up to ~300 KiB worst case; typically a few KiB) | **WEAKER** | ML-KEM decapsulation keys are 2400 bytes, fragment buffers are up to 32 KiB, skipped keys up to ~200 KiB at defaults. |
| 24 | Formal analysis / security proof | Multiple published analyses and formal models | **None** | **WEAKER — UNPROVEN** | No proof, no symbolic model, no reduction. The hybrid combiner's security is *assumed* from PQXDH's own assumption; the epoch-mixing synchronisation argument in §4 is prose plus tests, not a proof. Treat every "BETTER" above as an engineering claim, not a theorem. |
| 25 | Implementation maturity | Audited, deployed at enormous scale, years of adversarial attention | **Unreviewed, zero deployment** | **WEAKER** | The single most important row in this table. |
| 26 | Constant-time / no-panic hygiene | Yes | Yes | **EQUAL** | `#![forbid(unsafe_code)]`; `deny(indexing_slicing, unwrap_used, panic)`; all secrets zeroize on drop; secret comparison via `subtle`; AEAD failures collapse to one indistinguishable error; `Debug` impls redact. Not a protocol claim — an implementation claim, and only as good as the review it hasn't had. |

| 27 | Forward secrecy of the **recovery/reset channel** | Signal's session-reset paths are themselves ratcheted or re-handshaked | **None.** `MSG_TYPE_SKDM_REQUEST` (`0x06`) and `MSG_TYPE_SESSION_RESET` (`0x07`) deliberately ride `wire_v2::encrypt_v3`, whose wrap key is a PQXDH between two *static* identity keys | **WEAKER** | This is a correct design decision — recovery must work when the ratchet is desynced, so it cannot depend on the ratchet (`wire_v2.rs:149-151`) — with an honest cost that adopting OSL-RN does **not** fix. Compromise of either long-term identity key retroactively decrypts *and forges* every recovery/reset message. The ratchet migration leaves this channel exactly as weak as it is today; it is now the weakest link in an otherwise forward-secret conversation. Rate limiting and the act-on-symptom guards in the recv handler are load-bearing security controls, not hygiene. |

### Summary in one paragraph

**Genuinely stronger than shipped Signal:** post-quantum ratcheting
(rows 3–4), header encryption by default (row 7), and a smaller
skipped-key attack surface as a consequence of row 7 (row 12).
**Equal:** handshake, forward secrecy, classical PCS, out-of-order and
loss handling, replay, classical KCI, deniability, transactional
decryption. **Weaker:** wire overhead, state size, no groups, no
multi-device, no sealed sender, and — decisively — no formal analysis
and no review. **Explicitly not solved:** post-quantum *authentication*
(row 16).

---

## 11. Bounds stated explicitly

- **Classical PCS healing:** one round trip, as Signal.
- **Post-quantum PCS healing:** measured at ~82 round trips at defaults,
  ~105–126 under 40% periodic loss, ~34 with `rekey_interval = 16`.
  Requires bidirectional traffic. Between epoch mixes, PQ protection is
  the *previous* epoch's — so a quantum adversary who compromises state
  at time *t* recovers up to one epoch's worth of subsequent traffic
  before healing completes.
- **Forward secrecy granularity:** one message, immediately, except for
  keys held in the skipped store — which are exactly the keys the
  bounded store is designed to age out.
- **Skipped-key retention:** at most `max_total_keys` keys, at most
  `max_age` accepted messages old, at most `max_chains` chains.
- **Work per inbound message from an unauthenticated source:** parsing
  plus at most 7 AEAD opens over ≤ 1024 bytes. Zero KDF invocations,
  zero allocations of ratchet state.

## 12. What was considered and deliberately not done

- **ML-KEM at every DH step.** Correct and simple, but 2272 bytes per
  direction change on a carrier where 85 bytes is already expensive, and
  it makes epoch completion depend on specific messages surviving. Ruled
  out by constraint 3 in §1.
- **PQ signatures (ML-DSA) for authentication.** Would close row 16, but
  ML-DSA-65 signatures are ~3300 bytes and — more importantly — signing
  the transcript would destroy offline deniability (row 17). That is a
  real trade, not an oversight; a deployment that values PQ
  authentication over deniability should make it consciously.
- **Erasure coding of fragments.** Reed-Solomon over the fragment set
  would beat rotation under bursty loss. It adds a dependency and
  meaningful complexity for a gain the measurements did not yet justify.
  A reasonable future step.
- **Group / sender-key integration.** Out of scope (row 19).
- **Sealed sender.** No server exists to blind (row 9).
- **Wall-clock TTL on skipped keys.** Rejected in favour of a logical
  clock: deterministic, testable, and immune to clock manipulation.

## 13. Test inventory

87 tests, all passing, none ignored (2 doc-tests are `ignore`-fenced API sketches).

| File | Covers |
| --- | --- |
| `src/primitives.rs` | RFC 5869 HKDF and RFC 7748 X25519 published vectors, low-order point rejection, ML-KEM implicit rejection, AEAD tamper detection |
| `src/codec.rs` | canonical varint boundaries, over-long and non-canonical rejection, truncation never panics |
| `src/kdf.rs` | label distinctness, root/chain step determinism and independence |
| `src/pq.rs` | epoch completion, completion under 50%/66% loss, wrong-direction fragment rejection, duplicate-EK idempotence, fail-closed mixing, pruning |
| `src/skipped.rs` | every cap, age expiry, export/import, over-cap state rejection |
| `src/handshake.rs` | both-sides agreement with and without OPK, unknown OPK id, tampered KEM ciphertext, preamble truncation |
| `tests/interleaving.rs` | 24 seeded random delivery schedules, duplicates/replays, 35% permanent loss, whole-chain loss, 4000-message unidirectional run, full reverse-order delivery, bootstrap under loss+reordering, export/import every step |
| `tests/pq_healing.rs` | epoch advance and agreement, 40% random loss, periodic loss at four periods, reordering, wire-cost budget, one-way limitation |
| `tests/healing_latency.rs` | measured PCS healing period at three settings, skew bound |
| `tests/bounds.rs` | absurd-gap refusal as a no-op, storage caps over a long lossy run, eviction fails closed, trial-decryption cap, oversized blobs |
| `tests/negative.rs` | every single-bit flip at every offset, every truncation length, header/body splicing, cross-session, malformed framing by category, random garbage, forged bootstrap, corrupt state |
| `tests/kat.rs` | determinism, frozen wire/state vectors, message-size budget |
