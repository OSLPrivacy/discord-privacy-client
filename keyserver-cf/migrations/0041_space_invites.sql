-- 0041: opaque Space-invite capability ledger.
--
-- These rows intentionally contain no account, Space identifier, ciphertext,
-- or recipient.  An invite's encrypted payload travels out of band; this
-- service knows only a hash of each one-time redemption capability and its
-- separate revocation capability.  Neither value is useful if the database is
-- read directly.
CREATE TABLE space_invites (
  invite_hash            BLOB    PRIMARY KEY,
  revocation_hash        BLOB    NOT NULL UNIQUE,
  expires_at             INTEGER NOT NULL
) WITHOUT ROWID;

CREATE INDEX idx_space_invites_expires_at ON space_invites(expires_at);
