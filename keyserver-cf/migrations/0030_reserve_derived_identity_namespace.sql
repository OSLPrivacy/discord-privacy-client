-- 0030: reserve the derived identity namespace.
--
-- Existing rows are scheme 0 by definition. Scheme 1 is deliberately
-- UNAVAILABLE in this preparatory migration: no deployed client emits a root
-- proof and no Worker verifies one yet.
--
-- DEPLOY ORDER IS SECURITY-CRITICAL:
--   1. deploy the Worker that refuses the reserved `osl1_...` namespace;
--   2. verify that exact Worker artifact through the release lane;
--   3. apply this migration.
--
-- Applying this migration first is schema-compatible with the old Worker, but
-- it does NOT reserve the namespace: the old Worker can still insert an
-- `osl1_...` id and both new columns take their scheme-0 defaults. The refusal
-- must therefore be live before this DDL is applied. Never roll back to a
-- Worker that predates that refusal.

ALTER TABLE users ADD COLUMN identity_scheme INTEGER NOT NULL DEFAULT 0
  CHECK (identity_scheme IN (0, 1));

ALTER TABLE users ADD COLUMN ik_root_ed25519_pub TEXT;

-- Defense in depth at the D1 boundary. A request field is not proof, and a
-- future Worker bug must not be able to promote a row merely by setting the
-- flag or storing attacker-supplied root bytes. Until a later migration lands
-- together with the reviewed root-proof verifier, the only valid durable state
-- is exactly `(scheme 0, NULL root)`.
CREATE TRIGGER users_identity_scheme_insert_guard
BEFORE INSERT ON users
WHEN NEW.identity_scheme <> 0 OR NEW.ik_root_ed25519_pub IS NOT NULL
BEGIN
  SELECT RAISE(ABORT, 'derived identity scheme requires verified root proof');
END;

CREATE TRIGGER users_identity_scheme_update_guard
BEFORE UPDATE OF identity_scheme, ik_root_ed25519_pub ON users
WHEN NEW.identity_scheme <> 0 OR NEW.ik_root_ed25519_pub IS NOT NULL
BEGIN
  SELECT RAISE(ABORT, 'derived identity scheme requires verified root proof');
END;
