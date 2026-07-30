-- 0032: independently administered monotonic sender-filter capability floor.
--
-- This migration is inert for every already-deployed Worker. It creates no
-- route and changes no existing table. The successor Worker is the sole writer
-- and may insert version 1 only after independently rechecking migration 0031's
-- schema marker and columns.

CREATE TABLE sender_filter_capability_floors (
  identity_anchor_sha256 TEXT PRIMARY KEY
    CHECK (
      length(identity_anchor_sha256) = 64
      AND identity_anchor_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  capability_version INTEGER NOT NULL CHECK (capability_version = 1),
  monotonic_version INTEGER NOT NULL CHECK (monotonic_version = 1),
  first_observed_at_ms INTEGER NOT NULL CHECK (first_observed_at_ms > 0)
) WITHOUT ROWID;

-- The application has no reset API. These database guards also make an
-- accidental future DELETE or in-place rewrite fail closed. An independent D1
-- administrator can recover the database, but a signed client request cannot
-- lower, erase, or recreate genesis.
CREATE TRIGGER sender_filter_capability_floor_no_update
BEFORE UPDATE ON sender_filter_capability_floors
BEGIN
  SELECT RAISE(ABORT, 'sender-filter capability floor is immutable');
END;

CREATE TRIGGER sender_filter_capability_floor_no_delete
BEFORE DELETE ON sender_filter_capability_floors
BEGIN
  SELECT RAISE(ABORT, 'sender-filter capability floor cannot be deleted');
END;
