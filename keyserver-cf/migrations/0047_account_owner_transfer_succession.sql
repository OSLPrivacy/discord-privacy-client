-- 0047: co-owners, explicit accepted ownership transfers, and
-- owner-chosen succession.
--
-- Succession is never inferred from moderation state. It can only run from an
-- owner-authored standing setting naming a successor and quiet period.

CREATE TABLE account_roles (
  account_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'owner' CHECK (role = 'owner'),
  granted_by_user_id TEXT NOT NULL,
  granted_at_unix_seconds INTEGER NOT NULL,
  last_seen_at_unix_seconds INTEGER NOT NULL,
  PRIMARY KEY (account_id, user_id),
  FOREIGN KEY (user_id) REFERENCES users (user_id) ON DELETE CASCADE,
  FOREIGN KEY (granted_by_user_id) REFERENCES users (user_id) ON DELETE CASCADE
);

CREATE INDEX idx_account_roles_account_role
  ON account_roles (account_id, role);

CREATE INDEX idx_account_roles_user_role
  ON account_roles (user_id, role);

CREATE TABLE ownership_transfers (
  transfer_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  from_user_id TEXT NOT NULL,
  to_user_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'accepted')),
  requested_at_unix_seconds INTEGER NOT NULL,
  accepted_at_unix_seconds INTEGER,
  CHECK (
    (status = 'pending' AND accepted_at_unix_seconds IS NULL)
    OR (status = 'accepted' AND accepted_at_unix_seconds IS NOT NULL)
  ),
  FOREIGN KEY (from_user_id) REFERENCES users (user_id) ON DELETE CASCADE,
  FOREIGN KEY (to_user_id) REFERENCES users (user_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_ownership_transfers_one_pending_per_account
  ON ownership_transfers (account_id)
  WHERE status = 'pending';

CREATE INDEX idx_ownership_transfers_receiver_status
  ON ownership_transfers (to_user_id, status);

CREATE TABLE ownership_succession_settings (
  account_id TEXT NOT NULL,
  owner_user_id TEXT NOT NULL,
  successor_user_id TEXT NOT NULL,
  quiet_period_seconds INTEGER NOT NULL CHECK (quiet_period_seconds > 0),
  chosen_at_unix_seconds INTEGER NOT NULL,
  PRIMARY KEY (account_id, owner_user_id),
  FOREIGN KEY (account_id, owner_user_id)
    REFERENCES account_roles (account_id, user_id) ON DELETE CASCADE,
  FOREIGN KEY (successor_user_id) REFERENCES users (user_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX idx_ownership_succession_successor
  ON ownership_succession_settings (successor_user_id);

CREATE TABLE searchable_moderation_log (
  log_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  actor_user_id TEXT NOT NULL,
  subject_user_id TEXT NOT NULL,
  action TEXT NOT NULL,
  searchable_text TEXT NOT NULL,
  created_at_unix_seconds INTEGER NOT NULL,
  FOREIGN KEY (actor_user_id) REFERENCES users (user_id) ON DELETE CASCADE,
  FOREIGN KEY (subject_user_id) REFERENCES users (user_id) ON DELETE CASCADE
);

CREATE INDEX idx_searchable_moderation_log_account_action
  ON searchable_moderation_log (account_id, action);

CREATE INDEX idx_searchable_moderation_log_text
  ON searchable_moderation_log (searchable_text);

CREATE TABLE IF NOT EXISTS worker_schema_capabilities (
  capability TEXT PRIMARY KEY,
  version INTEGER NOT NULL
);

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('account_owner_transfer_succession', 1);
