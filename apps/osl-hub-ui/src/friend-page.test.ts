import { describe, expect, it } from "vitest";
import { builtInFriendPage, friendPageEmptyStateMarkup, friendPageMarkup } from "./friend-page";

describe("TASK 0840 friend page", () => {
  it("renders each named control and keeps them out of the empty state", () => {
    const page = friendPageMarkup(builtInFriendPage);
    for (const control of ["account", "conversation", "checkmark", "new-account", "save", "remove", "cancel", "back"]) {
      expect(page).toContain(`aria-label=\"${control}\"`);
      expect(friendPageEmptyStateMarkup()).not.toContain(`aria-label=\"${control}\"`);
    }
  });
});
