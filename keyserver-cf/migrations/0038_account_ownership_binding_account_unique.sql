-- 0038: one immutable Discord-account commitment may belong to one OSL owner.
--
-- The clear account identifier is never retained in D1.  New bindings must
-- carry its SHA-256 commitment, and the unique index makes the first admitted
-- owner win across distinct challenges and identity keys.

ALTER TABLE account_ownership_proof_bindings
  ADD COLUMN service_account_sha256 TEXT
    CHECK (
      length(service_account_sha256) = 64
      AND service_account_sha256 NOT GLOB '*[^0-9a-f]*'
    );

CREATE UNIQUE INDEX idx_account_ownership_proof_bindings_service_account
  ON account_ownership_proof_bindings (service, service_account_sha256);

DROP TRIGGER account_ownership_proof_bindings_insert_guard;

CREATE TRIGGER account_ownership_proof_bindings_insert_guard
BEFORE INSERT ON account_ownership_proof_bindings
WHEN NOT (
  NEW.service_account_sha256 IS NOT NULL
  AND length(NEW.service_account_sha256) = 64
  AND NEW.service_account_sha256 NOT GLOB '*[^0-9a-f]*'
  AND EXISTS (
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
