# AI carrier architecture

Status: design inventory and keep decision. This is not evidence that an AI
cover carrier ships today.

## What changed

Cloud carrier generation must not process the user's real plaintext or selected
private-message context. It conditions only on previous cover messages; when
there is no cover history, it generates a plausible random conversation from
nothing.

With the local model nothing leaves your device; if you choose cloud generation
it only ever sees text the platform already has. Cloud generation can still
reveal conversation membership to OSL, so this is not a zero-knowledge claim.

## Credits purchase contract

Under owner decision D72, v1 ships local AI first and measures it before any
cloud offering is considered. Consequently, v1 does not offer cloud generation
credits, a credits checkout, or crypto payment. Supported crypto chains are
none in v1.

T13 does not create a second payment route. The existing Stripe paths are for
donations and Pro; neither is a credits product and neither may be repurposed
as one. Credits and Pro time are separate balances and must never be conflated.

If a measured, explicitly approved future cloud offering needs purchasable
credits, **T11 owns the purchase surface and payment-provider integration**.
T13's role is limited to the credit balance and metering contract. A T11
checkout may be offered only after its checkout gate is green and the T13
credit contract exists; it must grant credits rather than a Pro entitlement.
Until then, no Buy button, Stripe session, crypto flow, or payment API is
permitted for credits.

### Contract test vectors

```json
{
  "version": 1,
  "v1": {
    "cloudGenerationOffered": false,
    "creditPurchaseOffered": false,
    "supportedCryptoChains": []
  },
  "future": {
    "checkoutOwner": "T11",
    "requiresCheckoutGate": true,
    "proEntitlementIsCreditBalance": false
  }
}
```

## Existing code: keep `osl-cover-draft`

`crates/cover-draft` remains a workspace member but has no consumer crate.
The repository-wide dependency survey finds references only in the workspace
manifest, lockfile, review documentation, and the crate itself. Its sole local
model adapter is `UnavailableModel`, which fails closed with
`ModelError::Unavailable`.

**Decision: keep the crate.** It is not dead transport code or a hidden
shipping AI feature. It contains reusable, model-independent security and
interaction work that a future local adapter should use rather than recreate:

- signed model-pack metadata verification, including observed artifact digest
  and size checks;
- caller-supplied, bounded authorized context and a hash binding to the exact
  account, conversation, recipient, and canonical private-message hash;
- hard limits, cooperative cancellation, and generation deadlines;
- zeroized draft and model output storage; and
- an editable draft lifecycle: prepare, explicit scope-bound approval,
  one-time consumption, cancellation, and expiry. Editing invalidates prior
  approval.

Keeping this boundary prevents a later model integration from gaining implicit
history, network, filesystem, or send authority. A trusted model pack is
necessary before generation; with no adapter installed, the existing behavior
is correctly unavailable rather than a fabricated cover.

## Build layer versus wire layer

`osl-cover-draft` is the build layer only. It receives explicitly authorized
local context and returns a reviewed, one-time consumable text draft with model
provenance. It deliberately has no networking, transport, persistence, or
destination/send API.

The wire layer is a separate future integration concern. It may choose a cover
source, encode the encrypted payload, and send through the established
transport path only after the user has approved the exact draft. That layer
must not treat a generated draft as authorization to send, or let model output
alter encryption, recipient binding, or transport semantics.

## Conditions for a future consumer

A consumer may be added only when it supplies a real local `LocalCoverModel`
implementation backed by a verified `TrustedModelPack`, exposes cancellation
and failure to the user, and preserves the explicit approval boundary. Until
then, no production route should depend on this crate and an unavailable model
must remain a visible, fail-closed condition.
