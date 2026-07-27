-- 0033: activate canonical opaque identities and create a non-resettable
-- sender-filter rollout root.
--
-- This migration does not provision the root. A separately authenticated D1
-- administrator first inserts one hashed, one-time genesis nonce through the
-- shipping provisioning command. The canonical root identity then consumes
-- that nonce through the Worker. Public first-caller-wins genesis is forbidden.

ALTER TABLE users ADD COLUMN identity_revision INTEGER NOT NULL DEFAULT 0
  CHECK (identity_revision >= 0);
ALTER TABLE users ADD COLUMN identity_bundle_proof_sig TEXT;

DROP TRIGGER users_identity_scheme_insert_guard;
DROP TRIGGER users_identity_scheme_update_guard;

CREATE TRIGGER users_identity_scheme_insert_guard
BEFORE INSERT ON users
WHEN NOT (
  (
    NEW.identity_scheme = 0
    AND NEW.identity_revision = 0
    AND NEW.ik_root_ed25519_pub IS NULL
    AND NEW.identity_bundle_proof_sig IS NULL
  )
  OR
  (
    NEW.identity_scheme = 1
    AND NEW.identity_revision >= 1
    AND substr(NEW.user_id, 1, 5) = 'osl1_'
    AND substr(NEW.user_id, 6) NOT GLOB '*[^a-z2-7]*'
    AND length(NEW.user_id) = 57
    AND length(NEW.ik_root_ed25519_pub) = 44
    AND length(NEW.identity_bundle_proof_sig) = 88
  )
)
BEGIN
  SELECT RAISE(ABORT, 'canonical identity state is invalid');
END;

CREATE TRIGGER users_identity_scheme_update_guard
BEFORE UPDATE OF
  user_id,
  identity_scheme,
  identity_revision,
  ik_root_ed25519_pub,
  identity_bundle_proof_sig,
  ik_x25519_pub,
  ik_ed25519_pub,
  ik_mlkem768_pub,
  ik_ratchet_initial_pub,
  rn_capabilities,
  ik_x25519_signature
ON users
WHEN NOT (
  (
    OLD.identity_scheme = 0
    AND NEW.identity_scheme = 0
    AND NEW.identity_revision = 0
    AND NEW.ik_root_ed25519_pub IS NULL
    AND NEW.identity_bundle_proof_sig IS NULL
  )
  OR
  (
    OLD.identity_scheme = 1
    AND NEW.identity_scheme = 1
    AND NEW.user_id = OLD.user_id
    AND NEW.ik_root_ed25519_pub = OLD.ik_root_ed25519_pub
    AND NEW.identity_revision = OLD.identity_revision + 1
    AND length(NEW.identity_bundle_proof_sig) = 88
  )
)
BEGIN
  SELECT RAISE(ABORT, 'canonical identity revision must advance exactly once');
END;

CREATE TABLE sender_filter_rollout_genesis (
  nonce_sha256 TEXT PRIMARY KEY
    CHECK (
      length(nonce_sha256) = 64
      AND nonce_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
  provisioned_at_ms INTEGER NOT NULL CHECK (provisioned_at_ms > 0),
  consumed_at_ms INTEGER CHECK (
    consumed_at_ms IS NULL OR consumed_at_ms >= provisioned_at_ms
  )
) WITHOUT ROWID;

CREATE TRIGGER sender_filter_rollout_genesis_no_delete
BEFORE DELETE ON sender_filter_rollout_genesis
BEGIN
  SELECT RAISE(ABORT, 'sender-filter genesis history cannot be deleted');
END;

CREATE TRIGGER sender_filter_rollout_genesis_transition_guard
BEFORE UPDATE ON sender_filter_rollout_genesis
WHEN NOT (
  NEW.nonce_sha256 = OLD.nonce_sha256
  AND NEW.provisioned_at_ms = OLD.provisioned_at_ms
  AND OLD.consumed_at_ms IS NULL
  AND NEW.consumed_at_ms IS NOT NULL
)
BEGIN
  SELECT RAISE(ABORT, 'sender-filter genesis transition is invalid');
END;

CREATE TABLE sender_filter_rollout_root (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  root_user_id TEXT NOT NULL UNIQUE,
  root_ed25519_pub TEXT NOT NULL,
  identity_bundle_sha256 TEXT NOT NULL CHECK (
    length(identity_bundle_sha256) = 64
    AND identity_bundle_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  capability_version INTEGER NOT NULL CHECK (capability_version >= 1),
  monotonic_version INTEGER NOT NULL CHECK (monotonic_version >= 1),
  last_observation_sha256 TEXT NOT NULL CHECK (
    length(last_observation_sha256) = 64
    AND last_observation_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  provisioned_at_ms INTEGER NOT NULL CHECK (provisioned_at_ms > 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= provisioned_at_ms)
);

CREATE TRIGGER sender_filter_rollout_root_no_delete
BEFORE DELETE ON sender_filter_rollout_root
BEGIN
  SELECT RAISE(ABORT, 'sender-filter rollout root cannot be deleted');
END;

CREATE TRIGGER sender_filter_rollout_root_monotonic_update
BEFORE UPDATE ON sender_filter_rollout_root
WHEN NOT (
  NEW.singleton = OLD.singleton
  AND NEW.root_user_id = OLD.root_user_id
  AND NEW.root_ed25519_pub = OLD.root_ed25519_pub
  AND NEW.identity_bundle_sha256 = OLD.identity_bundle_sha256
  AND NEW.capability_version >= OLD.capability_version
  AND NEW.monotonic_version = OLD.monotonic_version + 1
  AND NEW.provisioned_at_ms = OLD.provisioned_at_ms
  AND NEW.updated_at_ms >= OLD.updated_at_ms
)
BEGIN
  SELECT RAISE(ABORT, 'sender-filter rollout root must advance monotonically');
END;

CREATE TRIGGER users_rollout_root_no_delete
BEFORE DELETE ON users
WHEN EXISTS (
  SELECT 1
    FROM sender_filter_rollout_root
   WHERE root_user_id = OLD.user_id
)
BEGIN
  SELECT RAISE(ABORT, 'sender-filter rollout root identity cannot be deleted');
END;
