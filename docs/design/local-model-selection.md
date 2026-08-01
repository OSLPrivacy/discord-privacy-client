# Local model selection

Status: shortlist recorded 2026-07-31; no model is selected or packaged by this document.

<!-- local-model-selection-data
{
  "selectionStage": "shortlist",
  "licenseRequirement": "Apache-2.0",
  "candidates": [
    {
      "model": "Qwen3.5-0.8B",
      "artifact": "Qwen3.5-0.8B-Q4_K_M.gguf",
      "artifactBytes": 532517120,
      "license": "Apache-2.0"
    },
    {
      "model": "Qwen3-1.7B",
      "artifact": "Qwen3-1.7B-Q4_K_M.gguf",
      "artifactBytes": 1107409472,
      "license": "Apache-2.0"
    }
  ]
}
-->

## Decision boundary

The local model is an optional, removable Pro component. It must not be required for encryption
or for the free word-bank carrier. The shortlist has therefore been filtered by redistribution
licence before model size. A model is not selected until T13-D3 measures cover quality,
detectability, latency, and peak RSS; D45 and D50 require the smallest candidate that clears
those gates.

The model receives previous cover text only (or no context at cold start), never message
plaintext. This is the cover-from-cover boundary from D46.

## Licence-qualified candidates, smallest first

| rank | model | Q4_K_M artefact | exact bytes | displayed size | licence result | role in measurement |
|---|---|---|---:|---:|---|---|
| 1 | Qwen3.5-0.8B | `Qwen3.5-0.8B-Q4_K_M.gguf` | 532,517,120 | 532.52 MB / 507.85 MiB | Apache-2.0 | default candidate to test first |
| 2 | Qwen3-1.7B | `Qwen3-1.7B-Q4_K_M.gguf` | 1,107,409,472 | 1.11 GB / 1.03 GiB | Apache-2.0 | test only if the smaller candidate misses a gate |

The size ordering is deliberate: the 0.8B candidate is the provisional default for measurement,
not an implementation decision. The 1.7B candidate stays in the set as the smallest clean-licence
fallback if measurement overturns that default.

## First-hand verification record

Verified on 2026-07-31 against the linked repositories:

| item | verification |
|---|---|
| Qwen3.5-0.8B licence | The upstream [Qwen licence file](https://huggingface.co/Qwen/Qwen3.5-0.8B/raw/main/LICENSE) identifies itself as Apache License, Version 2.0. The Q4_K_M artefact is listed at [Unsloth's GGUF repository](https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/tree/main) as 532,517,120 bytes. |
| Qwen3-1.7B licence | The upstream [Qwen licence file](https://huggingface.co/Qwen/Qwen3-1.7B/raw/main/LICENSE) identifies itself as Apache License, Version 2.0. The Q4_K_M artefact is listed at [Unsloth's GGUF repository](https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/tree/main) as 1,107,409,472 bytes. |

These are current catalogue observations, not supply-chain pins. T13-D5 must choose a distribution
artefact and embed its digest before any download path ships. The artefact publisher is not a
substitute for the upstream Qwen licence verification.

## Excluded before size comparison

The earlier survey's exclusions remain out of this set: Llama 3.2 1B has attribution/naming and
agreement obligations; Gemma 3 has vendor terms and gated access; and LFM uses an Open License
with a revenue cap. They fail the clean-redistribution policy, so a smaller download would not
revive them.

No second classifier model is shortlisted. T13-D3 must measure whether the selected generator can
also meet the sensitive-content classification need. If it cannot, a separate classifier is a
separate optional download and must pass the same commercial-redistribution licence filter before
its size is considered.
