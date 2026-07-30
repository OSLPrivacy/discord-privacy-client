-- 0010: continuous recovery authorization for unlineaged `completing` rows.
--
-- DEPLOY ORDER. Apply this migration after 0009 and before the matching
-- Worker. The previous Worker ignores this table. The matching Worker reads
-- and validates it before any R2 observation or metadata/quota mutation, so a
-- missing or partial migration fails closed.
--
-- Migration 0009 bounded predecessor adoption to a one-hour creation window.
-- A predecessor Worker that survives longer than that window can still leave
-- a `completing` row without claim lineage. Such a row is otherwise
-- permanently excluded by the completion/sweep fence.
--
-- This marker authorizes recovery of any `completing` row that has no claim
-- row at all. It does not authorize taking an active or expired lineaged
-- claim: those continue through the existing exact worker/token/version CAS.
-- Work remains bounded to the ordinary 100-claim scheduled cycle.

CREATE TABLE attachment_predecessor_recovery (
  singleton            INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
  format               TEXT    NOT NULL
                               CHECK(format = 'osl.cipher-store.continuous-predecessor-recovery.v1'),
  max_claims_per_cycle INTEGER NOT NULL CHECK(max_claims_per_cycle = 100)
) STRICT;

INSERT INTO attachment_predecessor_recovery
  (singleton, format, max_claims_per_cycle)
VALUES
  (1, 'osl.cipher-store.continuous-predecessor-recovery.v1', 100);
