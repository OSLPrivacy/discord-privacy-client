-- 0045: a short account-ownership proof can claim exactly one public name.
--
-- The clear Discord account id is not retained. The nonce is the primary key,
-- so the same account-ownership proof cannot be replayed against a second
-- username. The Worker spends the matching challenge in the same batch that
-- records this receipt and writes the username row.

CREATE TABLE public_name_proofs (
  nonce_sha256 TEXT PRIMARY KEY
    CHECK (
      length(nonce_sha256) = 64
      AND nonce_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  binding_sha256 TEXT NOT NULL UNIQUE
    CHECK (
      length(binding_sha256) = 64
      AND binding_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  owner_user_id TEXT NOT NULL,
  service TEXT NOT NULL CHECK (service = 'discord'),
  service_account_sha256 TEXT NOT NULL
    CHECK (
      length(service_account_sha256) = 64
      AND service_account_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  username TEXT NOT NULL,
  claimed_at_unix_seconds INTEGER NOT NULL
    CHECK (claimed_at_unix_seconds > 0),
  expires_at_unix_seconds INTEGER NOT NULL
    CHECK (expires_at_unix_seconds > claimed_at_unix_seconds),
  FOREIGN KEY (owner_user_id) REFERENCES users (user_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX idx_public_name_proofs_owner
  ON public_name_proofs (owner_user_id, service);

CREATE INDEX idx_public_name_proofs_expiry
  ON public_name_proofs (expires_at_unix_seconds);

CREATE TRIGGER public_name_proofs_insert_guard
BEFORE INSERT ON public_name_proofs
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
       AND challenge.spent_at_unix_seconds IS NULL
       AND NEW.claimed_at_unix_seconds >= challenge.issued_at_unix_seconds
       AND NEW.claimed_at_unix_seconds < challenge.expires_at_unix_seconds
       AND NEW.expires_at_unix_seconds = challenge.expires_at_unix_seconds
  )
)
BEGIN
  SELECT RAISE(ABORT, 'public-name proof requires a fresh unspent account proof challenge');
END;

CREATE TRIGGER public_name_proofs_no_update
BEFORE UPDATE ON public_name_proofs
BEGIN
  SELECT RAISE(ABORT, 'public-name proof receipt is immutable');
END;

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('public_name_proof_required', 1);
