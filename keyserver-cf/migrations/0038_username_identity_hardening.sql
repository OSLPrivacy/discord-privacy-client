-- 0038 · EXPAND half of the username identity hardening.  D-175.
--
-- SPEC, UNCHANGED AND IN FULL:
--   A handle is an identity assertion, not a reusable label.  Keep its
--   normalized spelling and UTS #39 skeleton after every release path.
--
-- WHY THIS FILE IS ONLY HALF.  The original single-step form of this migration
-- was measured to break the Worker running in production, and deploying the
-- consuming Worker first was measured to break that Worker:
--
--   migrate first -> the deployed Worker writes no username_skeleton, so with
--                    `NOT NULL DEFAULT ''` every claim lands the same `''`
--                    skeleton: the SECOND claim ever collides on the unique
--                    index, and after any rename the tombstone holds a `''`
--                    skeleton so EVERY later claim aborts.  That is the
--                    shipping app's onboarding path (apps/osl-hub/src/main.rs:903).
--   deploy first  -> 12 statements of the consuming Worker cannot even prepare
--                    against the pre-migration schema, and `no such column` is
--                    rethrown rather than mapped to a 409, so every username
--                    claim becomes a 500.
--
-- Both orders are outages, so the pair is split expand/contract.  This file is
-- the EXPAND half: additive only, and safe to apply while the pre-hardening
-- Worker is serving.  Applying it alone leaves production working.
--
-- NOTHING IS DROPPED.  Exactly three constraints move to the CONTRACT half,
-- `migrations-contract/0100_username_identity_contract.sql`, which is applied
-- only after the consuming Worker is deployed:
--
--   1. `username_skeleton` / `display_username` are MANDATORY.
--      Here they are nullable; the contract half installs insert/update guards.
--      Nullable is load-bearing, not laziness: SQLite treats NULLs as distinct
--      in a UNIQUE index, so rows written by the pre-hardening Worker cannot
--      collide with each other.  `NOT NULL DEFAULT ''` is what made them collide.
--   2. `username_tombstones.skeleton` is MANDATORY.
--      Here it is nullable because the consuming Worker retires a directory row
--      by inserting `username_skeleton` verbatim (src/lib/db.ts:161-162 and
--      :262-263, no COALESCE), so a NULL-skeleton row written by the older
--      Worker during the window would abort its whole rotate/unregister batch.
--      The contract half installs the guard once the backfill has removed them.
--   3. Retired-name rejection for a writer that supplies NO skeleton.
--      Here that case is a silent no-op (`RAISE(IGNORE)`), which the deployed
--      Worker reports as a 409 "username is unavailable" — the correct answer.
--      An `ABORT` would be rethrown by that Worker's catch, which matches only
--      /username_directory\.username|UNIQUE|PRIMARY/, and surface as a 500.
--      The contract half makes the rejection an unconditional ABORT.
--
-- Every other guarantee in the original migration is live from here onward:
-- the skeleton unique index, the tombstone table, retirement on every release
-- path, and retired-name rejection for the consuming Worker.

ALTER TABLE username_directory ADD COLUMN username_skeleton TEXT;
ALTER TABLE username_directory ADD COLUMN display_username TEXT;

-- D-248 CORRECTION.  This statement used to also set
--   username_skeleton = COALESCE(username_skeleton, username)
-- under the claim that "rows predating the identity-hardening writer used the
-- ASCII-only grammar, for which the normalized spelling is also the skeleton".
-- THAT CLAIM IS FALSE, and measurably so on pure ASCII: the pinned UTS #39
-- artifact folds `michael` to `rnichael`, `paypa1` to `paypal` and `supp0rt` to
-- `support`.  Writing the raw name into the skeleton column makes the unique
-- index below decorative — a skeleton equal to the raw name can never collide
-- with anything the raw name did not already collide with on the primary key.
--
-- A UTS #39 skeleton cannot be computed in SQLite, so this file does not
-- pretend to.  Legacy rows keep a NULL skeleton — which is safe here, because
-- SQLite treats NULLs as distinct in a UNIQUE index and the columns are
-- nullable for exactly the length of the expand window — and the backfill is
-- performed by `scripts/backfill-username-skeletons.mjs`, which computes each
-- skeleton with the same artifact the Worker uses and REFUSES, printing the
-- whole set, if two live rows fold together.  `migrations-contract/0100`'s
-- guard is what stops the contract half landing before that has been done.
--
-- display_username is not derived and is filled here as before.
UPDATE username_directory
   SET display_username = COALESCE(display_username, username)
 WHERE display_username IS NULL;

CREATE UNIQUE INDEX idx_username_directory_skeleton
  ON username_directory(username_skeleton);

CREATE TABLE username_tombstones (
  username TEXT PRIMARY KEY,
  -- MANDATORY in the contract half; see note 2 above.
  skeleton TEXT UNIQUE,
  retired_at TEXT NOT NULL
) WITHOUT ROWID;

-- This trigger is deliberately the final guard for all three release paths:
-- rename, key rotation, and unregister each delete a directory row.  It also
-- protects future maintenance code from accidentally making a name reusable.
-- COALESCE is the expand-half addition: a row written by a Worker that sets no
-- skeleton is still retired, under the same rule the backfill above uses.
CREATE TRIGGER username_directory_retire_before_delete
BEFORE DELETE ON username_directory
BEGIN
  INSERT OR IGNORE INTO username_tombstones (username, skeleton, retired_at)
  VALUES (OLD.username,
          COALESCE(OLD.username_skeleton, OLD.username),
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

-- A claim must not revive either the spelling or a confusable spelling of a
-- retired identity.  The K2 writer supplies username_skeleton on new claims.
CREATE TRIGGER username_directory_reject_retired_before_insert
BEFORE INSERT ON username_directory
WHEN NEW.username_skeleton IS NOT NULL
 AND EXISTS (
  SELECT 1 FROM username_tombstones
   WHERE username = NEW.username OR skeleton = NEW.username_skeleton
)
BEGIN
  SELECT RAISE(ABORT, 'username is retired');
END;

-- The same rule for a writer that supplies no skeleton, expressed as a silent
-- no-op so that Worker reports 409 rather than 500 (note 3 above).  The claim
-- is still refused: the row is not written and the caller is told the name is
-- unavailable.  Dropped by the contract half, which no longer needs it.
CREATE TRIGGER username_directory_reject_retired_legacy_writer
BEFORE INSERT ON username_directory
WHEN NEW.username_skeleton IS NULL
 AND EXISTS (
  SELECT 1 FROM username_tombstones WHERE username = NEW.username
)
BEGIN
  SELECT RAISE(IGNORE);
END;

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('username_identity_hardening_expand', 1);
