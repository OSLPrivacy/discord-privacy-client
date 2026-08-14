/**
 * Task 0207: the four friends tabs.
 *
 * A tab bar with Online, All, Pending, and Blocked tabs, each naming the count
 * for that category from the backend's `cmd_osl_query_friends_tabs` (task 0204).
 * An Add Friend button sits to the right of the tabs.
 */

export type FriendsTabId = "online" | "all" | "pending" | "blocked";

export const FRIENDS_TAB_IDS: readonly FriendsTabId[] = ["online", "all", "pending", "blocked"] as const;

export const FRIENDS_TAB_LABELS: Readonly<Record<FriendsTabId, string>> = {
  online: "Online",
  all: "All",
  pending: "Pending",
  blocked: "Blocked",
};

export interface FriendsTabCounts {
  readonly online: number;
  readonly all: number;
  readonly pending: number;
  readonly blocked: number;
}

export interface FriendsTabsModel {
  readonly activeTab: FriendsTabId;
  readonly counts: FriendsTabCounts;
}

export type FriendsTab = FriendsTabId;

export function blankFriendsTabsModel(): FriendsTabsModel {
  return {
    activeTab: "all",
    counts: { online: 0, all: 0, pending: 0, blocked: 0 },
  };
}

export function friendsTabsMarkup(model: Pick<FriendsTabsModel, "activeTab"> & Partial<FriendsTabsModel>): string {
  const counts = model.counts ?? blankFriendsTabsModel().counts;
  const tabs = FRIENDS_TAB_IDS.map(
    (tabId) =>
      `<button class="friends-tab${model.activeTab === tabId ? " active" : ""}" data-friends-tab="${tabId}" type="button">${FRIENDS_TAB_LABELS[tabId]} <span class="tab-count">${counts[tabId]}</span></button>`,
  ).join("");

  return `<section class="friends-tabs" aria-label="Friends tabs"><div class="tab-bar">${tabs}<button class="button primary add-friend-button" data-add-friend type="button">Add Friend</button></div></section>`;
}

export function friendsTabsFixtureMarkup(): string {
  return `<section data-ui-fixture="task-0207-friends-tabs">${FRIENDS_TAB_IDS.map((tab) => `<button data-friends-tab="${tab}" type="button">${FRIENDS_TAB_LABELS[tab]}</button>`).join("")}<button data-add-friend-action="add-friend" type="button">Add Friend</button></section>`;
}
