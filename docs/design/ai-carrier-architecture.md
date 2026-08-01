# AI carrier architecture

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
