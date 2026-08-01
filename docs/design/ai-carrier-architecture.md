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

## LLM-driven coding

**Decision: evaluated and rejected for v1.** Encoder-driven LLM steganography
(the Meteor/Discop class) would replace the deterministic cover codec's CDF
with a model's next-token distribution. It is not a shipping carrier, a
fallback, or a format a peer may attempt to decode in v1.

The rejection is a correctness decision, not a judgement about cover quality.
The sender and receiver must reproduce bit-identical next-token distributions.
Published measurements report roughly 5--10% extraction failure for 128-bit
payloads from hardware floating-point nondeterminism alone; tokenisation drift
can reduce Discop recovery to 0%. Autoregressive desynchronisation then
propagates into total decode failure rather than a recoverable bad cover. No
production deployment of this class is known. This also conflicts with the
optional, Pro-only local model: a receiver without the exact model would lose
the message rather than safely use the deterministic decoder.

It may be reconsidered only as a new, negotiated per-conversation capability.
Both peers must advertise the same verified `TrustedModelPack::artifact_digest`
before it can be selected, and decoding must first be demonstrated across two
different CPU families at a measured failure rate. The digest accessor already
exists in `crates/cover-draft/src/lib.rs`; v1 implements neither the capability
advertisement nor LLM-driven coding.

### Decision contract

```json
{
  "version": 1,
  "llmDrivenCoding": {
    "v1Status": "evaluated-and-rejected",
    "isFallback": false,
    "revival": {
      "requiresNegotiatedPerConversationCapability": true,
      "requiresMatchingTrustedModelPackArtifactDigest": true,
      "requiresCrossCpuMeasuredDecodeFailureRate": true
    }
  }
}
```

## Conditions for a future consumer

A consumer may be added only when it supplies a real local `LocalCoverModel`
implementation backed by a verified `TrustedModelPack`, exposes cancellation
and failure to the user, and preserves the explicit approval boundary. Until
then, no production route should depend on this crate and an unavailable model
must remain a visible, fail-closed condition.
