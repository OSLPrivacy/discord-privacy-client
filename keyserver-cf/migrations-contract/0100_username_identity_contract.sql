-- 0100 · CONTRACT half of the username identity hardening.  D-175.
--
-- Carries forward, unchanged, the three constraints the expand half
-- (`migrations/0038_username_identity_hardening.sql`) had to defer:
--
--   1. `username_skeleton` / `display_username` mandatory on username_directory
--      (was `NOT NULL DEFAULT ''` in the original single-step migration)
--   2. `username_tombstones.skeleton` mandatory (was `NOT NULL`)
--   3. retired-name rejection is an unconditional ABORT for every writer
--
-- ⛔ APPLY ONLY AFTER the consuming Worker is deployed and has taken 100% of
-- traffic.  This file forbids write shapes an older Worker generation performs.
-- It is applied from its own directory, on purpose, because
-- `wrangler d1 migrations apply` has no subset selection and would otherwise
-- drag this file into the expand batch:
--
--   npx wrangler d1 migrations apply osl-keyserver-prod --remote \
--     --config wrangler.contract.toml
--
-- THE GUARDS COME FIRST, BEFORE ANY DDL, so a refusal changes nothing.
--
-- Guard A refuses if the expand half has not been applied: `username_skeleton`
-- does not exist and the statement fails with `no such column`.
--
-- Guard B refuses if any directory row carries no skeleton.  Such a row is
-- direct evidence that a pre-expand Worker wrote AFTER the expand migration,
-- i.e. that this contract is being applied too early.  It is deliberately not
-- self-repairing: run the backfill (step 2 of the runbook), work out how the
-- row got there, then re-run this.
--
-- What guard B does NOT prove, stated plainly: it detects a pre-expand writer
-- only if that writer has actually written since the expand.  A pre-expand
-- Worker that is live but idle leaves no trace.  The runbook pairs it with
-- `wrangler deployments list` showing 100% on the consuming version and a
-- settle wait, and neither of those is a proof either.

-- Each guard names its own constraint so the abort says which one fired:
--   `CHECK constraint failed: username_directory_rows_without_skeleton_must_be_zero`
CREATE TABLE _contract_guard_0100_directory (
  ok INTEGER PRIMARY KEY,
  CONSTRAINT username_directory_rows_without_skeleton_must_be_zero CHECK (ok = 0)
);
INSERT INTO _contract_guard_0100_directory (ok)
SELECT COUNT(*) FROM username_directory
 WHERE username_skeleton IS NULL OR display_username IS NULL;
DROP TABLE _contract_guard_0100_directory;

CREATE TABLE _contract_guard_0100_tombstones (
  ok INTEGER PRIMARY KEY,
  CONSTRAINT username_tombstones_rows_without_skeleton_must_be_zero CHECK (ok = 0)
);
INSERT INTO _contract_guard_0100_tombstones (ok)
SELECT COUNT(*) FROM username_tombstones WHERE skeleton IS NULL;
DROP TABLE _contract_guard_0100_tombstones;

-- 3. Retired-name rejection, unconditional.  The expand half's split into an
--    ABORT for skeleton writers and a silent no-op for non-skeleton writers
--    existed only to keep the pre-expand Worker on a 409; there is no such
--    writer once this file may be applied.
DROP TRIGGER username_directory_reject_retired_legacy_writer;
DROP TRIGGER username_directory_reject_retired_before_insert;

CREATE TRIGGER username_directory_reject_retired_before_insert
BEFORE INSERT ON username_directory
WHEN EXISTS (
  SELECT 1 FROM username_tombstones
   WHERE username = NEW.username OR skeleton = NEW.username_skeleton
)
BEGIN
  SELECT RAISE(ABORT, 'username is retired');
END;

-- 1. username_skeleton / display_username mandatory.  SQLite cannot alter a
--    column to NOT NULL without rebuilding the table, and rebuilding
--    username_directory would drop and recreate every dependent object; the
--    guards express the same rule with no rewrite.  The UPDATE guard is not
--    extra scope: the original `NOT NULL` covered updates too, and the claim
--    path's `ON CONFLICT(username) DO UPDATE` does not set either column, so
--    without it a NULL row could survive an update untouched.
CREATE TRIGGER username_directory_skeleton_required_insert
BEFORE INSERT ON username_directory
WHEN NEW.username_skeleton IS NULL OR NEW.display_username IS NULL
BEGIN
  SELECT RAISE(ABORT, 'username_directory.username_skeleton and display_username are required');
END;

CREATE TRIGGER username_directory_skeleton_required_update
BEFORE UPDATE ON username_directory
WHEN NEW.username_skeleton IS NULL OR NEW.display_username IS NULL
BEGIN
  SELECT RAISE(ABORT, 'username_directory.username_skeleton and display_username are required');
END;

-- 2. username_tombstones.skeleton mandatory.
CREATE TRIGGER username_tombstones_skeleton_required
BEFORE INSERT ON username_tombstones
WHEN NEW.skeleton IS NULL
BEGIN
  SELECT RAISE(ABORT, 'username_tombstones.skeleton is required');
END;

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('username_identity_hardening', 1);
