-- Wave A3: view-once links for recipients who do not run OSL.
--
-- Structural property this schema exists to guarantee: the Worker is
-- incapable of decrypting what it stores. The AES-256-GCM key never
-- reaches the server -- it lives only in the URL *fragment*, which no
-- browser transmits. `data` is opaque ciphertext; there is no key
-- column, no key-derivation input, and no place one could be added
-- without a migration that would be visible in review.
--
-- Identity/telemetry posture, matching the rest of this Worker:
--   * No user id, no IP, no user agent, no referrer, no timing detail.
--   * `retrieved_at` is unix-epoch SECONDS only -- deliberately coarse.
--   * `retrieval_count` bounds how many times the reservation window
--     served the same bytes; it is a small integer, not a log.
--
-- Burn model ("one view, or 60 seconds, whichever is first"):
--   * `reserved_until` is set to now+60 by the first successful fetch.
--     Repeat fetches inside that window are idempotent, so a flaky
--     network cannot destroy content the recipient never saw.
--   * When the window closes, the ciphertext is destroyed regardless of
--     whether the client ever confirmed. Destruction is `data = NULL`,
--     not a row delete, so the *sender* can still be told "Retrieved at
--     HH:MM" or "Expired without being retrieved". The residual row is a
--     receipt with no content and no identifiers; the sweep purges it a
--     day after expiry.
--
-- `data` is therefore NULLABLE on purpose: NULL is the burned state.

CREATE TABLE view_once_links (
  id                      TEXT    PRIMARY KEY NOT NULL
                                  CHECK(length(id) = 32 AND id NOT GLOB '*[^0-9a-f]*'),
  -- Opaque ciphertext: 12-byte AES-GCM nonce || AES-256-GCM(ct || tag).
  -- NULL once burned.
  data                    BLOB,
  size_bytes              INTEGER NOT NULL DEFAULT 0
                                  CHECK(size_bytes >= 0 AND size_bytes <= 262144),
  created_at              INTEGER NOT NULL,
  expires_at              INTEGER NOT NULL,
  -- SHA-256 of the bearer capability the recipient's page presents.
  -- The bearer value itself is never stored; a database leak does not
  -- yield fetchable links.
  fetch_token_sha256_hex  TEXT    NOT NULL
                                  CHECK(length(fetch_token_sha256_hex) = 64
                                    AND fetch_token_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  -- Separate sender-side capability: status + early revoke. Held only
  -- by the OSL client that created the link, never placed in the URL.
  manage_token_sha256_hex TEXT    NOT NULL
                                  CHECK(length(manage_token_sha256_hex) = 64
                                    AND manage_token_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  retrieved_at            INTEGER,
  retrieval_count         INTEGER NOT NULL DEFAULT 0 CHECK(retrieval_count >= 0),
  reserved_until          INTEGER,
  burned_at               INTEGER,
  CHECK((data IS NULL AND size_bytes = 0) OR (data IS NOT NULL AND size_bytes > 0))
) STRICT;

CREATE INDEX idx_view_once_links_expires_at ON view_once_links(expires_at);
CREATE INDEX idx_view_once_links_reserved_until ON view_once_links(reserved_until);
