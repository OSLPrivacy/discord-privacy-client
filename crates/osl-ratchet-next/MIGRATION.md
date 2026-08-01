# Migrating to OSL-RN (wire `0x10`) — what an integrator must actually do

> **Nothing in this document has been done.** This crate is not wired
> into anything and must not be until it has had external cryptographic
> review (`DESIGN.md`). This is a costing exercise, written so that the
> cost is visible *before* anyone commits to it.

Read `DESIGN.md` §10 for what you gain. This document is about what it
costs, what breaks, and which of those breaks are dealbreakers.

---

## 0. Executive summary

| | |
| --- | --- |
| **Effort on the OSL Chats path (`v=4`/`v=5`)** | Moderate. That path already has a ratchet and already persists per-peer session state. |
| **Effort on the Discord overlay path (`v=3`)** | **Large, and possibly not worth it.** Three behaviours that path relies on are structurally incompatible with a ratchet — see §4. |
| **Flag day required?** | No. `0x10` is a free version byte and the existing router already switches on it. |
| **Biggest single blocker** | Re-decryption. The Discord path decrypts the same stored ciphertext repeatedly; a ratchet can decrypt each message exactly once. §4.1. |

---

## 1. Version coexistence — no flag day

`ipc::wire_v2` defines `WIRE_VERSION_V2 = 0x02` through
`WIRE_VERSION_V5 = 0x05`. This protocol takes **`0x10`**, reserved by
`osl_ratchet_next::session::WIRE_VERSION_RN`.

`0x10`, not `0x06`. `0x06` is the next free *wire version*, but `0x06` is
already `wire_v2::MSG_TYPE_SKDM_REQUEST` in the *message-type* namespace
(byte 1, documented in `control_messages.rs`). Those are different
namespaces at different offsets, so `0x06` would not have been a live
protocol conflict — but this product classifies opaque inbox bundles by
reading raw bytes at fixed offsets (`wire_v2::is_native_overlay_relay_bundle`
and siblings do `bundle[0] == WIRE_VERSION_V3 && bundle[1] == MSG_TYPE_*`),
and nothing in the type system distinguishes the two byte ranges. `0x10`
is above every value assigned in either namespace, so a blob
mis-dispatched between them fails loudly instead of matching something
plausible. Full map in `WIRE_VERSION_RN`'s doc comment.

**Integrator warning:** in an OSL-RN blob **byte 1 is `flags`, not a
message type**, and the real message type is inside the encrypted header
— unavailable until a decrypt succeeds. Any fixed-offset probe that
reads `bundle[1]` as a message type must gate on
`bundle[0] == WIRE_VERSION_V3` first. The existing probes already do.

The receiving router at `crates/ipc/src/commands.rs:4370` already does:

```rust
let version = peek_wire_version(&content);
let recovered = match version {
    Some(WIRE_VERSION_V2) => { ... }
    Some(WIRE_VERSION_V3) => { ... }
    // v=4, v=5 ...
};
```

Adding `0x10` is one more arm. Nothing about the existing arms changes.
This crate provides `osl_ratchet_next::peek_wire_version(&str) -> Option<u8>`
with identical semantics (decode the first four base64 chars, return
byte 0) so a router can use either.

`Session::decrypt` returns `Error::WrongVersion { got, expected }`
**before any length check** when handed a non-`0x10` blob, so a
mis-routed v=3 message reports "wrong version" and can fall through
rather than being logged as corruption.

### Mixed-version pairs: an unrecognised version is NOT reliably reported

Two dispatchers behave differently, and the difference decides how a
mixed-version pair fails:

| Path | Unknown input | Consequence |
| --- | --- | --- |
| v=2/v=3 dispatcher, `crates/ipc/src/commands.rs:4540` | `other => Err("msg_type 0x.. not supported")` | **Fails closed and reports.** |
| Native-overlay drain, `apps/osl-hub/src/broker.rs:2283` | `is_native_overlay_relay_bundle(&bundle)` is false, so `continue` | **Silently dropped.** No error, no log, no trace. |

An OSL-RN blob has `bundle[0] == 0x10`, which is not `WIRE_VERSION_V3`,
so `is_native_overlay_relay_bundle` returns false. **An OSL-RN message
sent to a peer running an older build vanishes on the overlay path.**

Consequences for the rollout, and they are not optional:

1. **Never send OSL-RN opportunistically to an unproven peer on the
   overlay path.** Silence is indistinguishable from delivery. Capability
   must be established *before* the first OSL-RN send — which is what
   `ipc::wire_rn::select_wire_version` requires (`peer_supports_rn`) and
   what the key-server capability advertisement in §8 is for.
2. **Receive-only first.** Ship a build that understands `0x10` and
   never emits it, and wait for it to saturate, exactly as step 1 of the
   rollout below says. There is no in-band way to discover that the far
   side dropped your message.
3. **A "0x10 dropped silently" state is indistinguishable from a
   downgrade attack** from the sender's point of view. This is why the
   version pin is sticky and refuses to fall back rather than retrying on
   v=3: a retry loop here would turn a delivery failure into an automatic
   downgrade.

### Recovery and reset must stay OUTSIDE the ratchet

`wire_v2.rs:149-151` and `:157-164` document this deliberately, and it
must survive the migration:

- `MSG_TYPE_SKDM_REQUEST` (`0x06`) and `MSG_TYPE_SESSION_RESET` (`0x07`)
  ride `encrypt_v3`, **not** a ratchet, because they must work precisely
  when the ratchet is desynced — which is the only time they are sent.
- Moving them onto OSL-RN would make session recovery depend on the
  session being healthy, which is circular and would turn a recoverable
  desync into a permanently dead conversation.
- So they stay on `encrypt_v3` after this migration. That is correct, and
  it has an honest cost: **`encrypt_v3` has no forward secrecy.** Its
  wrap key comes from a PQXDH between two *static* identity keys, so
  whoever compromises either long-term identity key can decrypt every
  recovery/reset message ever sent **and forge new ones retroactively**.
  Recorded as row 27 of the claims table in `DESIGN.md` §10.
- Practical implication: the recovery channel is the weakest link in an
  otherwise forward-secret conversation, and it is authenticated only by
  a static key. Rate limiting and the act-on-symptom guards in the recv
  handler are load-bearing security controls, not just hygiene.

### Rollout shape

1. Before enabling `0x10` traffic, complete the state-safety and recovery
   prerequisites, then mint a second capability bit that only a fuse-open
   build advertises. The existing bit is already advertised by current
   builds and cannot safely be used as the live-traffic signal.
2. Select `0x10` only when the peer's new live-traffic capability is
   verified. A release that advertises that bit must open both send and
   receive fuses together; a receive-only build would attract traffic it
   cannot answer.
3. Sessions established as `v=3` stay `v=3` for their lifetime. A `0x10`
   session begins with a bootstrap message and runs forward from there.
   There is no in-place upgrade of an existing conversation and none
   should be attempted — mixing a stateless wrap and a ratchet over one
   logical conversation has no coherent security story.
4. In-flight `v=3` messages sent before the switch remain decryptable
   forever, because the `v=3` decoder is untouched.

### Capability advertisement already ships; its current bit is not a live-traffic signal

`RegisterRequest` has an `rn_capabilities` field. Both
`keystore::client::build_register_request` and
`keystore::client::build_rotation_request` set it to
`CLIENT_RN_CAPABILITY_FLOOR` (which includes `RN_CAP_WIRE_RN`) and sign
the extended `REG_MSG`. `verify_peer_capabilities` also has a production
caller in `ipc::commands::verified_rn_capabilities_for_live_peer`.

This means a verified current-build peer can already select OSL-RN. While
`RN_WIRE_IN_ENABLED` is false, the selection path fails closed rather than
downgrading to `v=3`: it refuses the send because the build cannot honour
the selected wire. The old receive-only-then-advertise rollout is therefore
no longer available.

Do not remove the existing bit: lowering an advertisement is a downgrade.
Instead, reserve a second live-traffic bit. Only a build with both OSL-RN
fuses open may advertise that bit, and wire selection must require it. That
keeps older peers on `v=3` and prevents a fuse-closed build from attracting
OSL-RN sends.

Once OSL-RN traffic ships with the live-traffic bit, a user who downgrades
to a pre-ratchet build while keeping the same identity key becomes
unreachable from peers that pinned them. For those peers,
`select_wire_version` returns `Err(PinnedToRn)`; a version rollback does
not lower the pin. Recovery requires the explicit, both-sides-confirmed,
out-of-band unpin ceremony. A new identity key also resets the relationship.

---

## 2. Call sites that change

### 2.1 Send path

Today (`apps/osl-hub/src/broker.rs:1323`):

```rust
fn encrypt_direct_manual_v3_payload(core, peer, message_type, payload) -> Result<String, String> {
    let identity = /* lock, clone, unwrap */;
    let recipients = [RecipientV3 { self.. }, RecipientV3 { peer.. }];
    ipc::wire_v2::encrypt_v3(&identity.x25519_secret, &identity.x25519_public,
                             &recipients, message_type, payload)
}
```

With OSL-RN:

```rust
fn encrypt_direct_manual_v6_payload(core, peer, message_type, payload) -> Result<String, String> {
    let mut session = load_session(core, peer)?;          // NEW: fallible I/O
    let wire = osl_ratchet_next::encrypt_rn(&mut session, message_type, payload)?;
    persist_session(core, peer, &session)?;               // NEW: must not be skipped
    Ok(wire)
}
```

The signature is deliberately the same shape — `(.., msg_type: u8,
plaintext: &[u8]) -> Result<String, _>` — so the ~9 call sites
(`broker.rs` lines 1320, 1964, 2808, 3205, 4246, 6039, 6306, 6344 and
their tests) change by name and by the addition of load/persist, not by
restructuring.

**The structural change is statelessness.** `encrypt_v3` is a pure
function of static keys. `encrypt_rn` mutates a session that must be
loaded before and persisted after, atomically with respect to the
message actually being sent. If a message is sent but its state is not
persisted, the peer's ratchet advances and the sender's does not; if
state is persisted but the send fails, one counter is burned (harmless).
**Persist-after-send-succeeds is the wrong order; persist-before-send is
correct** and costs at most a skipped counter on failure.

### 2.2 Receive path

Today (`broker.rs:3647`) `decrypt_direct_manual_v3_payload` is a pure
function. It becomes load / decrypt / persist, with the same
`Result<Vec<u8>, String>` shape. `Opened { msg_type, plaintext }` is
field-compatible with `wire_v2::DecryptedV2`.

### 2.3 Session establishment

`v=3` has no establishment step — any two identities can exchange
immediately. `0x10` needs one:

- The initiator calls `Session::initiate(&identity_secret, &peer_bundle, params)`
  and gets a session whose first messages carry a 1153-byte bootstrap
  preamble (repeated until the peer replies).
- The responder calls `Session::accept(&local_prekeys, wire, params)`,
  which returns both the session and that first message's plaintext.

`PeerBundle` maps directly onto what `ManualPeerBinding` already stores
(`peer_x25519_public`, `peer_mlkem768_public`). `LocalPrekeys` maps onto
`identity.x25519_secret` / `mlkem_decapsulation_key()`. **One-time
prekeys do not exist in this product today**; the handshake works
without them (`one_time_prekey: None`) at a small cost in forward
secrecy for the first message. Adding OPK publication is optional and
independent.

---

## 3. State persistence

`Session::export_state() -> Result<Vec<u8>>` /
`Session::import_state(&[u8]) -> Result<Session>`. Format is versioned
(`STATE_FORMAT_VERSION`), self-delimiting, and refuses trailing bytes.

**Size:** a few KiB typically. Worst case ~300 KiB, dominated by the
skipped-key store (default caps allow ~200 KiB) and an ML-KEM
decapsulation key (2400 B).

**Where it goes:** `peer_map.json` is the wrong home — it is a
plaintext-adjacent JSON file and this blob contains every key the
session holds. The right home is the existing encrypted `store` crate
(`MessageStore`-style, keyed off the identity) or a TPM-sealed blob via
`keystore`. `session_id()` gives a stable, non-secret 16-byte key both
sides compute identically, suitable as a storage key.

**Requirements the persistence layer must meet:**

- Sealed at rest. The export is plaintext secrets.
- Written atomically. A torn write loses the session.
- Not rolled back. Restoring an old blob resurrects consumed message
  keys, which is a nonce-reuse hazard (see `THREAT-MODEL.md` §5).
- Zeroized after use. The `Vec<u8>` handed back is not self-zeroizing.

---

## 4. What changes on the Discord path — the honest part

The Discord overlay path currently uses `wire_v2::encrypt_v3`: a
PQ-hybrid *wrap* with per-recipient slots and **no ratchet**. Adopting
OSL-RN there is a behavioural change, not just a code change.

### What you gain

**Per-message forward secrecy, which that path does not have today.**
`encrypt_v3` derives each recipient's wrap key from a PQXDH between the
sender's *static* identity key and the recipient's *static* identity
key. Compromise of either long-term key retroactively decrypts **every
message ever sent on that path**. A ratchet reduces that to the messages
whose keys are still resident. This is a large, real improvement — it is
the single strongest argument for doing this at all.

Plus: post-quantum ratcheting, header encryption (the v=3 wire currently
exposes the sender's identity key and message type in the clear on every
message), and per-message key independence.

### What you lose — three things, in decreasing order of severity

#### 4.1 Re-decryption. **This is the blocker.**

`encrypt_v3` is stateless, so the same stored ciphertext can be
decrypted any number of times. The overlay relies on this:
`rehydrate_native_discord_overlay_history` and
`decrypt_direct_manual_v3_payload` (called from 9 sites) re-open stored
Discord message bodies on demand to render transcript history, and
several loops opportunistically try to decrypt every bundle in an inbox
and `continue` on failure.

**A ratchet can decrypt each message exactly once.** The message key is
destroyed on use — that *is* forward secrecy. Every one of those
re-decrypt sites would return `AuthFailed` on the second attempt.

There are exactly two ways out, and both are real work:

- **Cache plaintext.** Decrypt once on arrival, store the plaintext in
  the encrypted `store`, render from there. This is what Signal clients
  do. It moves the security boundary from "ciphertext at rest in
  Discord" to "plaintext at rest in our own encrypted store", which is a
  deliberate trade the product should make consciously — note that
  view-once, expiry and burn semantics all currently lean on *not*
  having a durable plaintext copy.
- **Retain message keys.** Keep the message key alongside the stored
  ciphertext. This straightforwardly destroys the forward secrecy you
  adopted the ratchet to get. Not recommended.

**Nothing else in this migration is as decisive as this one.** If the
product cannot accept a plaintext cache, OSL-RN does not belong on the
Discord path.

#### 4.2 Self-decryption of sent messages

`encrypt_direct_manual_v3_payload` builds a two-recipient slot array —
`[self, peer]` — so the sender can read back its own outgoing messages
from the Discord transcript. A pairwise ratchet has one direction per
chain and **cannot decrypt its own ciphertext**.

Options: cache the plaintext at send time (same trade as §4.1, and the
natural companion to it), or run a second "note to self" session and
emit two blobs (doubling wire cost on a carrier where wire cost is the
whole problem).

#### 4.3 Wire-level sender attribution

`inspect_v3_wire` (`broker.rs:3898`) reads `raw[0] == 3`, the message
type at `raw[1]`, the sender identity key at `raw[2..34]`, and the
recipient hash prefixes — all from the *plaintext* header — to decide
who wrote a message and in which direction it travelled.
`verify_manual_v3_type` is built on this and is used as an
authentication gate before decryption.

**`0x10` encrypts all of it.** That is the point of header encryption,
and it is a metadata improvement — but it means direction and authorship
can only be determined *by successfully decrypting*, which is the
cryptographically correct answer and a rewrite of that gate. The
replacement is: attempt decryption with the peer's session; success *is*
the authentication. That is a simpler and stronger check, but it is not
a drop-in edit of `inspect_v3_wire`.

#### 4.4 Multi-recipient sends

Any `encrypt_v3` call with more than two recipients (scope/whitelist
sends via `whitelist::recipients_for_scope_v3`) has no `0x10` equivalent:
a pairwise ratchet means N sessions and N blobs. Those paths should stay
on `v=3`, or move to the existing `crypto::sender_keys` layer. OSL-RN
deliberately solves the pairwise problem only.

### 4.5 Statelessness and permanent loss — the trade, precisely

The brief for this work asked whether adopting a ratchet costs the
`v=3` path its tolerance for permanent message loss. The answer is
**mostly no, with one caveat**:

- **A permanently lost message does not stall the ratchet.** The
  ratchet public key and the PQ `mix_epoch` ride in *every* header, so
  any surviving message of a chain lets the receiver catch up in one
  step. This is tested against 35–40% permanent loss and against whole
  chains vanishing.
- **The caveat is the bounded skipped-key store.** If a peer sends a
  burst larger than `max_skip_per_message` (512 by default) and only the
  last message arrives, that message is *refused* — cleanly, with the
  session intact, but refused. `encrypt_v3` would have opened it. This
  is a genuine availability regression, tunable but not eliminable, and
  it is the price of bounding memory.
- **Ordering across a reconnect.** A stateless wrap does not care about
  history. A ratchet does: a client that loses its session state loses
  the conversation and must re-bootstrap. State durability becomes a
  correctness requirement, not just a nicety.

---

## 5. Ciphertext budget — may decide this on its own

Measured at default parameters (`DESIGN.md` §6):

| | `wire_v2::encrypt_v3` (2 recipients) | OSL-RN `0x10` |
| --- | --- | --- |
| Per-message overhead | ~2400 B (two 1150-byte slots) | **85 B** steady, **~105 B** mean |
| First message | same ~2400 B | 1241 B |
| PQ material | every message | ~16% of messages |

**OSL-RN is roughly 23× cheaper per message than the current
two-recipient `v=3` wrap**, because `v=3` pays a full PQXDH
ML-KEM ciphertext *per recipient per message* while the ratchet
amortises it. On a low-bandwidth cover-text carrier this is not a
rounding difference — it is the difference between one carrier row and
twenty.

Against a plain Double Ratchet (libsignal, ~50–60 B) OSL-RN is ~2×
more expensive. Against what this product ships on the Discord path
today, it is dramatically cheaper.

Base64 multiplies all figures by 4/3; the stego layer multiplies again.

Tuning: `PqParams::rekey_interval` trades amortised bytes against PQ
healing latency linearly (64 → ~20 B/msg and ~82 round trips per heal;
16 → ~80 B/msg and ~34 round trips). `SkipParams` trades memory against
tolerance for large gaps.

---

## 6. Suggested order of work, if it is ever undertaken

1. **External cryptographic review.** Nothing below matters until this
   happens. Two non-obvious correctness bugs were found by this crate's
   own tests during development (`DESIGN.md` §9); assume more remain.
2. Decide the §4.1 question — plaintext cache or not. If the answer is
   "not", stop: OSL-RN cannot serve the Discord path.
3. Land sealed session persistence in `store`/`keystore`, keyed by
   `session_id()`, with atomic writes.
4. Add the `0x10` arm to the receiving router in `commands.rs`. Ship
   receive-only.
5. Add per-peer `0x10` capability negotiation alongside the existing
   ML-KEM capability check.
6. Migrate the OSL Chats path (`v=4`) first — it already has a ratchet
   and per-peer state, so §4.1 and §4.2 do not bite there.
7. Only then evaluate the Discord path, with §4 resolved and real
   telemetry on carrier bandwidth.

---

## 7. The interface you would be coding against

```rust
pub trait SecureSession: Sized {
    type Bundle;   // peer's published, already-authenticated keys
    type Prekeys;  // local secrets matching our own published bundle
    type Params;
    type Opened;

    fn initiate_session(local_identity: &XSecret, peer: &Self::Bundle,
                        params: Self::Params) -> Result<Self>;
    fn accept_session(local: &Self::Prekeys, wire: &str,
                      params: Self::Params) -> Result<(Self, Self::Opened)>;
    fn seal(&mut self, msg_type: u8, plaintext: &[u8]) -> Result<String>;
    fn open(&mut self, wire: &str) -> Result<Self::Opened>;
    fn export(&self) -> Result<Vec<u8>>;
    fn import(bytes: &[u8]) -> Result<Self>;
}
```

Plus free functions shaped like the current ones: `encrypt_rn`,
`decrypt_rn`, `accept_rn`, `peek_wire_version`.

Nothing in that surface names Discord, a carrier, an overlay, a scope, a
whitelist or a hub. It is a general secure-messaging session, which is
the point — a future swap should be a matter of changing which module
the broker calls, not redesigning the broker.
