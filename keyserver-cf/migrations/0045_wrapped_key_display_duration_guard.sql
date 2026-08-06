-- View-once display duration is an explicit UI exposure window, not an
-- arbitrary protocol u32. Enforce the same 1..60 second contract at the D1
-- boundary so direct writers cannot bypass endpoint validation.

CREATE TRIGGER wrapped_keys_display_duration_insert_guard
BEFORE INSERT ON wrapped_keys
WHEN
  (
    NEW.single_use = 1
    AND (
      typeof(NEW.display_duration_seconds) != 'integer'
      OR NEW.display_duration_seconds < 1
      OR NEW.display_duration_seconds > 60
    )
  )
  OR (NEW.single_use = 0 AND NEW.display_duration_seconds IS NOT NULL)
  OR (NEW.single_use NOT IN (0, 1))
BEGIN
  SELECT RAISE(ABORT, 'wrapped key display duration must be integer seconds between 1 and 60 for single-use rows');
END;

CREATE TRIGGER wrapped_keys_display_duration_update_guard
BEFORE UPDATE OF single_use, display_duration_seconds ON wrapped_keys
WHEN
  (
    NEW.single_use = 1
    AND (
      typeof(NEW.display_duration_seconds) != 'integer'
      OR NEW.display_duration_seconds < 1
      OR NEW.display_duration_seconds > 60
    )
  )
  OR (NEW.single_use = 0 AND NEW.display_duration_seconds IS NOT NULL)
  OR (NEW.single_use NOT IN (0, 1))
BEGIN
  SELECT RAISE(ABORT, 'wrapped key display duration must be integer seconds between 1 and 60 for single-use rows');
END;
