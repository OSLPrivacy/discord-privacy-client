/**
 * Task 3674: searchable Friends list.
 *
 * The list state deliberately owns the selected tab and search text.  Opening
 * a friend page changes only the screen, making Back restore the exact list
 * the person navigated from.
 */
import {
  FRIENDS_TAB_IDS,
  FRIENDS_TAB_LABELS,
  type FriendsTabId,
} from "./friends-tabs";

export interface Friend {
  readonly id: string;
  readonly name: string;
  /** "all" is a view, not a stored friend category. */
  readonly tab: Exclude<FriendsTabId, "all">;
}

export interface FriendListState {
  readonly activeTab: FriendsTabId;
  readonly searchQuery: string;
  readonly screen: "list" | "friend-page";
  readonly openFriendId: string | null;
}

export const FRIEND_NOT_IN_VISIBLE_LIST_ERROR = "Friend is not in the visible Friends list";

export function initialFriendListState(activeTab: FriendsTabId = "all"): FriendListState {
  return { activeTab, searchQuery: "", screen: "list", openFriendId: null };
}

function matchesSearch(friend: Friend, query: string): boolean {
  return friend.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
}

export function visibleFriends(friends: readonly Friend[], state: FriendListState): readonly Friend[] {
  return friends.filter(
    (friend) => (state.activeTab === "all" || friend.tab === state.activeTab) && matchesSearch(friend, state.searchQuery),
  );
}

export function setFriendListTab(state: FriendListState, activeTab: FriendsTabId): FriendListState {
  return { ...state, activeTab };
}

export function setFriendListSearch(state: FriendListState, searchQuery: string): FriendListState {
  return { ...state, searchQuery };
}

export function openFriendPage(
  friends: readonly Friend[],
  state: FriendListState,
  friendId: string,
): FriendListState {
  if (!visibleFriends(friends, state).some((friend) => friend.id === friendId)) {
    throw new Error(FRIEND_NOT_IN_VISIBLE_LIST_ERROR);
  }
  return { ...state, screen: "friend-page", openFriendId: friendId };
}

export function backToFriendList(state: FriendListState): FriendListState {
  return { ...state, screen: "list", openFriendId: null };
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

function tabMarkup(state: FriendListState, friends: readonly Friend[]): string {
  return FRIENDS_TAB_IDS.map((tab) => {
    const count = tab === "all" ? friends.length : friends.filter((friend) => friend.tab === tab).length;
    return `<button class="friends-tab${state.activeTab === tab ? " active" : ""}" data-friends-tab="${tab}" type="button" aria-pressed="${state.activeTab === tab}">${FRIENDS_TAB_LABELS[tab]} <span class="tab-count">${count}</span></button>`;
  }).join("");
}

export function friendListMarkup(friends: readonly Friend[], state: FriendListState): string {
  const rows = visibleFriends(friends, state)
    .map((friend) => `<button class="friend-list-row" data-open-friend="${escapeHtml(friend.id)}" type="button">${escapeHtml(friend.name)}</button>`)
    .join("");
  const empty = rows || `<p data-friend-search-empty>No friends match this search.</p>`;
  return `<section class="friends-list" data-active-tab="${state.activeTab}" aria-label="Friends"><div class="tab-bar">${tabMarkup(state, friends)}<button class="button primary add-friend-button" data-add-friend type="button">Add Friend</button></div><label for="friend-list-search">Search friends</label><input id="friend-list-search" data-friend-search type="search" value="${escapeHtml(state.searchQuery)}" placeholder="Search friends" autocomplete="off"/><div data-friend-list-results>${empty}</div></section>`;
}

export function friendPageMarkup(friend: Friend): string {
  return `<section class="friend-page" data-friend-page="${escapeHtml(friend.id)}"><button data-friend-page-back type="button">Back</button><h2>${escapeHtml(friend.name)}</h2></section>`;
}

export function renderFriendListScreen(friends: readonly Friend[], state: FriendListState): string {
  if (state.screen === "list") return friendListMarkup(friends, state);
  const friend = friends.find((candidate) => candidate.id === state.openFriendId);
  if (!friend) throw new Error("Open friend no longer exists");
  return friendPageMarkup(friend);
}

export interface FriendListSearchController {
  readonly state: () => FriendListState;
  readonly render: () => void;
}

/** Bind a mounted Friends-list container without reloading or losing list state. */
export function attachFriendListSearch(
  root: HTMLElement,
  friends: readonly Friend[],
  startingState: FriendListState = initialFriendListState(),
): FriendListSearchController {
  let state = startingState;
  const render = (): void => {
    root.innerHTML = renderFriendListScreen(friends, state);
    const search = root.querySelector<HTMLInputElement>("[data-friend-search]");
    search?.addEventListener("input", () => {
      state = setFriendListSearch(state, search.value);
      render();
    });
    root.querySelectorAll<HTMLButtonElement>("[data-friends-tab]").forEach((tab) => {
      tab.addEventListener("click", () => {
        state = setFriendListTab(state, tab.dataset.friendsTab as FriendsTabId);
        render();
      });
    });
    root.querySelectorAll<HTMLButtonElement>("[data-open-friend]").forEach((row) => {
      row.addEventListener("click", () => {
        state = openFriendPage(friends, state, row.dataset.openFriend ?? "");
        render();
      });
    });
    root.querySelector<HTMLButtonElement>("[data-friend-page-back]")?.addEventListener("click", () => {
      state = backToFriendList(state);
      render();
    });
  };
  render();
  return { state: () => state, render };
}
