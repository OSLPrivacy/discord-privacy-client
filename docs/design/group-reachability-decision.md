# Decision record: group-sender-key reachability

**Owner decision required. No option is selected by this record.**

## Decision

Should the shipping `apps/osl-hub` product expose group conversations that can
reach the sender-key v=5 path for the 2026-08-02 release?

This is not a switch-flip decision. The IPC core enables sender keys, but the
shipping app constructs only `Dm` scopes. The only live group caller is the
excluded legacy `src-tauri` shell. Evidence is recorded in
[group-sender-keys-shipping-state.md](group-sender-keys-shipping-state.md).

## Options

### 1. Defer product group conversations

Ship the app without a group conversation surface. Keep group sender-key code
unreachable in the shipping product and say so accurately.

Cost: no 2026-08-02 group feature, and Group C's rotation work remains
integration coverage rather than a customer-reachable protection. This avoids
exposing an unaudited construction and avoids committing to a group member
limit before the T1 manifest and delivery-tag-window work is complete.

### 2. Build a deliberately bounded group surface for this release

Add the app-level scope declarations, roster/recipient resolution, composition
UI, and receive routing needed for groups, then route only those conversations
through the already-enabled v=5 path. The surface must enforce a selected
member/device cap before send and must fail closed above it; it must also expose
the membership-oracle and sender-authentication limits honestly.

Cost: this is a cross-track product build in `apps/osl-hub`, T3/T4, T1's group
manifest work, and Group C—not a T18-only change. At the proposed five-device
maximum, a 100-member message is up to 500 payload blobs plus a manifest and
501 PUTs; a 500-member message is up to 2,500 payload blobs and 2,501 PUTs.
The delivery-tag subscription window is still unresolved. The custom
construction remains unaudited, B3's at-rest review finding remains open, and
v=3 SKDM transport has no post-compromise security pending T19.

### 3. Build the conversation surface but do not expose sender keys yet

Deliver group conversations with the existing direct-message-grade / v=3
protection path, keeping v=5 unavailable to the product until the audit,
at-rest finding, T19 channel work, manifest composition, and group-cap product
decision are closed. The UI must not imply that group sender-key protections
are active.

Cost: the product work is still substantial and it defers the chief efficiency
benefit of sender keys. It also creates a later migration/routing project and
requires explicit, accurate feature copy so customers do not interpret
“groups” as a sender-key launch.

## Inputs the owner should weigh

- The fan-out measurement is in
  [group-fanout-cost.md](group-fanout-cost.md); sender keys reduce content-key
  work, not per-device blob fan-out.
- The cryptographic review input and open findings are in
  [sender-keys-audit-package.md](sender-keys-audit-package.md).
- Membership for third-party carrier groups is observed rather than
  authoritatively controlled; removal can only rotate once the client observes
  it.
- A carrier-reported author plus TOFU pin currently supplies the sender
  attribution check; that ceremony depends on T5.

## Deadline consequence

The owner’s choice determines whether Group C is a shipping path on
2026-08-02 or remains deferred integration work. Choosing option 2 requires a
separate, explicitly scoped implementation plan; this document does not grant
that scope or select a member cap.
