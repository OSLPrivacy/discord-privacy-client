-- 0008: lease-bounded, per-object attachment sweep claims.
--
-- DEPLOY ORDER. Apply this migration BEFORE deploying the matching Worker.
-- The previous Worker never names this companion table, so the migration is
-- inert until the new code is deployed. The new Worker claims through this
-- table before touching R2 and therefore fails closed, without deleting
-- storage or metadata, when the table is absent. Multipart completion also
-- acquires this same claim before its asynchronous R2 `complete`, so an expiry
-- sweep and completion can never own one attachment at the same lease version.
--
-- Each claim is bound to both a random Worker invocation id and a distinct
-- random token. `lease_version` increases on every claim, reclaim, or explicit
-- failure release; completion and release compare all three values. An expired
-- Worker can therefore never complete a claim after another invocation has
-- recovered it.
--
-- Failed storage work keeps the attachment row and moves this claim into a
-- bounded retry delay. Successful metadata deletion cascades this row. The
-- claim table never becomes an alternative attachment inventory.

CREATE TABLE attachment_sweep_claims (
  attachment_id   TEXT    PRIMARY KEY NOT NULL
                          REFERENCES attachment_objects(id) ON DELETE CASCADE,
  worker_id       TEXT    CHECK(worker_id IS NULL OR (
                            length(worker_id) = 32
                            AND worker_id NOT GLOB '*[^0-9a-f]*'
                          )),
  claim_token     TEXT    CHECK(claim_token IS NULL OR (
                            length(claim_token) = 64
                            AND claim_token NOT GLOB '*[^0-9a-f]*'
                          )),
  lease_version   INTEGER NOT NULL CHECK(lease_version > 0),
  lease_expires_at INTEGER NOT NULL CHECK(lease_expires_at >= 0),
  retry_not_before INTEGER NOT NULL CHECK(retry_not_before >= 0),
  attempt_count   INTEGER NOT NULL CHECK(attempt_count > 0),
  last_claimed_at INTEGER NOT NULL CHECK(last_claimed_at > 0),
  CHECK(
    (worker_id IS NULL AND claim_token IS NULL AND lease_expires_at = 0)
    OR
    (worker_id IS NOT NULL AND claim_token IS NOT NULL AND lease_expires_at > 0)
  )
) STRICT;

CREATE INDEX idx_attachment_sweep_claims_retry
  ON attachment_sweep_claims(retry_not_before, lease_expires_at);
