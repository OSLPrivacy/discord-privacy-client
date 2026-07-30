# Store monotonic anchored migration protocol

> **Design only — not implemented.** This document specifies the protocol
> required before an externally anchored Store migration can claim rollback
> detection. `MessageStore::open_anchored` currently has no platform/TPM/
> keystore provider wiring, and this design does not claim full-database or
> OS-root rollback protection.

## Purpose

An anchored database must verify its old schema generation before a migration
changes any anchored metadata. Reordering ordinary `migrate()` and anchor
enrolment is insufficient: migrations can have multiple local transactions and
post-transaction `VACUUM` work. Every durable, resumable state must therefore
be both locally recorded and externally compare-and-advanced.

## Canonical state

`_meta[anchor_migration_v1]` holds one canonical, length-delimited journal:

- protocol version;
- domain-separated Store identity;
- source and target schema versions;
- a nonce generated while the prior state is write-locked;
- a domain-separated plan digest; and
- a phase: `Prepared`, `Step { completed_schema }`, or `Vacuum { schema }`.

The plan digest is keyed and domain-separated as
`osl-store-anchor/migration-plan/v1`; it commits to protocol, source/target,
ordered step identities, and immutable implementation identifiers. The journal
is itself included in the anchor digest. Unknown fields, malformed lengths,
wrong store identity, changed target or plan, invalid phase/version relations,
and partial records refuse to open.

The anchored digest projection must be versioned and validate the exact schema
shape before hashing. It includes schema/version fingerprints, all relevant
`_meta` values (except the local anchor generation/digest bookkeeping), and
canonically ordered messages, attachments, and manifests. It intentionally
does not hash raw SQLite/WAL/SHM pages, free pages, filesystem paths, backups,
or external staging.

## State machine

`R(v)` means ready at schema `v`, with no journal. `P` is prepared, `S(k)` is
a committed migration step through schema `k`, and `V(k)` is a step that has an
anchored pending-vacuum marker.

1. Inspect version and schema shape read-only; check the canary.
2. For an existing anchor, reconcile the old local digest with the provider
   before any schema, metadata, pragma, or data mutation.
3. From verified `R(v)`, write `Prepared` in one local transaction and
   compare-and-advance that exact state externally.
4. Each `v -> k` schema hook runs transaction-only. It updates the journal
   phase in the same transaction passed to the anchored committer, then CASes
   the provider before another step can begin.
5. A sensitive-page-reclaiming step tags `Vacuum` before the physical vacuum.
   Vacuum runs only after that tag is externally advanced; the matching tag is
   cleared in a subsequent locally committed and externally advanced state.
6. After verified target shape, delete the exact journal in one local
   transaction and compare-and-advance the final ready digest before success.

Before every transition, acquire the SQLite write lock, reread the local anchor
record and provider state, and recompute the projected digest. The only repair
case is local state exactly one generation ahead of the provider with the same
pending transition: retry that exact CAS. Any other disagreement refuses.

## Enrollment distinction

- **Already anchored v7:** local and provider records must match the v7
  projection before creating `Prepared`; a replayed pre-migration v7 database
  is then detectable.
- **Unenrolled v7:** ordinary compatibility migration remains allowed. It may
  enrol only after current-schema completion and must never present that as an
  old v7 anchor.
- Provider/local asymmetry, wrong Store identity, a stale journal, or a
  changed plan are refusal states, not local fallbacks.

## Restart and adversary requirements

The implementation needs fault injection for: prepared local commit before
CAS; prepared CAS before the first schema hook; every step local commit before
CAS; each CAS before the next operation; vacuum before/after physical work;
final local journal clear before CAS; and final CAS before returning. Repeated
restart must preserve one nonce/plan/phase without adding transitions.

Tests must also cover stale pre-migration and stale pending replay, wrong-store
provider state, altered plan/target, and a second writer during recovery. A
disposable mutation restoring migration-before-reconcile must fail the test
that observes and verifies the old anchored v7 digest before any migration
write. All negative tests require a nonempty successful v7-to-target migration
and reopen control so constant refusal, an empty database, or no-op migration
cannot pass.

An exactly one-generation provider rollback is indistinguishable from a crash
after local commit and before CAS with the current provider contract. The
protocol therefore depends on a genuinely durable monotonic provider; it does
not manufacture monotonicity from a second local file.

## Minimal ownership

- `crates/store/src/anchor.rs`: projection, journal codec, transition and
  reconciliation executor.
- `crates/store/src/schema.rs`: read-only schema inspection, exact shape
  validation, transaction-only migration hooks, and tagged vacuum helpers.
- `crates/store/src/lib.rs`: separate ordinary/anchored open paths; anchored
  inspection and reconciliation precede all mutation.
- `crates/store/tests/monotonic_anchor_test.rs`: crash, restart, replay,
  wrong-store, plan-mismatch, and concurrent-writer matrix.

Platform/TPM/keystore provider wiring remains external and intentionally
unwired.
