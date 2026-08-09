import { describe, expect, it } from "vitest";
import {
  FRIEND_NOT_IN_VISIBLE_LIST_ERROR,
  backToFriendList,
  friendPageMarkup,
  initialFriendListState,
  openFriendPage,
  setFriendListSearch,
  setFriendListTab,
  visibleFriends,
  type Friend,
} from "./friend-list-search";

const hundredFriends: readonly Friend[] = Array.from({ length: 100 }, (_, index) => ({
  id: `friend-${index + 1}`,
  name: `Friend ${index + 1}`,
  tab: "online",
}));

describe("task 3674: searchable Friends list", () => {
  it("among 100 friends, exact search returns friend 73 once, opens it, and Back preserves tab and search", () => {
    const searched = setFriendListSearch(initialFriendListState("all"), "Friend 73");
    const results = visibleFriends(hundredFriends, searched);
    expect(results).toEqual([{ id: "friend-73", name: "Friend 73", tab: "online" }]);

    const opened = openFriendPage(hundredFriends, searched, "friend-73");
    expect(opened.screen).toBe("friend-page");
    expect(opened.openFriendId).toBe("friend-73");
    expect(friendPageMarkup(results[0]!)).toContain('data-friend-page="friend-73"');

    const back = backToFriendList(opened);
    expect(back).toMatchObject({ screen: "list", activeTab: "all", searchQuery: "Friend 73" });
    expect(visibleFriends(hundredFriends, back)).toEqual(results);
    console.log("TASK3674_FRIEND_COUNT=100");
    console.log('TASK3674_SEARCH_QUERY="Friend 73"');
    console.log("TASK3674_RESULT_COUNT=1");
    console.log("TASK3674_RESULT_IDS=friend-73");
    console.log("TASK3674_OPENED_SCREEN=friend-page");
    console.log("TASK3674_OPENED_FRIEND=friend-73");
    console.log("TASK3674_BACK_SCREEN=list");
    console.log("TASK3674_BACK_TAB=all");
    console.log('TASK3674_BACK_QUERY="Friend 73"');
    console.log("TASK3674_AFTER_BACK_RESULT_COUNT=1");
  });

  it("keeps a non-default selected tab across the friend-page round trip", () => {
    const friends: readonly Friend[] = [
      { id: "pending-73", name: "Friend 73", tab: "pending" },
      { id: "online-73", name: "Friend 73", tab: "online" },
    ];
    const searched = setFriendListSearch(setFriendListTab(initialFriendListState(), "pending"), "Friend 73");
    const back = backToFriendList(openFriendPage(friends, searched, "pending-73"));
    expect(back).toMatchObject({ activeTab: "pending", searchQuery: "Friend 73", screen: "list" });
    expect(visibleFriends(friends, back).map((friend) => friend.id)).toEqual(["pending-73"]);
  });

  it("refuses a friend outside the active tab and search results", () => {
    const searched = setFriendListSearch(initialFriendListState("all"), "Friend 73");
    expect(() => openFriendPage(hundredFriends, searched, "friend-72")).toThrow(FRIEND_NOT_IN_VISIBLE_LIST_ERROR);
  });
});
