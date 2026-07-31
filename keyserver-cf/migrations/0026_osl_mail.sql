-- OSL Mail v1 control plane. Message bodies never enter D1: mailbox content
-- lives ciphertext-only in the per-identity Mailbox Durable Object.

CREATE TABLE mail_address_epochs (
  address TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  user_id TEXT NOT NULL,
  address_epoch INTEGER NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('active', 'tombstoned')),
  created_at TEXT NOT NULL,
  tombstoned_at TEXT,
  UNIQUE (user_id, address_epoch),
  FOREIGN KEY (user_id) REFERENCES users (user_id)
) WITHOUT ROWID;

CREATE UNIQUE INDEX idx_mail_one_active_address
  ON mail_address_epochs (user_id) WHERE state = 'active';
CREATE INDEX idx_mail_address_owner
  ON mail_address_epochs (user_id, state);

-- Recipient-controlled allowlist. OSL-to-OSL delivery is default-deny, so a
-- discovered username is not by itself permission to place mail in a mailbox.
CREATE TABLE mail_sender_consents (
  recipient_user_id TEXT NOT NULL,
  sender_user_id TEXT NOT NULL,
  allowed INTEGER NOT NULL CHECK (allowed IN (0, 1)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (recipient_user_id, sender_user_id),
  FOREIGN KEY (recipient_user_id) REFERENCES users (user_id),
  FOREIGN KEY (sender_user_id) REFERENCES users (user_id)
) WITHOUT ROWID;

-- Single-use signed control-plane mutations. Only request commitments and
-- expiry are retained; no subject, recipient address, or message content.
CREATE TABLE mail_control_receipts (
  user_id TEXT NOT NULL,
  request_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  request_digest BLOB NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, request_id),
  FOREIGN KEY (user_id) REFERENCES users (user_id)
) WITHOUT ROWID;
