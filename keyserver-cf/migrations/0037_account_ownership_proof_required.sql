-- 0036: require a consumed account-ownership challenge before recording a
-- durable platform-account proof binding.
--
-- The prerequisite challenge table stores only nonce/binding commitments. This
-- migration adds the durable proof-admission boundary: a platform-account
-- binding row cannot be written unless the exact challenge commitment has
-- already been spent while still fresh. Clear service account identifiers and
-- proof payloads stay out of D1.

CREATE TABLE account_ownership_proof_bindings (
  binding_sha256 TEXT PRIMARY KEY
    CHECK (
      length(binding_sha256) = 64
      AND binding_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  nonce_sha256 TEXT NOT NULL UNIQUE
    CHECK (
      length(nonce_sha256) = 64
      AND nonce_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  owner_user_id TEXT NOT NULL,
  service TEXT NOT NULL CHECK (service = 'discord'),
  proof_type TEXT NOT NULL
    CHECK (proof_type = 'ed25519_identity_challenge_v1'),
  verified_at_unix_seconds INTEGER NOT NULL
    CHECK (verified_at_unix_seconds > 0)
) WITHOUT ROWID;

CREATE INDEX idx_account_ownership_proof_bindings_owner
  ON account_ownership_proof_bindings (owner_user_id, service);

CREATE TRIGGER account_ownership_proof_bindings_insert_guard
BEFORE INSERT ON account_ownership_proof_bindings
WHEN NOT (
  EXISTS (
    SELECT 1
      FROM users
     WHERE user_id = NEW.owner_user_id
  )
  AND EXISTS (
    SELECT 1
      FROM account_ownership_challenges AS challenge
     WHERE challenge.nonce_sha256 = NEW.nonce_sha256
       AND challenge.binding_sha256 = NEW.binding_sha256
       AND challenge.service = NEW.service
       AND challenge.spent_at_unix_seconds IS NOT NULL
       AND challenge.spent_at_unix_seconds < challenge.expires_at_unix_seconds
       AND NEW.verified_at_unix_seconds >= challenge.spent_at_unix_seconds
       AND NEW.verified_at_unix_seconds < challenge.expires_at_unix_seconds
  )
)
BEGIN
  SELECT RAISE(ABORT, 'account ownership proof challenge is required');
END;

CREATE TRIGGER account_ownership_proof_bindings_no_update
BEFORE UPDATE ON account_ownership_proof_bindings
BEGIN
  SELECT RAISE(ABORT, 'account ownership proof binding is immutable');
END;

CREATE TRIGGER account_ownership_proof_bindings_no_delete
BEFORE DELETE ON account_ownership_proof_bindings
BEGIN
  SELECT RAISE(ABORT, 'account ownership proof binding is immutable');
END;

CREATE TABLE IF NOT EXISTS worker_schema_capabilities (
  capability TEXT PRIMARY KEY,
  version    INTEGER NOT NULL CHECK (version >= 1)
) WITHOUT ROWID;

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('account_ownership_proof_required', 1);
