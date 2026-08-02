# OSL Spaces fan-out measurement

**Task:** T21-K4  
**Status:** awaiting a real-store measurement; no cap or availability claim is derived from this document.

The earlier arithmetic is a planning estimate, not evidence.  This record is
the required capture format for a repeatable run against the deployed cipher
store.  Populate it only from transport/store instrumentation for the same
run; do not infer bytes or requests from source code.

## Method

For each cohort size (5, 20, and 100), create a fresh Space against the real
store, add the members, and send one uniquely tagged carrier. Capture the
store's rows written, request bytes, PUT count, grants minted, and the final
manifest byte length. Repeat three times and record the median. The probes
must use the real store endpoint and authenticated principals, never the
in-memory test store.

| Members | Run IDs | Rows | Request bytes | PUTs | Grants minted | Manifest bytes |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 5 | not run | — | — | — | — | — |
| 20 | not run | — | — | — | — | — |
| 100 | not run | — | — | — | — | — |

## Acceptance checks

- The captured message nonce appears on every intended receiver and on no
  removed or pre-join receiver.
- Counters are reset or attributed to the run before each cohort; unrelated
  background PUTs and grant refreshes are excluded with their evidence kept.
- The 100-member run includes the generated manifest itself in the byte and
  row measurements.
- Compare every populated row with
  [`osl-spaces-fanout-arithmetic.md`](osl-spaces-fanout-arithmetic.md). If the
  measurement disagrees, update the limits and downstream B1/B5/G9 decisions;
  do not round the observation toward the model.

This task cannot be marked measured in a Linux authoring checkout: it needs a
real configured store and the completed three-machine history/removal harness
run. Until the table contains those captures, it is deliberately not evidence
for a production fan-out cap.
