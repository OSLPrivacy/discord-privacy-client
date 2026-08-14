-- Retired expand attempt for short-lived public-name proof tokens.
--
-- The live claim path uses the nonce-bound account-proof receipt created by
-- 0045_public_name_proofs.sql. Keeping this filename as an explicit no-op
-- prevents local migration application from creating the obsolete token-shaped
-- table before 0045 creates the schema the Worker actually verifies.

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('retired_token_public_name_proofs', 1);
