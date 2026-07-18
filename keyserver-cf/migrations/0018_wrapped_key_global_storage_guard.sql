-- A per-sender or per-recipient limit does not bound total storage when
-- registration is open. Keep an exact physical-row/decoded-byte counter so a
-- distributed set of identities cannot grow D1 without a hard ceiling.

CREATE TABLE wrapped_key_storage_usage (
  singleton      INTEGER PRIMARY KEY CHECK (singleton = 1),
  row_count      INTEGER NOT NULL CHECK (row_count >= 0),
  decoded_bytes  INTEGER NOT NULL CHECK (decoded_bytes >= 0)
) WITHOUT ROWID;

INSERT INTO wrapped_key_storage_usage (singleton, row_count, decoded_bytes)
SELECT
  1,
  COUNT(*),
  COALESCE(
    SUM(
      (length(wrapped_share_blob) * 3 / 4) -
      CASE
        WHEN substr(wrapped_share_blob, -2) = '==' THEN 2
        WHEN substr(wrapped_share_blob, -1) = '=' THEN 1
        ELSE 0
      END
    ),
    0
  )
FROM wrapped_keys;

-- The Worker already requires canonical UTC timestamps. This database guard
-- prevents a future or legacy writer from inserting a value that SQLite cannot
-- parse, which would otherwise evade both quota predicates and the TTL sweep.
CREATE TRIGGER wrapped_keys_expiry_must_parse
BEFORE INSERT ON wrapped_keys
WHEN unixepoch(NEW.expires_at) IS NULL
BEGIN
  SELECT RAISE(ABORT, 'wrapped key expiry is not SQLite-compatible');
END;

-- 262,144 physical rows or 1 GiB of decoded opaque key material. Physical,
-- rather than merely live, accounting keeps expired rows bounded even if the
-- scheduled sweep is delayed. Raising either ceiling is an explicit migration.
CREATE TRIGGER wrapped_keys_global_storage_quota
BEFORE INSERT ON wrapped_keys
WHEN
  (SELECT row_count FROM wrapped_key_storage_usage WHERE singleton = 1) >= 262144
  OR
  (SELECT decoded_bytes FROM wrapped_key_storage_usage WHERE singleton = 1) +
    (
      (length(NEW.wrapped_share_blob) * 3 / 4) -
      CASE
        WHEN substr(NEW.wrapped_share_blob, -2) = '==' THEN 2
        WHEN substr(NEW.wrapped_share_blob, -1) = '=' THEN 1
        ELSE 0
      END
    ) > 1073741824
BEGIN
  SELECT RAISE(ABORT, 'wrapped key global storage quota exceeded');
END;

CREATE TRIGGER wrapped_keys_storage_usage_insert
AFTER INSERT ON wrapped_keys
BEGIN
  UPDATE wrapped_key_storage_usage
     SET row_count = row_count + 1,
         decoded_bytes = decoded_bytes +
           (
             (length(NEW.wrapped_share_blob) * 3 / 4) -
             CASE
               WHEN substr(NEW.wrapped_share_blob, -2) = '==' THEN 2
               WHEN substr(NEW.wrapped_share_blob, -1) = '=' THEN 1
               ELSE 0
             END
           )
   WHERE singleton = 1;
END;

CREATE TRIGGER wrapped_keys_storage_usage_delete
AFTER DELETE ON wrapped_keys
BEGIN
  UPDATE wrapped_key_storage_usage
     SET row_count = MAX(0, row_count - 1),
         decoded_bytes = MAX(
           0,
           decoded_bytes -
             (
               (length(OLD.wrapped_share_blob) * 3 / 4) -
               CASE
                 WHEN substr(OLD.wrapped_share_blob, -2) = '==' THEN 2
                 WHEN substr(OLD.wrapped_share_blob, -1) = '=' THEN 1
                 ELSE 0
               END
             )
         )
   WHERE singleton = 1;
END;
