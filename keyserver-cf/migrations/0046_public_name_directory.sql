-- 0046: minimal public-name directory.
--
-- The legacy username_directory still stores protocol routing material needed
-- by older lookup flows. This table is the public-name search surface: one
-- exact name, one identity fingerprint, and timestamps only.

CREATE TABLE public_name_directory (
  name TEXT PRIMARY KEY,
  identity_fingerprint TEXT NOT NULL
    CHECK (
      length(identity_fingerprint) = 64
      AND identity_fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
  claimed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (name) REFERENCES username_directory (username)
    ON DELETE CASCADE
    ON UPDATE CASCADE
) WITHOUT ROWID;

CREATE INDEX idx_public_name_directory_identity_fingerprint
  ON public_name_directory (identity_fingerprint);

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('minimal_public_name_directory', 1);
