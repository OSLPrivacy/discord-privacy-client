# Sender-filter rollout compatibility contract

Status: source/test-only. This document and
`scripts/sender-filter-rollout-contract.mjs` perform no HTTP, D1, migration,
deployment, or secret operation. Every plan has
`direct_deploy_permitted=false` and `execution_authorized=false`.

## Version-skew matrix

“Legacy” means the byte-identical request without `?sender=`. “Filtered” means
the new client signs the sender component, requires the exact echo, and refuses
cross-sender rows. “Refuse” means no response is accepted as inbox content.

| Schema | Worker | Legacy client | Sender-filter client | Capability |
| --- | --- | --- | --- | --- |
| pre-0031 | legacy | legacy drain | legacy fallback | absent |
| pre-0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| pre-0031 | Artifact B | refuse (503) | refuse at capability probe | HTTP 503 / 0 |
| 0031 | legacy | legacy drain | legacy fallback | absent |
| 0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| 0031 | Artifact B | legacy drain | signed filtered drain | exactly 1 |

The executable matrix expands this table to all 12
schema × Worker × client rows and requires a nonempty continuity fixture.

## Request/response rules

- A missing sender is the old request. Legacy and final Workers preserve its
  canonical bytes and return an unfiltered page.
- A present malformed sender is 400. It is never silently treated as missing.
- Adding, stripping, or substituting `sender` changes the canonical signed
  bytes and is 401.
- A legacy Worker reconstructs the old canonical bytes, so a new filtered
  signature is 401 rather than an ignored filter.
- A new client uses the legacy request while capability 1 has never been
  observed. Once capability 1 is observed, its disappearance is a downgrade
  refusal.
- A filtered response is accepted only with the exact echo, no foreign sender,
  and a nonempty result when the positive fixture says a selected-sender row
  exists.

An old client can still encounter the pre-existing 64-row head-of-line
limitation because it cannot express a sender filter. The rollout does not
remove or starve its legacy inbox route: the admission plan requires a
nonempty legacy continuity probe. It does not falsely claim the old client can
reach a selected sender behind 64 older foreign rows. Artifact B plus a new
client proves that positive separately.

## Ordering and admission

The source-only planner records selection labels, never actions:

1. **Client-first:** safe. A new client sees no capability on the legacy
   Worker and sends the byte-identical legacy request.
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
