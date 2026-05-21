-- Phase 6.4: Control Inbox.
--
-- Out-of-band delivery for v=5 sender-key distribution bundles.
-- Pre-6.4, every v=5 GC send dropped a 2KB prose-token cover into
-- the Discord channel (visible as ciphertext noise to observers
-- AND consuming cipher-store upload budget). Post-6.4, the bundle
-- gets POSTed to the recipient's row here; receivers poll their
-- own inbox (~10s cadence) and drain.
--
-- The bundle payload is the same v=3 multi-recipient PQ-hybrid
-- wire encrypt_v5_send already produces -- the only thing that
-- changes is the transport. End-to-end content is unchanged so
-- no crypto risk.
--
-- Auth: every POST/GET/DELETE carries an ed25519 signature over
-- canonical bytes (see keyserver-cf/src/lib/canonical.ts). The
-- pubkey is the requesting user's ik_ed25519_pub stored in the
-- existing `users` table -- one less surface to authenticate.
--
-- TTL: rows auto-expire at `expires_at`. Server sweep at the
-- existing keyserver cron deletes expired rows. Default TTL is
-- 7 days; receivers who are offline that long lose the SKDM and
-- recover via the existing SKDM_REQUEST round-trip on next v=5
-- inbound they can't decrypt.

CREATE TABLE IF NOT EXISTS control_inbox (
  id            BLOB    PRIMARY KEY NOT NULL,  -- 16 random bytes
  recipient_id  TEXT    NOT NULL,              -- discord_id of recipient
  sender_id     TEXT    NOT NULL,              -- discord_id of sender (for verify + audit)
  scope_id      TEXT    NOT NULL,              -- scope.storage_key() for routing
  bundle        BLOB    NOT NULL,              -- v=3 wire bytes; server never reads
  expires_at    INTEGER NOT NULL,              -- unix-epoch seconds
  created_at    INTEGER NOT NULL               -- unix-epoch seconds; for FIFO order
);

-- Receiver lookup: every GET drains a single user's inbox in
-- insert order. The composite covers both the predicate and the
-- ORDER BY for that query.
CREATE INDEX IF NOT EXISTS idx_control_inbox_recipient
  ON control_inbox(recipient_id, created_at);

-- TTL sweep predicate.
CREATE INDEX IF NOT EXISTS idx_control_inbox_expires_at
  ON control_inbox(expires_at);
