-- Protected friend profile pictures are owned by exactly one OSL identity.
-- The image is optional because the owner can exist before choosing a picture;
-- when present, only the protected image envelope is stored here.
CREATE TABLE protected_friend_profile_pictures (
  owner_user_id TEXT PRIMARY KEY,
  protected_image_ciphertext TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (owner_user_id) REFERENCES users (user_id),
  CHECK (
    protected_image_ciphertext IS NULL
    OR length(protected_image_ciphertext) > 0
  )
) WITHOUT ROWID;
