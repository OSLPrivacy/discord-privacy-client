-- 0009: bounded adoption for predecessor-created `completing` rows.
--
-- DEPLOY ORDER. Apply this migration before the matching Worker. The 0008
-- Worker ignores these additive columns and tables. The matching Worker names
-- every new field before touching R2, so an absent or partial 0009 schema
-- refuses the whole attachment sweep before storage or quota metadata moves.
--
-- During a rolling deployment the predecessor Worker can leave a multipart
-- object in `completing` without a 0008 claim row. Only rows created during the
-- fixed one-hour migration window may enter the new adoption path. New Worker
-- completions always create claim lineage first. Existing claims, including
-- active completion leases, never use this cutoff and retain their original
-- token/version lease rules.
--
-- `storage_fence_state = 'object_absent_confirmed'` is written only after a
-- successful multipart abort and an empty post-abort HEAD (or after a
-- mismatched completed object is deleted and absence is re-observed). It is
-- preserved across lease recovery and failure backoff. A crash therefore
-- keeps the attachment quota reserved until an exact later claim/version CAS
-- removes metadata; it never turns an ambiguous R2 result into silent loss.

ALTER TABLE attachment_sweep_claims
  ADD COLUMN claim_origin TEXT NOT NULL DEFAULT 'lineaged'
    CHECK(claim_origin IN (
      'lineaged',
      'sweep',
      'completion',
      'predecessor_adoption'
    ));

ALTER TABLE attachment_sweep_claims
  ADD COLUMN storage_fence_state TEXT NOT NULL DEFAULT 'pending'
    CHECK(storage_fence_state IN ('pending', 'object_absent_confirmed'));

CREATE TABLE attachment_predecessor_adoption (
  singleton                INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
  format                   TEXT    NOT NULL
                                  CHECK(format = 'osl.cipher-store.predecessor-adoption.v1'),
  migration_started_at     INTEGER NOT NULL CHECK(migration_started_at > 0),
  eligible_created_through INTEGER NOT NULL
                                  CHECK(eligible_created_through > migration_started_at),
  max_claims_per_cycle     INTEGER NOT NULL CHECK(max_claims_per_cycle = 100)
) STRICT;

INSERT INTO attachment_predecessor_adoption
  (singleton, format, migration_started_at, eligible_created_through,
   max_claims_per_cycle)
VALUES
  (1, 'osl.cipher-store.predecessor-adoption.v1',
   unixepoch(), unixepoch() + 3600, 100);
