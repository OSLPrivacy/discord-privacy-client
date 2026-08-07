/**
 * Task 0207: the four friends tabs.
 *
 * The fixture renders all four tab labels and the Add Friend button with
 * fixture counts from the backend query (task 0204). The assertions check:
 * - All four tab labels appear: Online, All, Pending, Blocked
 * - The Add Friend button is present
 * - Each tab displays its count
 */

import { describe, expect, it } from "vitest";
import {
  blankFriendsTabsModel,
  friendsTabsMarkup,
  type FriendsTabCounts,
} from "./friends-tabs";

function countedMarkup(counts: FriendsTabCounts): string {
  return friendsTabsMarkup({
    activeTab: "all",
    counts,
  });
}

describe("task 0207: friends tabs", () => {
  it("renders all four tab labels and the Add Friend button", () => {
    const markup = countedMarkup({ online: 1, all: 2, pending: 1, blocked: 1 });
    expect(markup).toContain("Online");
    expect(markup).toContain("All");
    expect(markup).toContain("Pending");
    expect(markup).toContain("Blocked");
    expect(markup).toMatch(/<button[^>]*data-add-friend[^>]*>Add Friend<\/button>/u);
  });

  it("displays the correct count for each tab", () => {
    const counts = { online: 1, all: 2, pending: 1, blocked: 1 };
    const markup = countedMarkup(counts);
    expect(markup).toContain('<span class="tab-count">1</span>');
    expect(markup).toContain('<span class="tab-count">2</span>');
    expect([...markup.matchAll(/<span class="tab-count">\d+<\/span>/gu)]).toHaveLength(4);
  });

  it("renders all four tab buttons with data attributes", () => {
    const markup = countedMarkup({ online: 0, all: 0, pending: 0, blocked: 0 });
    expect(markup).toContain('data-friends-tab="online"');
    expect(markup).toContain('data-friends-tab="all"');
    expect(markup).toContain('data-friends-tab="pending"');
    expect(markup).toContain('data-friends-tab="blocked"');
  });

  it("marks the active tab with the active class", () => {
    const markup = friendsTabsMarkup({
      activeTab: "pending",
      counts: { online: 1, all: 2, pending: 5, blocked: 0 },
    });
    const activeMatch = markup.match(/class="friends-tab active"[^>]*data-friends-tab="pending"/u);
    expect(activeMatch).toBeTruthy();
  });

  it("renders with zero counts from blank model", () => {
    const markup = friendsTabsMarkup(blankFriendsTabsModel());
    expect(markup).toContain("Online");
    expect(markup).toContain("All");
    expect(markup).toContain("Pending");
    expect(markup).toContain("Blocked");
    expect(markup).toMatch(/<button[^>]*data-add-friend[^>]*>Add Friend<\/button>/u);
    console.log(
      `TASK_0207_FIXTURES labels=Online,All,Pending,Blocked button=Add Friend tabs=4 counts_zero=true`,
    );
  });
});
