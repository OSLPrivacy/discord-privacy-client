# Space fan-out: live-store measurement record

Status: **unmeasured — not a capacity input** (2026-08-02).

T21-K4 requires observations from the deployed store at N = 5, 20, and 100.
This repository does not contain a reachable real-store endpoint, store
credentials, or a captured result bundle. Therefore this document deliberately
does not turn the arithmetic estimate into reported measurements. In
particular, it does not change B1, B5, G9, or any cap.

## Required run

Use a fresh Space for each row, send exactly one uniquely tagged protected
message from a member, and snapshot the real store after all fan-out writes
have settled. Repeat each cohort three times and record the median; the
collector must record the following values from that snapshot, not from client
side estimates:

| Members (N) | Rows | Total bytes | PUTs | Grants minted | Manifest bytes | Evidence bundle |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 5 | not measured | not measured | not measured | not measured | not measured | absent |
| 20 | not measured | not measured | not measured | not measured | not measured | absent |
| 100 | not measured | not measured | not measured | not measured | not measured | absent |

For every row, retain the request nonce, Space ID, raw store listing, PUT audit
records, grant records, manifest object bytes, collection timestamps, deployed
store revision, and the tool version used to collect it. The evidence bundle
must identify one run per N and be immutable enough for a second operator to
recount rows and bytes. Counters must be reset or attributed to the run so
background PUTs and grant refreshes are excluded with their evidence retained.

## Acceptance and follow-up

The measurement is accepted only when every table value is traceable to the
same real-store run and the evidence is reviewed independently. If any measured
value differs from the planning assumptions in
[`osl-spaces-fanout-arithmetic.md`](osl-spaces-fanout-arithmetic.md), update
that document and reopen the dependent B1/G9 capacity decisions. Until then,
the arithmetic document remains a planning model, not a deployment claim.

## Why this is blocked

No real-store target or credentials are available in this checkout, and the
task forbids treating a model as a measurement. Supplying numeric values here
would be fabricated evidence. A QA operator with the deployed store authority
must perform the required run and replace only the `not measured` cells with
the captured values and evidence locations.
