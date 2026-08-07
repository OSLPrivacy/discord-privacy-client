-- 0047: blinded discovery cards.
--
-- The server stores only opaque routing material. `drawer_name` is a deliberately
-- short bucket name, `label` is the per-epoch lookup label, `sealed_note` is
-- ciphertext from the writer for the intended reader, and `discovery_epoch` is
-- the weekly stamp used for retention.

CREATE TABLE discovery_cards (
  drawer_name TEXT NOT NULL
    CHECK (
      length(drawer_name) = 3
      AND drawer_name NOT GLOB '*[^0-9a-f]*'
    ),
  label TEXT NOT NULL
    CHECK (
      length(label) BETWEEN 22 AND 64
      AND label NOT GLOB '*[^A-Za-z0-9_-]*'
    ),
  sealed_note TEXT NOT NULL
    CHECK (length(sealed_note) BETWEEN 1 AND 8192),
  discovery_epoch TEXT NOT NULL
    CHECK (discovery_epoch GLOB '[0-9][0-9][0-9][0-9]-W[0-9][0-9]'),
  discovery_epoch_index INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (drawer_name, label)
) WITHOUT ROWID;

CREATE INDEX idx_discovery_cards_epoch
  ON discovery_cards (discovery_epoch_index);

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('blinded_discovery_cards', 1);
