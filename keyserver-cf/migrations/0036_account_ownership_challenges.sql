-- 0036: short-lived account-ownership proof challenges.
--
-- The Worker issues a nonce bound to one claimed platform account and one OSL
-- owner identity. The verifier lands later; this table is already shaped for
-- that verifier by storing only nonce/binding commitments and single-use state.
-- Clear Discord account IDs and OSL owner IDs are not retained in D1.

CREATE TABLE account_ownership_challenges (
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
  service TEXT NOT NULL CHECK (service = 'discord'),
  issued_at_unix_seconds INTEGER NOT NULL CHECK (issued_at_unix_seconds >= 0),
  expires_at_unix_seconds INTEGER NOT NULL
    CHECK (expires_at_unix_seconds > issued_at_unix_seconds),
  spent_at_unix_seconds INTEGER
    CHECK (
      spent_at_unix_seconds IS NULL
      OR spent_at_unix_seconds >= issued_at_unix_seconds
    )
) WITHOUT ROWID;

CREATE INDEX idx_account_ownership_challenges_expires_at
  ON account_ownership_challenges (expires_at_unix_seconds);

CREATE TRIGGER account_ownership_challenges_binding_immutable
BEFORE UPDATE OF
  nonce_sha256,
  binding_sha256,
  service,
  issued_at_unix_seconds,
  expires_at_unix_seconds
ON account_ownership_challenges
BEGIN
  SELECT RAISE(ABORT, 'account ownership challenge binding is immutable');
END;

CREATE TRIGGER account_ownership_challenges_spent_once
BEFORE UPDATE OF spent_at_unix_seconds
ON account_ownership_challenges
WHEN OLD.spent_at_unix_seconds IS NOT NULL
  OR NEW.spent_at_unix_seconds IS NULL
  OR NEW.spent_at_unix_seconds < OLD.issued_at_unix_seconds
BEGIN
  SELECT RAISE(ABORT, 'account ownership challenge already spent');
END;
