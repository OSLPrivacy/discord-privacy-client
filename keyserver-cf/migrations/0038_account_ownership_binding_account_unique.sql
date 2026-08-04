-- 0038 · EXPAND half: one immutable Discord-account commitment may belong to
-- one OSL owner.  D-175.
--
-- SPEC, UNCHANGED AND IN FULL:
--   The clear account identifier is never retained in D1.  New bindings must
--   carry its SHA-256 commitment, and the unique index makes the first admitted
--   owner win across distinct challenges and identity keys.
--
-- WHY THIS FILE IS ONLY HALF.  The original form replaced the insert guard with
-- one that makes `service_account_sha256` mandatory.  That is a CONTRACTING
-- change: it forbids a write shape an older Worker generation may still be
-- performing.  It moves to
-- `migrations-contract/0101_account_ownership_binding_contract.sql` and is
-- applied only after the consuming Worker is deployed.
--
-- Measured, and it is why this half is safe to apply at any time: the Worker
-- running in production (version 08df28ba, deployed 2026-07-31T08:52Z) does not
-- contain the string `account_ownership_proof_bindings` at all — 0 occurrences
-- in its downloaded bundle — and routes only `/v1/account-ownership/challenge`,
-- not the proof route.  It cannot write this table, so neither half of this
-- migration can break it.  Earlier verdicts to the contrary (D-166, D-167,
-- D-174) came from a hand-written harness that reconstructed a deployed write
-- path the running artifact does not have.
--
-- The unique index below is NOT deferred.  SQLite treats NULLs as distinct in a
-- UNIQUE index, so it cannot collide on rows carrying no commitment, and it is
-- fully effective for every row the consuming Worker writes, from here onward.
--
-- NOTHING IS DROPPED.  Exactly one constraint moves to the contract half:
--   1. `service_account_sha256` is MANDATORY on insert (the replacement guard).

ALTER TABLE account_ownership_proof_bindings
  ADD COLUMN service_account_sha256 TEXT
    CHECK (
      length(service_account_sha256) = 64
      AND service_account_sha256 NOT GLOB '*[^0-9a-f]*'
    );

CREATE UNIQUE INDEX idx_account_ownership_proof_bindings_service_account
  ON account_ownership_proof_bindings (service, service_account_sha256);

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('account_ownership_binding_account_unique_expand', 1);
