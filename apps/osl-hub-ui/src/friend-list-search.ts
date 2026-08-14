import type { FriendsTabId as FriendsTab } from "./friends-tabs";

export interface Friend {
  readonly id: string;
  readonly name: string;
  readonly tab: FriendsTab;
}

export type FriendListScreen = "list" | "friend-page";

export interface FriendListState {
  readonly activeTab: FriendsTab;
  readonly searchQuery: string;
  readonly screen: FriendListScreen;
  readonly openFriendId: string | null;
}

export function initialFriendListState(activeTab: FriendsTab = "all"): FriendListState {
  return { activeTab, searchQuery: "", screen: "list", openFriendId: null };
}

function matchesSearch(friend: Friend, query: string): boolean {
  const trimmed = query.trim().toLowerCase();
  if (trimmed.length === 0) return true;
  return friend.name.toLowerCase().includes(trimmed);
}

/** The friends shown for the current tab and search query, list screen only. */
export function visibleFriends(friends: readonly Friend[], state: FriendListState): Friend[] {
  return friends.filter(
    (friend) => friend.tab === state.activeTab && matchesSearch(friend, state.searchQuery),
  );
}

export function setFriendListTab(state: FriendListState, tab: FriendsTab): FriendListState {
  return { ...state, activeTab: tab };
}

export function setFriendListSearch(state: FriendListState, query: string): FriendListState {
  return { ...state, searchQuery: query };
}

export const FRIEND_NOT_IN_TAB_ERROR = "Cannot open a friend outside the selected tab's visible results.";

/**
 * Open a friend's page. Only a friend currently visible (right tab, matches the
 * search) can be opened, so the id passed here always came from a row the list
 * screen actually drew.
 */
export function openFriendPage(
  friends: readonly Friend[],
  state: FriendListState,
  friendId: string,
): FriendListState {
  const visible = visibleFriends(friends, state);
  if (!visible.some((friend) => friend.id === friendId)) {
    throw new Error(FRIEND_NOT_IN_TAB_ERROR);
  }
  return { ...state, screen: "friend-page", openFriendId: friendId };
}

/**
 * Back from the friend page returns to the list screen with the same tab and
 * search query it had when the friend was opened -- neither was ever cleared.
 */
export function backToFriendList(state: FriendListState): FriendListState {
  return { ...state, screen: "list", openFriendId: null };
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function friendRowMarkup(friend: Friend): string {
  return `<li class="friend-row" data-friend-row="${escapeAttribute(friend.id)}"><button class="friend-open-button" type="button" data-open-friend="${escapeAttribute(friend.id)}">${friend.name}</button></li>`;
}

export function friendListMarkup(friends: readonly Friend[], state: FriendListState): string {
  const rows = visibleFriends(friends, state).map(friendRowMarkup).join("");
  return [
    `<section class="friend-list" data-friend-list-screen data-active-tab="${escapeAttribute(state.activeTab)}">`,
    `<input class="friend-search-input" type="search" data-friend-search value="${escapeAttribute(state.searchQuery)}" aria-label="Search friends by name" />`,
    `<ul class="friend-list-rows">${rows}</ul>`,
    `</section>`,
  ].join("");
}

export function friendPageMarkup(friends: readonly Friend[], state: FriendListState): string {
  const friend = friends.find((candidate) => candidate.id === state.openFriendId);
  if (!friend) throw new Error("No friend page is open.");
  return [
    `<section class="friend-page" data-friend-page="${escapeAttribute(friend.id)}">`,
    `<button class="friend-page-back" type="button" data-friend-page-back>Back</button>`,
    `<h2 class="friend-page-name">${friend.name}</h2>`,
    `</section>`,
  ].join("");
}

export function renderFriendListScreen(friends: readonly Friend[], state: FriendListState): string {
  return state.screen === "friend-page"
    ? friendPageMarkup(friends, state)
    : friendListMarkup(friends, state);
}

/** Mount the searchable friend list. Search and the active tab persist across an open/Back round trip. */
export function attachFriendListSearch(
  mount: HTMLElement,
  friends: readonly Friend[],
  initialTab: FriendsTab = "all",
): { getState: () => FriendListState } {
  let state = initialFriendListState(initialTab);
  const draw = (): void => {
    mount.innerHTML = renderFriendListScreen(friends, state);
  };
  mount.addEventListener("input", (event) => {
    const target = event.target as HTMLInputElement | null;
    if (!target || target.dataset.friendSearch === undefined) return;
    state = setFriendListSearch(state, target.value);
    draw();
  });
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const openButton = target?.closest?.("[data-open-friend]") as HTMLElement | null;
    if (openButton) {
      const friendId = openButton.dataset.openFriend;
      if (friendId) {
        state = openFriendPage(friends, state, friendId);
        draw();
      }
      return;
    }
    const backButton = target?.closest?.("[data-friend-page-back]") as HTMLElement | null;
    if (backButton) {
      state = backToFriendList(state);
      draw();
    }
  });
  draw();
  return { getState: () => state };
}
