-- 0101 · CONTRACT half of the account-ownership binding hardening.  D-175.
--
-- Carries forward, verbatim, the one constraint the expand half
-- (`migrations/0038_account_ownership_binding_account_unique.sql`) deferred:
--
--   1. `service_account_sha256` is MANDATORY on insert.
--
-- The trigger body below is byte-identical to the one in the original
-- single-step migration; only its arrival time moved.
--
-- ⛔ APPLY ONLY AFTER the consuming Worker is deployed.  See the header of
-- `0100_username_identity_contract.sql` for how and why this directory is
-- applied separately.
--
-- Guard first, before any DDL.  A binding row without a commitment could only
-- have been written by a Worker generation that predates the column, and the
-- immutability triggers from 0037 make such a row unrepairable in place — it
-- can be neither updated nor deleted.  So this refuses and hands the decision
-- to a human rather than pretending it can fix it.  Expected count today: 0
-- (`account_ownership_proof_bindings` is empty in production, measured
-- 2026-08-04, and the deployed Worker never writes it).

CREATE TABLE _contract_guard_0101 (
  ok INTEGER PRIMARY KEY,
  CONSTRAINT account_ownership_bindings_without_commitment_must_be_zero CHECK (ok = 0)
);
INSERT INTO _contract_guard_0101 (ok)
SELECT COUNT(*) FROM account_ownership_proof_bindings
 WHERE service_account_sha256 IS NULL;
DROP TABLE _contract_guard_0101;

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

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('account_ownership_binding_account_unique', 1);
