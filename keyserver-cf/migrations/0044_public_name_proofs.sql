-- 0044: short-lived public-name proofs.
--
-- A public name claim is no longer authorized by the OSL identity signature
-- alone. The claim must also consume one short-lived proof minted for the exact
-- app account, OSL owner, and public name. The clear service account id is not
-- retained; the Worker recomputes its SHA-256 commitment from the request.

CREATE TABLE public_name_proofs (
  token_sha256 TEXT PRIMARY KEY
    CHECK (
      length(token_sha256) = 64
      AND token_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  service TEXT NOT NULL CHECK (service = 'discord'),
  service_account_sha256 TEXT NOT NULL
    CHECK (
      length(service_account_sha256) = 64
      AND service_account_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  owner_user_id TEXT NOT NULL,
  name TEXT NOT NULL,
  issued_at_unix_seconds INTEGER NOT NULL CHECK (issued_at_unix_seconds >= 0),
  expires_at_unix_seconds INTEGER NOT NULL
    CHECK (expires_at_unix_seconds > issued_at_unix_seconds),
  consumed_at_unix_seconds INTEGER
    CHECK (
      consumed_at_unix_seconds IS NULL
      OR consumed_at_unix_seconds >= issued_at_unix_seconds
    ),
  FOREIGN KEY (owner_user_id) REFERENCES users (user_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX idx_public_name_proofs_expiry
  ON public_name_proofs (expires_at_unix_seconds);

CREATE INDEX idx_public_name_proofs_owner_name
  ON public_name_proofs (owner_user_id, service, name);

CREATE TRIGGER public_name_proofs_binding_immutable
BEFORE UPDATE OF
  token_sha256,
  service,
  service_account_sha256,
  owner_user_id,
  name,
  issued_at_unix_seconds,
  expires_at_unix_seconds
ON public_name_proofs
BEGIN
  SELECT RAISE(ABORT, 'public name proof binding is immutable');
END;

CREATE TRIGGER public_name_proofs_spent_once
BEFORE UPDATE OF consumed_at_unix_seconds
ON public_name_proofs
WHEN OLD.consumed_at_unix_seconds IS NOT NULL
  OR NEW.consumed_at_unix_seconds IS NULL
  OR NEW.consumed_at_unix_seconds < OLD.issued_at_unix_seconds
BEGIN
  SELECT RAISE(ABORT, 'public name proof already consumed');
END;

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('public_name_proof_required', 1);
