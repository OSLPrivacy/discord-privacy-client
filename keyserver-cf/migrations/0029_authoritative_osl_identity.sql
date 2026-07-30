-- 0029: Discord snowflakes are not keyserver identities.
--
-- Open self-signed registration proves control of the submitted OSL
-- key, never control of a Discord account named by `user_id`. Existing
-- rows therefore start disabled. A non-platform OSL identity can
-- re-register with its current signing key to enable lookup; numeric
-- Discord snowflakes are refused by the worker and can never enable.
--
-- Migration BEFORE worker deployment. The worker filters every
-- identity-authenticated read through this column.

ALTER TABLE users
  ADD COLUMN identity_lookup_enabled INTEGER NOT NULL DEFAULT 0
  CHECK (identity_lookup_enabled IN (0, 1));

-- NOT DEPLOYED by this change. The owner must apply this migration,
-- then deploy the worker.
