import {
  FRIENDS_TAB_IDS,
  FRIENDS_TAB_LABELS,
  friendsTabsMarkup,
  type FriendsTabId,
} from "./friends-tabs";

/** One representative query row for every category returned by task 0208. */
export const TASK_0209_ROWS: Readonly<Record<FriendsTabId, Readonly<{ name: string; detail: string; state: string; initial: string; tone: string }>>> = {
  online: { name: "Ari Online", detail: "Available now", state: "Online", initial: "A", tone: "ok" },
  all: { name: "Bea Friend", detail: "Friend since today", state: "Friend", initial: "B", tone: "active" },
  pending: { name: "Casey Pending", detail: "Awaiting your response", state: "Pending", initial: "C", tone: "warn" },
  blocked: { name: "Devon Blocked", detail: "No messages or requests", state: "Blocked", initial: "D", tone: "danger" },
};

let activeTab: FriendsTabId = "online";

function counts() {
  return Object.fromEntries(FRIENDS_TAB_IDS.map((tab) => [tab, 1])) as Record<FriendsTabId, number>;
}

function rowMarkup(tab: FriendsTabId): string {
  const row = TASK_0209_ROWS[tab];
  return `<article class="friend-tab-row" data-friend-row="${tab}" aria-label="${row.name}, ${row.state}">
    <span class="friend-tab-avatar" aria-hidden="true">${row.initial}</span>
    <div><span class="friend-tab-name">${row.name}</span><span class="friend-tab-detail">${row.detail}</span></div>
    <span class="status-tag ${row.tone}" data-friend-state="${tab}">${row.state}</span>
  </article>`;
}

function render(): void {
  document.querySelector("#app")!.innerHTML = `<main class="friends-tabs-fixture">
    <p class="eyebrow">People</p>
    <h1>Friends</h1>
    <p class="fixture-intro">Choose a tab to review its friends.</p>
    ${friendsTabsMarkup({ activeTab, counts: counts() })}
    <section class="friends-tab-results" aria-live="polite" aria-label="${FRIENDS_TAB_LABELS[activeTab]} friends">
      ${rowMarkup(activeTab)}
    </section>
  </main>`;

  document.querySelectorAll<HTMLButtonElement>("[data-friends-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      activeTab = button.dataset.friendsTab as FriendsTabId;
      render();
    });
  });
}

render();
