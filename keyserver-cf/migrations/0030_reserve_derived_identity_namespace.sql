-- 0030: reserve the derived identity namespace.
--
-- Existing rows are scheme 0 by definition. Scheme 1 requires a verified
-- root proof and can never be reached by flipping a flag. This migration goes
-- BEFORE the worker so the namespace is reserved before any client can emit
-- derived identifiers.

ALTER TABLE users ADD COLUMN identity_scheme INTEGER NOT NULL DEFAULT 0
  CHECK (identity_scheme IN (0, 1));

ALTER TABLE users ADD COLUMN ik_root_ed25519_pub TEXT;
