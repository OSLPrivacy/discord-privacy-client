import { describe, expect, it } from "vitest";
import { friendPictureMarkup } from "./friend-picture";
import {
  acceptedFriendPageActionsMarkup,
  acceptedFriendPageElementCount,
  FRIEND_PAGE_ELEMENT_NAMES,
  hasCompleteAcceptedFriendPage,
} from "./friend-page-actions";

function acceptedFriendFixture(accepted: boolean): string {
  return acceptedFriendPageActionsMarkup({
    personId: "hub-person-task-0270",
    name: "Ada Lovelace",
    pictureMarkup: friendPictureMarkup({
      picture: "data:image/png;base64,task0270",
      fallbackLetter: "A",
      fallbackColour: "#06b6d4",
    }),
    accepted,
  });
}

describe("TASK 0270 - friend page actions", () => {
  it("renders all 9 named elements for an accepted friend", () => {
    const page = acceptedFriendFixture(true);
    const count = acceptedFriendPageElementCount(page);

    console.log(`TASK0270 accepted friend page: ${count} of ${FRIEND_PAGE_ELEMENT_NAMES.length}`);
    expect(count).toBe(9);
    expect(hasCompleteAcceptedFriendPage(page)).toBe(true);
    for (const element of FRIEND_PAGE_ELEMENT_NAMES) {
      expect(page).toContain(`data-friend-page-element="${element}"`);
    }
  });

  it("fails the complete-page check when the accepted tick is removed", () => {
    const pageWithoutTick = acceptedFriendFixture(false);
    const count = acceptedFriendPageElementCount(pageWithoutTick);

    console.log(`TASK0270 tick removed: ${count} of ${FRIEND_PAGE_ELEMENT_NAMES.length}; complete=${hasCompleteAcceptedFriendPage(pageWithoutTick)}`);
    expect(count).toBe(8);
    expect(hasCompleteAcceptedFriendPage(pageWithoutTick)).toBe(false);
    expect(() => expect(count).toBe(9)).toThrow();
  });
});
