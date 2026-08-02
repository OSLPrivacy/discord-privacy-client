-- A handle is an identity assertion, not a reusable label.  Keep its
-- normalized spelling and UTS #39 skeleton after every release path.
ALTER TABLE username_directory ADD COLUMN username_skeleton TEXT NOT NULL DEFAULT '';
ALTER TABLE username_directory ADD COLUMN display_username TEXT NOT NULL DEFAULT '';

-- Rows predating the identity-hardening writer used the ASCII-only grammar,
-- for which the normalized spelling is also the skeleton and display value.
UPDATE username_directory
   SET username_skeleton = username,
       display_username = username
 WHERE username_skeleton = '' OR display_username = '';

CREATE UNIQUE INDEX idx_username_directory_skeleton
  ON username_directory(username_skeleton);

CREATE TABLE username_tombstones (
  username TEXT PRIMARY KEY,
  skeleton TEXT NOT NULL UNIQUE,
  retired_at TEXT NOT NULL
) WITHOUT ROWID;

-- This trigger is deliberately the final guard for all three release paths:
-- rename, key rotation, and unregister each delete a directory row.  It also
-- protects future maintenance code from accidentally making a name reusable.
CREATE TRIGGER username_directory_retire_before_delete
BEFORE DELETE ON username_directory
BEGIN
  INSERT OR IGNORE INTO username_tombstones (username, skeleton, retired_at)
  VALUES (OLD.username, OLD.username_skeleton, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

-- A claim must not revive either the spelling or a confusable spelling of a
-- retired identity.  The K2 writer supplies username_skeleton on new claims.
CREATE TRIGGER username_directory_reject_retired_before_insert
BEFORE INSERT ON username_directory
WHEN EXISTS (
  SELECT 1 FROM username_tombstones
   WHERE username = NEW.username OR skeleton = NEW.username_skeleton
)
BEGIN
  SELECT RAISE(ABORT, 'username is retired');
END;
