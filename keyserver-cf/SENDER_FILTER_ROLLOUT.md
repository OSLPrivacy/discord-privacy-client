# Sender-filter rollout compatibility contract

Status: shipping source closure plus fail-closed, internally bound
nonauthorizing admission receipt derivation. A production floor-genesis
boundary now exists in source as an identity-authenticated, migration-0031
derived, durable migration-0032 D1 record. It has not been migrated or deployed;
producer trust and transactional verifier storage also remain deliberately
unprovisioned, so this is test-proven-only `+0`. The
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
| pre-0031 | legacy | legacy drain | refuse: authority route absent | absent |
| pre-0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| pre-0031 | Artifact B | refuse (503) | refuse at capability probe | HTTP 503 / 0 |
| 0031 | legacy | legacy drain | refuse: authority route absent | absent |
| 0031 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| 0031 | Artifact B | legacy drain | refuse: 0032 authority absent | exactly 1 |
| 0031+0032 | legacy | legacy drain | refuse: authority route absent | absent |
| 0031+0032 | Artifact A | refuse (503) | refuse at bridge marker | absent/bridge |
| 0031+0032 | Artifact B | legacy drain | signed filtered drain | exactly 1 |

The executable matrix expands this table to all 18
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
- The shipping Rust client—not its broker caller—GETs `/v1/healthz` and then
  the signed `/v1/sender-filter-capability-floor/:user_id` authority route.
  Artifact A, a 503/0 bridge, the historical response, malformed health, and
  wrong capability versions refuse.
- The floor request carries the registered recipient identity, a fresh
  timestamp, and a signed random request id. The Worker verifies that exact
  request, independently rechecks migration 0031's marker and columns, and
  only then inserts the hard-coded version-1 identity anchor in migration
  0032's D1 table. The request supplies no capability, state path, prior floor,
  or genesis bit.
- Migration 0032 makes each floor row immutable and undeletable through D1
  statements. The response echoes the exact timestamp/request id and the Rust
  client recomputes the identity anchor, so stale, replayed, empty, wrong-key,
  or mismatched observations refuse. Local deletion, tamper, and process
  restart cannot recreate `NeverObserved` because no local floor exists.
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

1. **Client-first:** fail closed. A new client cannot obtain the D1 authority
   observation from a legacy Worker and performs no inbox drain. Existing
   legacy clients remain unaffected.
2. **Migration-first with the legacy Worker:** endpoint-compatible because
   0031 is additive; legacy drains remain nonempty. This fact is not migration
   authorization.
3. **Worker-first Artifact B:** refused. Before 0031, its routes and health
   return 503, so the planner never selects this order under active traffic.
4. **Artifact A:** all inbox routes return 503. The authenticated producer
   chain does not prove traffic quiescence, so this planner always refuses to
   advance from A.
5. **Final Artifact B:** source-compatible only with schema 0031 plus the
   inert-before-Worker 0032 authority table, capability exactly 1, a nonempty
   legacy continuity probe, a nonempty exact-echo filtered probe, and zero
   cross-sender rows. Current deployment-evidence admission does not yet prove
   0032 and therefore cannot authorize this phase.

Even a stable-compatible plan is not deployment admission. The package
production deploy and migration commands remain separate unconditional
refusals.

## Phase receipts and source closure

The planner accepts no caller-authored phase value and has no phase-receipt
constructor. Its API rejects caller-supplied trust registries and verifier
stores. It uses only the module-owned production producer registry and
transactional verifier binding; both are currently empty/unprovisioned, so
receipt derivation refuses. Once independently provisioned, it verifies the
trusted producer's Ed25519 deployment-evidence envelope, then requires that
exact receipt to be the consumed head of that independently administered
transactional verifier store.
The phase is derived from the authenticated Artifact A/B observation rather
than supplied by the caller. The derived view binds commit/archive, migrations
0031 and 0032, D1 identity/environment/schema fingerprint, Worker version/deployment,
sender-filter route observation, producer identity/sequence, verifier
administrator/database/monotonic version, the semantic source closure, and
exact shipping call-site identifiers. Caller-authored, unsigned, unconsumed,
stale, replayed/superseded, empty, leaky, or source-mismatched evidence
refuses.

The closure spans migrations 0031/0032, Worker routing/health/filter/floor
authority/canonical functions, Artifact A refusals, both Rust signed request
encodings, the fresh authority-response validator, and
`broker.rs::fetch_peer_control_inbox`. The gate parses Rust function boundaries
and requires the measured probe plus measured authority invocation to be the
live tail immediately feeding the filtered/refusal match. It rejects dead
measured blocks, unfiltered legacy-call filtering, locally invented floors,
and any production filesystem reset/delete primitive—including cast-through
function type aliases. Executable SQL parsing excludes line and block comments
and requires the D1 immutability/delete guards. The derived receipts remain
source/test-only and can never authorize a production action; the production
trust registry and verifier binding remain deliberately unprovisioned.
