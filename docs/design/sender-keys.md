# Design: Sender keys for group messaging (v1)

Status: **Draft.** Construction decisions captured below.
**v1 alpha ships without paid cryptographer audit** (budget
constraint; loud disclosure to users in onboarding and
[`../THREAT_MODEL.md`](../THREAT_MODEL.md) audit-status section).
**v1 stable requires audit** — this doc is a hard prerequisite
input to that engagement.

This doc replaces the pairwise fan-out approach previously in
`group-messaging.md`. Per user direction, v1 ships sender keys for
group encryption. Sender keys carry higher cryptographic complexity
than pairwise but ship with bounded blast radius via mandatory
rotation.

## Purpose

Encrypt group messages once per send, with one wrapped sender-key
distribution per recipient (one-time per rotation). Avoids the O(N)
per-message wrapped-key upload cost of pairwise fan-out, at the cost
of a new construction that requires its own audit.

## Construction

### Sender chain

Each sender, per group, maintains a "sender chain" identified by
`(sender_id, group_id, chain_id)`. `chain_id` increments on each
rotation.

Per chain:

- `RotationRoot`: 32-byte CSPRNG output, generated at rotation. Sealed
  to TPM where available (see "TPM sealing" below).
- `CK_0`: chain root, derived from `RotationRoot` via
  `HKDF-SHA256("sender-keys/chain-init", RotationRoot, chain_id)`.
- `CK_n`: derived from `CK_{n-1}` via single one-way HKDF step.
  **Past chain keys cannot be derived from the current chain key** —
  this is mandatory, not optional.
- `MK_n`: per-message key, derived from `CK_n` via HKDF with label
  `"sender-keys/msg-key"`.
- `HK_n`: per-message header key, derived alongside `MK_n` (encrypted-
  header construction matching Signal's spec exactly).

Per send:

1. Compute `MK_n` and `HK_n` from `CK_n`.
2. AEAD-encrypt the plaintext under `MK_n`; encrypt the header under
   `HK_n`.
3. Advance the chain:
   `CK_{n+1} = HKDF(CK_n, "sender-keys/chain-step")`.
4. **Zero `CK_n` and `MK_n` immediately** (via `zeroize`).

State at message N cannot derive message keys for messages 1..N-1.
This is the forward-secrecy property, mandatory.

### Rotation

A rotation creates a fresh `RotationRoot` and resets the chain.
Rotation fires on whichever happens first:

| Trigger              | Threshold                                          |
| ---                  | ---                                                |
| Time                 | 1 hour since last rotation                         |
| Message count        | 500 messages on the current chain                  |
| Membership change    | Add or remove of any group member                  |
| Suspicious event     | Any of the events listed below                     |
| Recipient request    | Any recipient sends a "rotate now" message         |

### Suspicious-event auto-rotation

| Event                                | Source                                          |
| ---                                  | ---                                              |
| Screen-recording software detected   | Hourly process scan (see spec recorder section) |
| App suspend / resume                 | Win32 power notifications                        |
| USB device insertion (capture class) | Win32 device-notification API; see USB classes below |
| ≥ 6 h inactivity (configurable)      | Local idle timer                                 |
| Duress mode entered                  | Internal duress-passphrase pathway               |

Rotations on suspicious events run **before the next outbound
message** is sent on the chain. Adds latency on the next send (one
rotation distribution), surfaced in the UI as a brief
"rotating keys for security" indicator.

#### USB device classes that trigger rotation (resolved)

**Trigger rotation: USB Video Class only, restricted to interfaces
declaring capture descriptors.**

- USB-IF base class `0x0E` (Video) with interface subclass
  `0x02` (`SC_VIDEOSTREAMING`) and an `Input Terminal` descriptor
  whose `wTerminalType` indicates a camera or external composite
  input (`ITT_CAMERA = 0x0201`, `ITT_MEDIA_TRANSPORT_INPUT = 0x0202`,
  external-input terminal types).
- A device exposing only video-control or video-output descriptors
  (e.g. a webcam disabled by hardware, or a capture device in
  loop-back mode without an input terminal) does **not** trigger
  rotation.

**Do NOT trigger rotation** (these classes are common during normal
use; rotation on every insertion would cause rotation storm):

| Class | Hex  | Reason for exclusion                      |
| ---   | ---  | ---                                       |
| Audio | 0x01 | Common: headphones, mics, audio dongles   |
| Comms | 0x02 | Common: modems, network adapters          |
| HID   | 0x03 | Common: keyboards, mice, game controllers |
| Print | 0x07 | Common: printers, scanners                |
| Mass Storage | 0x08 | Common: USB sticks, external drives |
| Hub   | 0x09 | Common: USB hubs                          |
| Smart Card | 0x0B | Common: card readers (legitimate auth) |

#### Recorder-detection cadence (resolved)

Hourly process scan (matches the spec's recorder-handling section).
Detected recorder triggers rotation on the next outbound message,
not immediately (avoids rotation while idle / in background).

#### DoS / spoof cap (resolved)

**Suspicious-event rotations are capped at 1 per 5 minutes.** Further
suspicious events within the 5-minute window queue and trigger one
rotation at the end of the window. Time-based (1 hour) and
message-count (500 msgs) rotations are **not** subject to this cap;
they always fire on schedule.

### Recipient-initiated rotation request

Any recipient may send a signed `rotate-now` request via the pairwise
PQXDH ratchet to the sender. On receipt, the sender's client
immediately triggers a rotation. The request includes a free-form
reason string (e.g., "compromised device suspected"); the reason is
logged in the sender's local audit. Throttle: each recipient may
request rotation at most once per 5 minutes.

### TPM sealing of the RotationRoot

Where TPM 2.0 is available (see `keystore` crate), the active
`RotationRoot` is wrapped under a TPM-bound key. Per-message chain
operations (HKDF of `CK_n → CK_{n+1}`, derivation of `MK_n` / `HK_n`)
remain in regular memory for performance — only the rotation root is
TPM-protected.

**Memory dump bound:** an attacker capturing memory at time `t` can
derive `MK_n` for `n ≥ current_n` (until the next rotation). They
**cannot** derive `MK_n` for `n < current_n` due to the one-way HKDF
step. They **cannot** derive `MK_n` from prior rotations because the
prior `RotationRoot` was zeroed and its sealed copy required TPM
unsealing on this machine in this TPM state.

#### TPM unavailable mid-session (resolved policy)

If TPM access fails during an active session (TBS errors, TPM owner
clear, etc.):

1. **Surface and continue.** The session keeps running with a memory-
   only `RotationRoot` for the remainder of the session.
2. **Persistent non-dismissible UI indicator** displays:
   *"Reduced compromise bound this session — TPM unavailable."*
3. **Failure logged** with diagnostic info (TPM error code, time of
   failure). **No key material logged.**
4. **On app restart**, attempt to re-seal the `RotationRoot` to TPM.
5. **If TPM is unavailable for 3+ consecutive sessions**, escalate to
   a startup warning modal recommending TPM diagnostics or hardware
   inspection.

This is "fail-open with disclosure," not fail-closed — the user can
keep messaging while the bound is reduced. Trade-off: a malicious
TPM-disable attack downgrades to memory-dump bound silently from the
session perspective, but is loud to the user via the indicator.

### Encrypted headers

Match Signal's encrypted-header spec exactly. The header carries:

- `chain_id` (rotation index)
- `message_index` (n)
- `prev_chain_length` (length of the previous chain, for skipped-key
  decryption)

Receiver maintains a window of skipped header keys per chain.

- **Per-chain limit**: 1000 keys (matches pairwise-ratchet default).
- **Total cap across all sender-key chains**: 100,000 entries with
  LRU eviction when exceeded.
- **TTL**: 30 days.

### Distribution

For each recipient `r` in the group, the sender encrypts the new
`RotationRoot` and current `chain_id` to `r` via the existing pairwise
PQXDH + Double Ratchet session with `r`. Distribution messages are
uploaded to the key server alongside the per-recipient wrapped blob.

This means:

- O(N) wrapped uploads at rotation time.
- O(1) wrapped upload per actual group message (one symmetric AEAD
  blob, one wrapped sender-chain reference).

Amortized cost over a 500-message rotation window: O(N/500) per
message on average, much better than pairwise fan-out's O(N) per
message. Forward secrecy on rotation distribution is provided by the
pairwise ratchet itself.

### Distribution acknowledgments

Each recipient's client, on first successful decryption of a new
`RotationRoot`, returns an `ack` to the sender via the pairwise
channel.

Sender's UI displays:

- Acks received: `M of N`.
- Unack'd recipients listed (e.g., "3 of 10 members haven't received
  the new key — they may be offline").

#### Unack'd-recipient escalation (resolved)

| Time since rotation | Behaviour |
| --- | --- |
| 0 – 24 h          | Informational counter only. |
| 24 h              | First warning surfaces in conversation header. |
| 24 h – 7 d        | Persistent warning. Manual override available ("rotate now excluding @user"). |
| 7 d               | **Automatic rotation excluding the unack'd recipient.** |

The 7-day timeout is configurable in advanced settings. The 24-h
warning threshold is also configurable.

Recipients still receive the rotation distribution when they come
online before day 7 (it's queued via the pairwise ratchet). After
day-7 auto-exclusion, the formerly-excluded recipient must be
re-added via a fresh rotation triggered by membership change.

### AEAD

XChaCha20-Poly1305 from `dryoc`. Same primitives as the pairwise
ratchet.

### Associated data

```
AD = canonical_encode(
    sender_ik_x25519_pub
    || sender_ik_mlkem_pub
    || group_id
    || chain_id
    || message_index
    || prev_chain_length
    || session_version
)
```

All fields length-prefixed (4-byte big-endian length, then bytes).
Receiver rejects any AD with a mismatched `session_version`.

## Burn semantics

Burn deletes the per-message wrapped keys (and the rotation
distribution wrapped keys) from the key server. Once deleted:

- Cover text of past messages remains on Discord's CDN, rendered as
  cover text by recipient clients per the burn-rendering rule in
  `group-messaging.md` and `key-server-api.md`.
- New rotations cannot be derived from burned material.

### Burn exposure window under sender keys

Same window as pairwise messages (see `THREAT_MODEL.md`):

- ≤ ~5 min for messages currently visible (until re-validation cycle).
- Until next interaction or 5-min cycle for inactive conversations.
- Zero for recipients still offline at burn time who haven't fetched.
- Indefinite for content captured outside the app.

## Threat model

> Group encryption uses sender keys with rotation triggered hourly,
> by message count, on membership change, or on suspicious events.
> Within each rotation, a forward-secure ratchet ensures past message
> keys cannot be re-derived from current state. A successful attack
> requires either persistent endpoint compromise (which defeats all
> encryption schemes), or a one-time RAM dump combined with rapid
> action before the next rotation. Past messages outside the current
> rotation window remain unrecoverable even with full state
> compromise.

### Attack required to read past group messages outside the current rotation

1. Persistent endpoint compromise of sender or recipient — defeats
   all encryption schemes by definition.
2. Coordinated:
    - RAM dump of sender or any recipient at time `t` (captures
      current `CK_n`).
    - Capture of all chain-key descendants up to the next rotation
      — this only yields message keys within the current rotation
      window (≤ 1 h or ≤ 500 messages).
    - The burned content must still be available for re-derivation
      — but burn deletes the wrapped keys server-side, so re-fetch
      is impossible after burn completes.

**Past messages outside the current rotation window remain
unrecoverable even with full current-state RAM compromise**, by the
forward-secrecy property of the chain.

## Acknowledged complexity tradeoffs

This v1 sender-keys design **deviates from libsignal's standard
sender-keys construction.** libsignal does not include all of TPM-
sealed rotation root, suspicious-event auto-rotation, recipient-
initiated rotation, or our specific rotation-trigger set. All
deviations require explicit cryptographer review.

Sender keys **lose per-recipient post-compromise security** (a
recipient's compromise affects them and the sender for messages
within the current rotation window, vs. pairwise where each recipient
has independent PCS). This is **replaced with bounded-window post-
compromise security** via mandatory rotation: rotations heal
compromise within at most 1 hour or 500 messages, sooner on
membership / suspicious events.

This design **adds suspicious-event auto-rotation**, coupling crypto
state to OS-level signals (process scan, USB events, suspend/resume
hooks). Crypto state is altered by OS observations — review for DoS /
spoofing of those signals to force expensive rotations.

This design **increases v1 cryptographic complexity**. The previous
round's pairwise fan-out reused only the pairwise ratchet, with no
new construction; sender keys add a new construction that must be
audited as part of the v1 review engagement.

The 50-recipient cap from the previous round's `group-messaging.md` is
**removed**. Sender keys do not require a hard cap; rotation
distribution is the only O(N) cost.

## Resolved decisions

1. **Library**: build from scratch on `dryoc` + RustCrypto `hkdf`
   with the construction above. No existing Rust library matches
   our requirements (TPM sealing, suspicious-event triggers,
   recipient-initiated rotation). **Cryptographer review documented
   as a gate for v1 stable, but NOT a blocker for v1 alpha** given
   budget constraints. Loud disclosure of unaudited status to alpha
   users via onboarding and `../THREAT_MODEL.md` audit-status
   section. High-stakes users explicitly directed to Signal / Briar
   / Cwtch.
2. **Rotation distribution failure**: 7-day timeout. UI escalation:
   informational 0–24 h → first warning at 24 h → persistent
   warning until day 7 → automatic rotation excluding the unack'd
   recipient at day 7. Manual override available throughout.
   Configurable in advanced settings. See "Unack'd-recipient
   escalation" subsection above.
3. **Suspicious-event details**:
    - **USB classes**: USB Video Class (`0x0E`) with capture
      descriptor (`Input Terminal` of camera/external-input type).
      All other classes (HID, mass storage, audio, comms, printer,
      hub, smart-card) explicitly do NOT trigger rotation. See
      "USB device classes that trigger rotation" subsection above.
    - **Recorder detection**: hourly process scan; rotation triggered
      on next outbound message; "rotating keys for security" UI
      indicator.
    - **DoS / spoof cap**: 1 suspicious-event rotation per 5 minutes,
      further events queue. Time-based and message-count rotations
      exempt.
4. **Skipped-key cache**: per-chain 1000, total 100,000 with LRU
   eviction, 30-day TTL. See "Encrypted headers" subsection above.
5. **Recipient ack channel**: pairwise PQXDH ratchet. No new
   channel.
6. **TPM mid-session failure**: surface and continue with memory-
   only `RotationRoot`; persistent non-dismissible UI indicator;
   log failure (no key material); on restart re-seal; 3+ consecutive
   sessions without TPM escalates to startup warning. See "TPM
   unavailable mid-session" subsection above.
7. **Group-membership manifest**: v1 trusts Discord's member list.
   Cryptographic admin-signed manifest deferred to v2 roadmap.
   Residual risk documented in `../THREAT_MODEL.md`:
   > *"Discord could theoretically add fake members to your channel,
   > causing your client to encrypt to attacker-controlled keys. This
   > is a narrow threat — adversaries capable of manipulating
   > Discord's member list typically have many other attack vectors."*
8. **FS composition** (pairwise-ratchet FS + sender-key chain FS):
   **deferred to cryptographer review**. v1 assumes soundness based
   on intuitive reasoning. Documented as a known unverified
   property in `../THREAT_MODEL.md`:
   > *"v1 assumes sound composition between pairwise PQXDH-Double-
   > Ratchet forward secrecy and sender-key chain forward secrecy
   > based on intuitive reasoning. Professional cryptographic
   > verification of this composition is gated behind audit
   > funding."*
9. **System messages for burn alerts**: confirmed — same construction,
   same signing path (`IK_X25519`), same audit scope as user
   messages. See `group-messaging.md` burn-and-alert section.

## Remaining open items

- Audit funding and engagement scheduling for v1 stable.
- v2 roadmap: cryptographic group-membership manifest design.
- v2 roadmap: PQ-OPK extension to sender-key rotation distribution
  (currently distribution rides on the pairwise ratchet, which
  inherits PQ coverage from PQXDH identity; PQ-OPK would extend that
  coverage to per-OPK).

## Performance

- Send (encrypt + chain step + AEAD): < 5 ms p95.
- Rotation: O(N) pairwise-ratchet sends, each ≤ 5 ms locally + key-
  server upload latency. For N = 50: ≈ 250 ms encryption + network.
  For N = 500: ≈ 2.5 s + network.
- Memory: bounded by max-active-rotations × max-skipped-keys-per-chain.

## Worst-case memory footprint

Bounded by the resolved cache caps:

- Per-chain limit: 1000 skipped keys × ~100 bytes/key ≈ 100 KB per
  chain.
- **Total cap: 100,000 skipped-key entries across all chains** ≈
  10 MB upper bound for the sender-keys skipped-key cache, with LRU
  eviction.
- Memory monitoring shipped in v1; trip wire at 50 MB sender-keys
  cache (well above the 10 MB cap, catches accounting bugs), evict
  oldest chains on breach.

## Review gate

**v1 alpha** — ships without these gates being met (loud disclosure
required):

- [x] Construction decisions captured (this document).
- [ ] Memory monitoring scaffold in place before alpha ship.
- [ ] Audit-status disclosure copy in onboarding (see
      [`../ONBOARDING.md`](../ONBOARDING.md)).

**v1 stable** — all of the above, plus:

- [ ] Paid cryptographer review of from-scratch construction.
- [ ] Cryptographer review of suspicious-event-rotation coupling and
      DoS / spoof surface.
- [ ] Test vectors for chain step, message-key derivation, encrypted
      headers.
- [ ] Composition proof: pairwise-ratchet FS + sender-key chain FS
      (open question 8 above).
- [ ] Constant-time review of all secret-dependent code paths.

## References

- libsignal-protocol Sender Keys construction (closest existing
  pattern).
- Marlinspike & Perrin (2016), §5 (encrypted headers).
- RFC 5869 (HKDF).
