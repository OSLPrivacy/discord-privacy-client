# Group properties blocked on T19

Verified against the working tree on 2026-07-31. This document defines the
boundary between T18's sender-key work and T19's stronger ratchet; it is not a
product-shipping claim.

## Current boundary

The IPC core can select sender-key v=5 for a group scope, but the shipping hub
constructs DM scopes only. When the core emits a sender-key distribution
message (SKDM), `send_skdm_via_v3_bundle` encrypts it in v=3 recipient slots.
Each slot is opened with the recipient's long-term X25519 and ML-KEM keys. The
fresh v=3 sender ephemeral protects the sender's past messages, but it does not
heal a compromised recipient: the recipient's long-term keys can open every
future SKDM sent to that device. A compromised device therefore learns each
future sender-chain root delivered to it.

The physical-device identifier carried by an SKDM keys the installed receiver
chain. It does not make the v=3 transport stateful and does not provide
post-compromise security (PCS) for that transport.

## Dependency matrix

`T19 gate` means that T19 is a necessary condition for the property. `Extra
gate` names work that T19 alone cannot close.

| Property | Current status | T19 gate | Extra gate | Claim status |
| --- | --- | --- | --- | --- |
| Per-member PCS for SKDM delivery | A long-term recipient-key compromise decrypts future v=3 SKDMs and exposes future chain roots. | blocked | T19 must route SKDMs per device over its recoverable stateful channel and prove restart/offline recovery. | Do not claim. |
| “Rotation heals compromise” | Rotation replaces the sender chain, but the replacement root still travels in v=3 to the compromised recipient. | blocked | T19 delivery recovery proof; successful delivery of a post-recovery root to each device. | Do not claim. |
| Bounded group compromise window | No defensible bound exists while the attacker can read every future root. The current rotation trigger is 24 hours or membership change, not the stale one-hour/500-message policy. | blocked | A specified and enforced rotation/ack policy, plus T19 recovery proof. | Do not claim. |
| Forward-secrecy composition of the pairwise channel and sender-key chain | Unverified: the currently used SKDM channel is not a pairwise ratchet, and the composition question starts only once T19 supplies one. | blocked | F2 cryptographic audit closes this; T19 tests do not. | Remain unverified after T19 until F2 closes. |

## What T19 must close for these rows

Owner decision D41 allows wiring and testing the stronger engine, but it does
not allow making it the default until its prior silent-desync failure is
measurably closed. For the rows above, T19 must provide a per-device SKDM
route with persisted state, detected and recoverable desynchronization, and a
parallel or fallback path that does not lose messages. The required evidence
includes a two-identity proof across restart and offline operation. D78 further
requires an explicit, both-sides-confirmed out-of-band unpin ceremony rather
than silent downgrade.

Merely wiring T19 for direct-message content does not close this matrix. The
SKDM send and receive path has to select the T19 route, and the delivery and
recovery evidence must cover every recipient device that receives a chain root.

## Properties T19 does not decide

- Whether a product user can create a group: the shipping hub currently has no
  group conversation surface.
- Group sender attribution and membership trust: these retain the T5 identity
  and membership dependencies.
- Sender-key construction correctness, rotation-trigger correctness, and
  per-device receiver-chain handling: these are T18 responsibilities and need
  their own tests.
- Cryptographic soundness of the ratchet/sender-key forward-secrecy composition:
  this remains an F2 audit result, not a conclusion from implementation tests.

T18 must therefore keep its v=3 SKDM limitation explicit and must not use a
sender-chain rotation test as evidence of PCS, healing, or a bounded compromise
window. T19 owns the secure distribution-channel transition; F2 owns the audit
conclusion.

## Evidence consulted

- `docs/design/group-sender-keys-shipping-state.md` — corrected core and
  shipping-app reachability.
- `crates/ipc/src/commands.rs` — v=3 SKDM bundle transport, 24-hour or
  membership-change rotation, and device-bound SKDM receipt.
- `docs/THREAT_MODEL.md` — v=3 recipient-compromise limitation and ratchet
  desynchronization history (some sender-key routing positions there are stale;
  the shipping-state document records the corrected positions).
- `plan/09-DECISIONS.md` D41, D76, and D78 — T19's recovery/default gate,
  first-usable scope, and unpin ceremony.
