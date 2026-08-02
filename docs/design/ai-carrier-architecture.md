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

## Measurements, not a detectability claim

These are the reproducible measurements currently available in this repository.
They describe a codec and a fixed selection harness, not a claim that the
resulting cover is undetectable. Run the named test with `--nocapture` to print
the source value before changing any of these numbers.

### Codec capacity and cover length

`crates/cover-ai/tests/entropy_budget.rs` measures the shipping bigram model's
stationary conditional entropy at **5.05 bits/word** (accepted range ±0.02).
At that rate, its idealized pointer payload lengths are:

| Pointer payload width | Ideal words | Ideal characters (including spaces) |
|---:|---:|---:|
| 96 bits | 19.0 | 90 |
| 128 bits | 25.4 | 121 |
| 160 bits | 31.7 | 151 |
| 192 bits | 38.0 | 182 |

The expanded 256-word candidate corpus measures **6.012429583 bits/word** in
`crates/cover-ai/tests/corpus_quality.rs`; its corresponding figures are 16.0/
86, 21.3/116, 26.6/145, and 31.9/174 for 96/128/160/192-bit pointers. These are
capacity calculations, not a naturalness or detectability measurement. The
bounded vocabulary and protected-message timing remain observable.

### Selection gain and distortion

The frozen, auditable score corpus in `crates/cover-ai/tests/selection_gain.rs`
selects **K = 13**. Its measured quality proxy is **12.000** at K=13 and
**12.000** at K=256: the larger pool produced **0.000** additional proxy gain.
`crates/cover-ai/tests/selection_detectability.rs` reports a selected-versus-
unselected rank displacement of **0.429** at K=13, under its 0.45 cap. This is
a distribution-distortion proxy, not a classifier accuracy figure; it is the
reason selection must be described as a quality trade-off rather than a
detection-evasion property.

### Local-model comparison

No model has been packaged or benchmarked in this worktree, so latency, peak
RSS, cover-quality, and detector-rate columns are deliberately **unavailable**.
The table records every licence-qualified candidate, including the larger one
that could lose on the required measurements, rather than pretending a model
was selected from its parameter count.

| Candidate | Q4_K_M artifact size | Quality / detectability / latency / peak RSS | Result |
|---|---:|---|---|
| Qwen3.5-0.8B | 532,517,120 bytes | Not measured | No winner declared |
| Qwen3-1.7B | 1,107,409,472 bytes | Not measured | No loser declared |
| Llama 3.2 1B | n/a | Not measured | Excluded before measurement: attribution/naming and agreement obligations |
| Gemma 3 | n/a | Not measured | Excluded before measurement: vendor terms and gated access |
| LFM | n/a | Not measured | Excluded before measurement: revenue-capped Open License |

The candidate sizes and licence exclusions are sourced in
[`local-model-selection.md`](local-model-selection.md). A future model choice
must replace every `Not measured` cell with a reproducible result; it must not
replace them with an adjective.

### Political-speech false positives

There is no political-speech classifier in v1 and therefore no valid
classifier false-positive rate to publish. The executable policy measurement
is **0/3**: `crates/sensitive-classify/tests/political_speech_is_not_sensitive.rs`
checks that the three potentially political lexical detector categories
(`Profanity`, `PotentiallyUnlawfulConduct`, and `ControlledSubstances`) map to
no send-time warning. That is a policy-coverage count, **not** a claim of 0%
false positives on political speech. Any future semantic classifier must
publish a held-out political-speech corpus, denominator, and false-positive
rate alongside its other results before it can be enabled.

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

## Why not encrypt the cloud call?

OSL's cloud-generation design is governed by D46: it sends no protected-message
plaintext to a cloud model. At most it can send cover text that the destination
platform already has, plus a style hint; a cold start needs no conversation text.
This is not a claim that cloud generation has no metadata cost: OSL can still
observe request IP (unless routed), timing, frequency, size, and which cover
conversation was used. It is the reason the alternatives below are rejected.

### Fully homomorphic encryption (FHE)

FHE for LLM inference is rejected as a dead end, not merely deferred. A published
CKKS Llama-3-8B result reports 20 s prefill and 18 s per token on eight RTX PRO
6000 GPUs for a 128-token encrypted prompt. The comparison plaintext baseline is
about 10 ms per token on one H100: roughly 1,800x wall-clock and 14,000x
GPU-seconds. A 40-word cover sentence would take about 12 minutes on eight
top-end GPUs. That is incompatible with interactive cover generation.

### Secure multi-party computation (MPC)

MPC is also rejected. PUMA reports about 200 s and about 1.8 GB of communication
for a single output token of LLaMA-7B. The faster PermLLM result still takes about
3 s per token, relies on a heuristic substitution rather than secure evaluation,
and requires the weight-holding server for every token. It does not create a
usable or adequately private cloud path for this feature.

### Embeddings, split inference, and activations

Sending an embedding instead of text is not encryption. Embedding inversion work
recovered 92% of 32-token inputs exactly (BLEU 97.3) from
`text-embedding-ada-002`, including full names in clinical notes. Noise and
8-bit quantisation did not provide a reliable defence. Split inference has the
same problem: cut-layer activations preserve recoverable input information.
OSL therefore does not send plaintext, embeddings derived from plaintext, or
intermediate activations to cloud generation.

### Confidential computing

Confidential computing (including SEV-SNP, TDX, NVIDIA Confidential Computing,
and Apple PCC-style approaches) is unnecessary under D46 because plaintext is
never sent to the cloud call. This is deliberately recorded as unnecessary, not
unavailable: a TEE would add operational complexity without protecting data that
the design excludes from the request.

It would also replace a direct data-flow guarantee with a hardware-attestation
assumption. That means trusting the silicon vendor, its firmware, its attestation
root, the host configuration, and the absence of implementation vulnerabilities.
If a future design proposes sending plaintext, it must be reviewed as a new
privacy architecture; adding a confidential-computing wrapper does not preserve
the D46 guarantee.

## Free tier today

<!-- current-free-cover-record
{
  "currentCarrier": "fixed beacon string",
  "currentValue": "🔒 OSL private message",
  "observerSignal": "an exact-match classifier identifies it",
  "productionStealth": "off",
  "requiredFreeCarrier": "word-bank carrier only"
}
-->

The current free-cover implementation returns the fixed string `🔒 OSL private
message`. It carries no message-dependent variation, so an observer can label it
with a single exact-match rule. This is a beacon, not cover traffic.

That helper currently has no production call site. This does not make the
design safe: it is still the implemented free-cover path, ready to create a
perfect classifier signal if wired into sending.

The shipping path is not a stealth mechanism today. It coerces Mode 1 to Mode
0 and emits a visible `DPC0::` capsule. Product and public copy must therefore
describe stealth as **off**, not merely degraded.

## Required free-tier floor

Owner decision D49 requires **word-bank carrier only** for Free. The word-bank
path must always ship and work with no model pack, network access, credits, or
cloud consent; encryption must never depend on an optional component.

T13-B2 replaces the fixed free-cover constant with the existing codec's
word-bank output. It must not introduce a second word list. Until that change
lands, the fixed beacon is an explicitly recorded defect rather than a
stealth claim.

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
