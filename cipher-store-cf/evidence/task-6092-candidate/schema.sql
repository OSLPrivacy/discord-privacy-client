CREATE TABLE service_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  frozen INTEGER NOT NULL CHECK (frozen IN (0, 1))
);
INSERT INTO service_state (singleton, frozen) VALUES (1, 0);

CREATE TABLE messages (
  message_id TEXT PRIMARY KEY CHECK (length(message_id) = 32),
  recipient_id TEXT NOT NULL CHECK (length(recipient_id) = 32),
  kind TEXT NOT NULL CHECK (kind IN ('text', 'file')),
  offline INTEGER NOT NULL CHECK (offline IN (0, 1)),
  object_key TEXT NOT NULL UNIQUE,
  envelope BLOB NOT NULL CHECK (length(envelope) >= 120),
  consumed INTEGER NOT NULL CHECK (consumed IN (0, 1)),
  created_at INTEGER NOT NULL
);

CREATE TABLE surface_leaks (
  surface TEXT NOT NULL,
  message_id TEXT NOT NULL,
  leak_bytes BLOB NOT NULL,
  FOREIGN KEY (message_id) REFERENCES messages(message_id)
);
