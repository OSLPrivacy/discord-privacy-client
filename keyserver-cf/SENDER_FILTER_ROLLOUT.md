# Sender-filter rollout compatibility contract

Status: shipping source closure plus source/test-only admission receipts. The
Rust client/broker boundary performs the ordinary read-only health and inbox
GETs described below. This document and
`scripts/sender-filter-rollout-contract.mjs` perform no HTTP, D1, migration,
deployment, or secret operation. Every plan still has
`direct_deploy_permitted=false` and `execution_authorized=false`.

## Version-skew matrix

“Legacy” means the byte-identical request without `?sender=`. “Filtered” means
the new client signs the sender component, requires the exact echo, and refuses
cross-sender rows. “Refuse” means no response is accepted as inbox content.

| Schema | Worker | Legacy client | Sender-filter client | Capability |
| --- | --- | --- | --- | --- |
| pre-0031 | legacy | legacy drain | probed legacy drain, narrowed locally | absent |
| pre-0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| pre-0031 | Artifact B | refuse (503) | refuse at capability probe | HTTP 503 / 0 |
| 0031 | legacy | legacy drain | probed legacy drain, narrowed locally | absent |
| 0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| 0031 | Artifact B | legacy drain | signed filtered drain | exactly 1 |

The executable matrix expands this table to all 12
schema × Worker × client rows and requires a nonempty continuity fixture.

## Request/response rules

- A missing sender is the old request. Legacy and final Workers preserve its
  canonical bytes and return an unfiltered page.
- On Artifact B, a present malformed sender is 400. On the historical Worker,
  a filtered signature (malformed or otherwise) is 401; an unknown sender
  query appended to a valid legacy signature is ignored and returns the legacy
  page. The executable model preserves that historical distinction.
- Adding, stripping, or substituting `sender` changes the canonical signed
  bytes and is 401.
- A legacy Worker reconstructs the old canonical bytes, so a new filtered
  signature is 401 rather than an ignored filter.
- The shipping Rust client—not its broker caller—GETs `/v1/healthz`. It uses
  the legacy request only for the exact historical `{ok:true}` response while
  capability 1 has never been observed. Artifact A, a 503/0 bridge, malformed
  health, and wrong capability versions refuse.
- Before the first filtered GET, the client writes a fixed-path, write-once,
  identity-signed capability-floor receipt inside the active account. A fresh
  client process verifies that durable receipt. Once capability 1 is observed,
  its disappearance is a downgrade refusal; the broker has no boolean, probe,
  path, reset, or fallback parameter.
- A filtered response is accepted only with the exact echo and no foreign
  sender. An empty authenticated filtered response is not mislabeled
  starvation: a real client has no omniscient view of undisclosed server rows.
  Positive reachability belongs to a bound phase receipt, not runtime guesses.

An old client can still encounter the pre-existing 64-row head-of-line
limitation because it cannot express a sender filter. The rollout does not
remove or starve its legacy inbox route: the admission plan requires a
nonempty legacy continuity probe. It does not falsely claim the old client can
reach a selected sender behind 64 older foreign rows. Artifact B plus a new
client proves that positive separately.

## Ordering and admission

The source-only planner records selection labels, never actions:

1. **Client-first:** shipping-compatible. A new client probes the legacy
   Worker, sends the byte-identical legacy request, and narrows that returned
   page locally before exposing it to the active peer broker. This preserves
   legacy availability but does not claim to fix the old 64-row page limit.
2. **Migration-first with the legacy Worker:** endpoint-compatible because
   0031 is additive; legacy drains remain nonempty. This fact is not migration
   authorization.
3. **Worker-first Artifact B:** refused. Before 0031, its routes and health
   return 503, so the planner never selects this order under active traffic.
4. **Artifact A:** all inbox routes return 503. The planner refuses A while
   traffic is active and can mention it only after traffic is independently
   quiesced.
5. **Final Artifact B:** compatible only with schema 0031, capability exactly
   1, a nonempty legacy continuity probe, a nonempty exact-echo filtered probe,
   and zero cross-sender rows.

Even a stable-compatible plan is not deployment admission. The package
production deploy and migration commands remain separate unconditional
refusals.

## Phase receipts and source closure

The planner no longer accepts independent `worker`, `schema`, capability,
legacy-probe, filtered-probe, or prior-capability arguments. It consumes one
exact phase receipt for `legacy-pre-0031`, `legacy-0031`,
`artifact-a-pre-0031`, `artifact-a-0031`, `artifact-b-pre-0031`, or
`artifact-b-0031`. Every capability/inbox observation repeats and must match
the exact phase, Worker commit/version/deployment, and client commit. The
receipt also binds migration 0031, the semantic source-closure digest, exact
shipping call-site identifiers, capture time, and its recomputed receipt hash.
Missing, empty, stale, cross-phase, leaky, or source-mismatched receipts refuse.

The closure spans migration 0031, Worker routing/health/filter/canonical
functions, Artifact A refusals, the Rust capability probe and signed durable
floor, and `broker.rs::fetch_peer_control_inbox`. Checks operate on executable
tokens with comments and string contents removed, plus normalized SQL, so
moving required call text into a comment or string cannot satisfy the gate.
These receipts remain source/test-only and unsigned; they describe compatible
selection labels and can never authorize a production action. Live admission
still requires the independently trusted producer boundary.
