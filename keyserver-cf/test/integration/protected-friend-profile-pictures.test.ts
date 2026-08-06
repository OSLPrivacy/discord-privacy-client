import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { defineProtectedFriendProfilePicture } from "../../src/lib/db.js";
import { registerTestUser } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

describe("protected friend profile pictures", () => {
  it("stores one identity-owned record with an image-present state", async () => {
    const owner = `profile-picture-owner-${crypto.randomUUID()}`;
    await registerTestUser(SELF, owner);

    await defineProtectedFriendProfilePicture(
      testDb,
      owner,
      "protected-image-envelope-v1",
    );

    const direct = await testDb
      .prepare(
        `SELECT COUNT(DISTINCT owner_user_id) AS owner_count,
                MAX(CASE WHEN protected_image_ciphertext IS NULL THEN 0 ELSE 1 END)
                  AS image_present
           FROM protected_friend_profile_pictures
          WHERE owner_user_id = ?`,
      )
      .bind(owner)
      .first<{ owner_count: number; image_present: number }>();

    console.info(
      `TASK 0230 direct record check: owner_count=${direct?.owner_count} image_present=${direct?.image_present}`,
    );
    expect(direct).toEqual({ owner_count: 1, image_present: 1 });
  });

  it("refuses a profile-picture record without an owner identity", async () => {
    await expect(
      defineProtectedFriendProfilePicture(
        testDb,
        `missing-profile-picture-owner-${crypto.randomUUID()}`,
        "protected-image-envelope-v1",
      ),
    ).rejects.toThrow("protected friend profile picture owner identity missing");
  });
});
