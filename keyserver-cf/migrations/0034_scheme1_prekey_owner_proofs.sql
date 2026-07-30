-- 0034: scheme-1 prekey owner proofs and non-resettable lifecycle authority.
--
-- Migration BEFORE Worker is mandatory. The scheme-1 Worker selects these
-- columns and tables and therefore fails against an older schema. Applying the
-- migration alone is inert: existing scheme-0 rows retain generation 0 and
-- NULL proof fields, while scheme-1 replenish remains unavailable until the
-- matching Worker is activated.

ALTER TABLE opk_pool
  ADD COLUMN owner_proof_json TEXT;
ALTER TABLE opk_pool
  ADD COLUMN lifecycle_generation INTEGER NOT NULL DEFAULT 0
    CHECK (lifecycle_generation >= 0);
ALTER TABLE opk_pool
  ADD COLUMN batch_commitment_b64 TEXT;

-- Scheme 0 continues to insert the original four-column receipt shape and
-- receives these defaults. Scheme 1 records enough authenticated context to
-- return the original stable result after an ambiguous/lost response.
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN identity_scheme INTEGER NOT NULL DEFAULT 0
    CHECK (identity_scheme IN (0, 1));
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1
    CHECK (protocol_version IN (1, 2));
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN identity_revision INTEGER;
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN identity_bundle_commitment_b64 TEXT;
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN lifecycle_generation INTEGER;
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN batch_commitment_b64 TEXT;
ALTER TABLE prekey_replenish_receipts
  ADD COLUMN opks_added INTEGER;

CREATE TABLE prekey_lifecycle_authority (
  user_id TEXT PRIMARY KEY,
  ik_root_ed25519_pub TEXT NOT NULL,
  ik_ed25519_pub TEXT NOT NULL,
  identity_revision INTEGER NOT NULL CHECK (identity_revision >= 1),
  identity_bundle_commitment_b64 TEXT NOT NULL
    CHECK (length(identity_bundle_commitment_b64) = 44),
  rn_capabilities INTEGER NOT NULL
    CHECK (rn_capabilities BETWEEN 0 AND 65535),
  proof_version INTEGER NOT NULL CHECK (proof_version = 1),
  -- Public signed-bundle protocol version. This is unrelated to the Rust
  -- keystore's private on-disk IDENTITY_BLOB_VERSION.
  identity_bundle_version INTEGER NOT NULL CHECK (identity_bundle_version = 1),
  lifecycle_version INTEGER NOT NULL CHECK (lifecycle_version = 2),
  spk_pub_b64 TEXT NOT NULL CHECK (length(spk_pub_b64) = 44),
  spk_signature_b64 TEXT NOT NULL CHECK (length(spk_signature_b64) = 88),
  spk_rotated_at TEXT NOT NULL,
  highest_generation INTEGER NOT NULL CHECK (highest_generation >= 1),
  batch_commitment_b64 TEXT NOT NULL CHECK (length(batch_commitment_b64) = 44),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms > 0)
) WITHOUT ROWID;

CREATE TRIGGER prekey_lifecycle_authority_insert_guard
BEFORE INSERT ON prekey_lifecycle_authority
WHEN NOT (
  (
    (
      NOT EXISTS (
        SELECT 1 FROM prekey_lifecycle_authority
         WHERE user_id = NEW.user_id
      )
      AND NEW.highest_generation = 1
    )
    OR EXISTS (
      SELECT 1 FROM prekey_lifecycle_authority
       WHERE user_id = NEW.user_id
    )
  )
  AND EXISTS (
    SELECT 1
      FROM users
     WHERE user_id = NEW.user_id
       AND identity_scheme = 1
       AND identity_revision = NEW.identity_revision
       AND ik_root_ed25519_pub = NEW.ik_root_ed25519_pub
       AND ik_ed25519_pub = NEW.ik_ed25519_pub
       AND rn_capabilities = NEW.rn_capabilities
       AND identity_lookup_enabled = 1
  )
)
BEGIN
  SELECT RAISE(ABORT, 'scheme-1 prekey lifecycle genesis is invalid');
END;

CREATE TRIGGER prekey_lifecycle_authority_update_guard
BEFORE UPDATE ON prekey_lifecycle_authority
WHEN NOT (
  NEW.user_id = OLD.user_id
  AND NEW.ik_root_ed25519_pub = OLD.ik_root_ed25519_pub
  AND NEW.highest_generation = OLD.highest_generation + 1
  AND NEW.updated_at_ms >= OLD.updated_at_ms
  AND EXISTS (
    SELECT 1
      FROM users
     WHERE user_id = NEW.user_id
       AND identity_scheme = 1
       AND identity_revision = NEW.identity_revision
       AND ik_root_ed25519_pub = NEW.ik_root_ed25519_pub
       AND ik_ed25519_pub = NEW.ik_ed25519_pub
       AND rn_capabilities = NEW.rn_capabilities
       AND identity_lookup_enabled = 1
  )
)
BEGIN
  SELECT RAISE(ABORT, 'scheme-1 prekey lifecycle CAS is stale');
END;

CREATE TRIGGER prekey_lifecycle_authority_no_delete
BEFORE DELETE ON prekey_lifecycle_authority
BEGIN
  SELECT RAISE(ABORT, 'scheme-1 prekey lifecycle authority cannot be reset');
END;

CREATE TRIGGER prekey_replenish_receipt_scheme_guard
BEFORE INSERT ON prekey_replenish_receipts
WHEN NOT (
  (
    NEW.identity_scheme = 0
    AND NEW.protocol_version = 1
    AND NEW.identity_revision IS NULL
    AND NEW.identity_bundle_commitment_b64 IS NULL
    AND NEW.lifecycle_generation IS NULL
    AND NEW.batch_commitment_b64 IS NULL
    AND NEW.opks_added IS NULL
    AND NOT EXISTS (
      SELECT 1 FROM users
       WHERE user_id = NEW.user_id
         AND identity_scheme = 1
    )
  )
  OR (
    NEW.identity_scheme = 1
    AND NEW.protocol_version = 2
    AND NEW.identity_revision >= 1
    AND length(NEW.identity_bundle_commitment_b64) = 44
    AND NEW.lifecycle_generation >= 1
    AND length(NEW.batch_commitment_b64) = 44
    AND NEW.opks_added BETWEEN 1 AND 100
    AND EXISTS (
      SELECT 1 FROM users
       WHERE user_id = NEW.user_id
         AND identity_scheme = 1
         AND identity_revision = NEW.identity_revision
         AND ik_ed25519_pub = NEW.signer_ed25519_pub
         AND identity_lookup_enabled = 1
    )
  )
)
BEGIN
  SELECT RAISE(ABORT, 'prekey replenish receipt scheme context is invalid');
END;

CREATE TRIGGER opk_pool_scheme1_proof_guard
BEFORE INSERT ON opk_pool
WHEN EXISTS (
  SELECT 1 FROM users
   WHERE user_id = NEW.user_id AND identity_scheme = 1
)
AND COALESCE(NOT (
  NEW.owner_proof_json IS NOT NULL
  AND json_valid(NEW.owner_proof_json)
  AND NEW.lifecycle_generation >= 1
  AND length(NEW.batch_commitment_b64) = 44
  AND CAST(json_extract(NEW.owner_proof_json, '$.version') AS INTEGER) = 1
  AND json_extract(NEW.owner_proof_json, '$.owner_user_id') = NEW.user_id
  AND CAST(json_extract(NEW.owner_proof_json, '$.identity_bundle_version') AS INTEGER) = 1
  AND CAST(json_extract(NEW.owner_proof_json, '$.lifecycle_version') AS INTEGER) = 2
  AND CAST(json_extract(NEW.owner_proof_json, '$.lifecycle_generation') AS INTEGER)
      = NEW.lifecycle_generation
  AND json_extract(NEW.owner_proof_json, '$.batch_commitment_b64')
      = NEW.batch_commitment_b64
  AND CAST(json_extract(NEW.owner_proof_json, '$.opk_id') AS INTEGER) = NEW.opk_id
  AND json_extract(NEW.owner_proof_json, '$.opk_pub_b64') = NEW.opk_pub
  AND EXISTS (
    SELECT 1
      FROM prekey_lifecycle_authority
     WHERE user_id = NEW.user_id
       AND highest_generation = NEW.lifecycle_generation
       AND batch_commitment_b64 = NEW.batch_commitment_b64
       AND identity_bundle_commitment_b64
           = json_extract(
               NEW.owner_proof_json,
               '$.identity_bundle_commitment_b64'
             )
       AND rn_capabilities
           = CAST(
               json_extract(NEW.owner_proof_json, '$.rn_capabilities')
               AS INTEGER
             )
       AND spk_pub_b64
           = json_extract(NEW.owner_proof_json, '$.spk_pub_b64')
       AND spk_signature_b64
           = json_extract(NEW.owner_proof_json, '$.spk_signature_b64')
       AND spk_rotated_at
           = json_extract(NEW.owner_proof_json, '$.spk_rotated_at')
  )
  AND NOT EXISTS (
    SELECT 1
      FROM opk_pool
     WHERE user_id = NEW.user_id
       AND opk_pub = NEW.opk_pub
  )
), 1)
BEGIN
  SELECT RAISE(ABORT, 'scheme-1 OPK owner proof row is invalid');
END;

CREATE TRIGGER opk_pool_scheme0_separation_guard
BEFORE INSERT ON opk_pool
WHEN EXISTS (
  SELECT 1 FROM users
   WHERE user_id = NEW.user_id AND identity_scheme = 0
)
AND NOT (
  NEW.owner_proof_json IS NULL
  AND NEW.lifecycle_generation = 0
  AND NEW.batch_commitment_b64 IS NULL
)
BEGIN
  SELECT RAISE(ABORT, 'legacy OPK row cannot carry scheme-1 proof state');
END;

CREATE TRIGGER opk_pool_proof_rows_immutable
BEFORE UPDATE OF
  user_id,
  opk_id,
  opk_pub,
  owner_proof_json,
  lifecycle_generation,
  batch_commitment_b64
ON opk_pool
WHEN OLD.owner_proof_json IS NOT NULL
BEGIN
  SELECT RAISE(ABORT, 'scheme-1 OPK proof rows are immutable');
END;
