# OSL Spaces prototype triage

> **Status: design vocabulary only; no implementation may claim a capability from this prototype.**
> Surveyed 2026-07-31 against the frozen feature, transport, and storage contracts. Owner decision
> D66 puts OSL Spaces in v1; D68 forbids claiming a capability before it ships.

The prototype at `docs/prototypes/osl-chats-lab/` is a useful visual shell, not an
implementation or a security design. This document records the boundary so subsequent Spaces
work can retain its good interaction vocabulary without importing its unsupported product claims.

## Reuse as UI vocabulary

The following are visual or interaction starting points only. Their future implementations must
meet the contracts cited below.

| Prototype surface | Live evidence | Reuse boundary |
|---|---|---|
| Three-column community navigation: circle rail, channel sidebar, message pane | `docs/prototypes/osl-chats-lab/app.js:317-326` | Keep the information architecture; do not preserve the `Circle` product name or fixture state. |
| Text/Voice visual split | `app.js:320-326` | Keep the distinction, but use an explicit channel type. The prototype infers voice from an id/name string and adds a hard-coded `studio` row. |
| Reply, reactions, author scrub, and view-once affordances | `app.js:267-280`, `:454-457`, `:554-557` | Keep as a message-primitive list. The feature contract, not this UI, defines lifecycle, receipts, and destruction semantics. |
| Message-rules modal and its honest external-camera boundary | `app.js:401-408` | Keep the boundary language. The controls themselves do not implement the frozen lifecycle. |
| No rendered member list | `app.js:317-326` | Preserve this privacy posture: a Space must not become a roster/presence surface merely because a member count is visible. |
| Audit events keyed by device public-key digest instead of friendly name | `app.js:145-148` | Retain as a design input for a future signed governance log; it is not an audit implementation. |

## Designed from zero

### Governance, membership, and moderation

The prototype has no membership record, member object, join/leave/remove flow, invite object,
directory object, or moderation model. Its community membership is only fixture counts and channel
arrays (`app.js:104-118`). There is no member list renderer in the Circle view (`app.js:317-326`).

The only apparent role model is not functional: `chatDetailMarkup()` contains `Curators` and
`Stewards` at `app.js:285-291`, but no call site renders that function, and the entire detail panel
is disabled by `styles.css:199`. The visible Roles button is also wired to the encryption modal,
not a roles surface (`app.js:324`, `:552-553`). Roles, permissions, membership authority, invites,
removal, governance logging, discovery, and reporting therefore require new designs.

Moderation does not exist. A keyword scan of `app.js` has zero hits for `moderation`, `moderate`,
`kick`, `report`, `slowmode`, and `verification`; the sole delete affordance is author-side scrub
(`app.js:280`, `:557`). Do not read local scrub as moderator deletion or treat it as a governance
mechanism.

### Delivery, cryptography, and persistence

There is no network, cryptography, or backend in the prototype: its `app.js` has zero uses of
`fetch`, `WebSocket`, `RTCPeerConnection`, `getUserMedia`, `indexedDB`, or `crypto`. It instead
uses one local-storage namespace, `osl-chats-lab-v5` (`app.js:1`, `:186-197`, `:568`), and its
export manifest explicitly labels the data unencrypted, prototype-only, and unsigned
(`app.js:481-489`). Voice is a modal mock (`app.js:437-440`, `:550-551`, `:580`), not a media
transport.

Consequently, the prototype supplies no basis for E2EE, sender authentication, key rotation,
offline delivery, server deletion, or live collaboration. The seed claims and local toasts are
not product evidence.

## Frozen-contract deltas the prototype cannot decide

| Frozen requirement | Contract evidence | Prototype gap / required consequence |
|---|---|---|
| A Space message fans out as one blob per recipient device. | `03-CONTRACTS/features.md:196-208`; `03-CONTRACTS/storage.md:557-572` | Fixture member counts cannot establish a roster, device targets, or delivery scale. Membership and device-roster work must be designed before fan-out. |
| Group delivery uses one carrier pointer to a manifest and isolated recipient copies. | `03-CONTRACTS/transport.md:166-195`; `03-CONTRACTS/storage.md:221-240` | The prototype has no carrier, manifest, pointer, or per-copy keys; its “Circle” messages cannot be treated as a group-wire design. |
| Payload delivery is eager, persisted locally before storage acknowledgement, and supports offline reading afterward. | `03-CONTRACTS/features.md:128-145`; `03-CONTRACTS/storage.md:386-415` | `localStorage` fixture persistence is neither encrypted local storage nor the required receive/ack lifecycle. |
| View-once, expiry, and burn have separate server and local effects; receipts are optional and recipient-controlled. | `03-CONTRACTS/features.md:275-305`, `:353-395`, `:450-508` | The modal’s lifetime, opens, and forwarding controls (`app.js:401-408`) are UI ideas only; their current local mutation does not satisfy any lifecycle guarantee. |
| Delivery uses a persistent, constant-rate padded connection rather than fixed-cadence polling. | `03-CONTRACTS/transport.md:391-413` | The prototype has no delivery connection at all and must not dictate a polling or real-time implementation. |
| The relay keeps no Space membership roster. | Owner decision D66; T21 track §3.2 items 4–6 | Aggregate fixture counters (`app.js:115-117`, `:324`) are not a permissible server-side membership design. |

## Conclusion

Use the prototype for navigation, message affordances, and privacy-oriented wording. Build every
governance, membership, moderation, cryptographic, delivery, and voice capability from zero against
the frozen contracts. In particular, no Spaces claim may be derived from this interactive local
prototype.
