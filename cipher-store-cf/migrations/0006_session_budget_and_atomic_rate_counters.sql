-- 0006: the two availability findings from the 2026-07-26 source audit
-- (docs/security/osl-audit-2026-07-26-codex.md). Both are schema support for
-- Worker logic; neither changes how any existing row is read.
--
-- DEPLOY ORDER. Apply this migration BEFORE deploying the matching Worker.
-- The old Worker never names `content_expires_at` or `rate_counters`, so it is
-- unaffected by their existence. The new Worker reads both on every request, so
-- a Worker deployed against the old schema returns 500 for everything.
--
-- ---------------------------------------------------------------------------
-- HIGH-1 — bodyless multipart reservations
--
-- `POST /v1/attachment/session` inserted the caller's DECLARED size with the
-- caller's requested content TTL before a single byte of ciphertext existed.
-- Sixteen bodyless POSTs therefore reserved the entire 8 GiB global budget for
-- seven days and every later attachment answered 503.
--
-- The row now carries two distinct times:
--
--   expires_at          when the row may be reclaimed. For an incomplete
--                       session this is a short deadline that slides forward on
--                       each accepted part; for a ready object it is the
--                       content expiry.
--   content_expires_at  the expiry promised to the caller in the session
--                       receipt, applied to `expires_at` only on successful
--                       completion.
--
-- Keeping the promised value on the row is a wire requirement, not a
-- convenience: the shipping Rust client
-- (crates/ipc/src/cipher_store_client.rs) rejects a completion receipt whose
-- `expires_at` differs from the session receipt's, and its response structs are
-- `#[serde(deny_unknown_fields)]`, so the value cannot be carried in a new
-- field either.
ALTER TABLE attachment_objects ADD COLUMN content_expires_at INTEGER;

-- Every pre-existing row already holds its final content expiry.
UPDATE attachment_objects SET content_expires_at = expires_at
 WHERE content_expires_at IS NULL;

-- Supports the incomplete-pool aggregates the Worker's conditional INSERT runs
-- on every session creation.
CREATE INDEX IF NOT EXISTS idx_attachment_objects_state
  ON attachment_objects(state);

-- ---------------------------------------------------------------------------
-- HIGH-2 — non-atomic rate limiting
--
-- The KV limiter did `get` → compare → `put(used + 1)` across two awaits.
-- Concurrent requests all observed the same value and collapsed to one
-- increment, so the ceiling its comment claimed did not hold even inside a
-- single POP. Mutation buckets now increment through one conditional statement
-- here instead; D1 serialises writers, so the ceiling is real.
--
-- PRIVACY POSTURE. This is a deliberate, narrow amendment to the "rate-limit
-- state never touches D1" note in wrangler.toml, and it does not weaken it:
--
--   * `bucket_key` is the same opaque value the KV key already used — a
--     truncated HMAC of (bucket, client address) under a server-only key. No
--     address, and nothing derived from one without the secret, is stored.
--   * Retention is SHORTER than before. KV entries lived for 2x the window;
--     these rows are deleted by the existing five-minute cron as soon as their
--     window closes.
--
-- Read buckets deliberately stay on KV: they are an availability control only,
-- they are the highest-volume paths, and they must keep failing open.
CREATE TABLE IF NOT EXISTS rate_counters (
  bucket_key   TEXT    PRIMARY KEY NOT NULL,
  window_start INTEGER NOT NULL,
  used         INTEGER NOT NULL CHECK(used >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_rate_counters_window_start
  ON rate_counters(window_start);
