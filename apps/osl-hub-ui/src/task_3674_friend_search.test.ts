import { describe, expect, it } from "vitest";
import type { Friend } from "./friend-list-search";
import {
  backToFriendList,
  FRIEND_NOT_IN_TAB_ERROR,
  friendListMarkup,
  friendPageMarkup,
  openFriendPage,
  setFriendListSearch,
  setFriendListTab,
  initialFriendListState,
  visibleFriends,
} from "./friend-list-search";

function hundredFriends(): Friend[] {
  return Array.from({ length: 100 }, (_, index) => {
    const n = index + 1;
    return { id: `friend-${n}`, name: `Friend ${n}`, tab: "all" as const };
  });
}

describe("TASK 3674 friend search for the large list", () => {
  it("an exact search returns friend 73 once, opens friend 73, and Back returns to the same search and tab", () => {
    const friends = hundredFriends();
    console.log(`TASK3674_FRIEND_COUNT=${friends.length}`);

    let state = initialFriendListState("all");
    state = setFriendListTab(state, "all");
    state = setFriendListSearch(state, "Friend 73");

    const results = visibleFriends(friends, state);
    console.log(`TASK3674_SEARCH_QUERY="${state.searchQuery}"`);
    console.log(`TASK3674_RESULT_COUNT=${results.length}`);
    console.log(`TASK3674_RESULT_IDS=${results.map((f) => f.id).join(",")}`);

    expect(results).toHaveLength(1);
    expect(results[0].id).toBe("friend-73");

    const listBeforeOpen = friendListMarkup(friends, state);
    expect(listBeforeOpen).toContain('data-active-tab="all"');
    expect(listBeforeOpen).toContain('value="Friend 73"');
    expect(listBeforeOpen).toContain('data-open-friend="friend-73"');

    const opened = openFriendPage(friends, state, "friend-73");
    console.log(`TASK3674_OPENED_SCREEN=${opened.screen}`);
    console.log(`TASK3674_OPENED_FRIEND=${opened.openFriendId}`);
    expect(opened.screen).toBe("friend-page");
    expect(opened.openFriendId).toBe("friend-73");

    const pageMarkup = friendPageMarkup(friends, opened);
    expect(pageMarkup).toContain('data-friend-page="friend-73"');
    expect(pageMarkup).toContain("Friend 73");
    expect(pageMarkup).toContain("data-friend-page-back");

    const backState = backToFriendList(opened);
    console.log(`TASK3674_BACK_SCREEN=${backState.screen}`);
    console.log(`TASK3674_BACK_TAB=${backState.activeTab}`);
    console.log(`TASK3674_BACK_QUERY="${backState.searchQuery}"`);

    expect(backState.screen).toBe("list");
    expect(backState.activeTab).toBe(state.activeTab);
    expect(backState.searchQuery).toBe(state.searchQuery);

    const listAfterBack = friendListMarkup(friends, backState);
    const resultsAfterBack = visibleFriends(friends, backState);
    console.log(`TASK3674_AFTER_BACK_RESULT_COUNT=${resultsAfterBack.length}`);
    expect(listAfterBack).toContain('data-active-tab="all"');
    expect(listAfterBack).toContain('value="Friend 73"');
    expect(resultsAfterBack).toHaveLength(1);
    expect(resultsAfterBack[0].id).toBe("friend-73");

    console.log("TASK3674_DONE=true");
  });

  it("preserves a non-default tab across the open/Back round trip", () => {
    const friends: Friend[] = [
      { id: "p-1", name: "Pat", tab: "pending" },
      { id: "a-1", name: "Pat", tab: "all" },
    ];
    let state = initialFriendListState("pending");
    state = setFriendListSearch(state, "Pat");

    const results = visibleFriends(friends, state);
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe("p-1");

    const opened = openFriendPage(friends, state, "p-1");
    const backState = backToFriendList(opened);
    expect(backState.activeTab).toBe("pending");
    expect(backState.searchQuery).toBe("Pat");
  });

  it("refuses to open a friend that is not in the currently visible (tab + search) results", () => {
    const friends = hundredFriends();
    let state = initialFriendListState("all");
    state = setFriendListSearch(state, "Friend 73");
    expect(() => openFriendPage(friends, state, "friend-1")).toThrow(FRIEND_NOT_IN_TAB_ERROR);
  });
});
