import { describe, expect, it } from "vitest";
import {
  FRIEND_WHITELIST_EVERYWHERE_SELECTOR,
  FRIEND_WHITELIST_NOWHERE_SELECTOR,
  friendWhitelistEverywhereButtonMarkup,
  friendWhitelistNowhereButtonMarkup,
} from "./ui-behavior";

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[char] ?? char);
}

function friendPageFixtureMarkup(personId: string): string {
  return `<article class="person-row person-profile"><details class="friend-management"><summary>Manage</summary><div class="friend-whitelist-reach">${friendWhitelistEverywhereButtonMarkup(personId, escapeHtml)}${friendWhitelistNowhereButtonMarkup(personId, escapeHtml)}</div></details></article>`;
}

describe("task 0260 - friend-wide whitelist buttons", () => {
  it("renders both Whitelist everywhere and Whitelist nowhere buttons on one friend page", () => {
    const page = friendPageFixtureMarkup("hub-person-task-0260");

    expect(page).toContain("Whitelist everywhere");
    expect(page).toContain("Whitelist nowhere");
    expect(page).toContain('data-whitelist-everywhere-person="hub-person-task-0260"');
    expect(page).toContain('data-whitelist-nowhere-person="hub-person-task-0260"');
    expect(FRIEND_WHITELIST_EVERYWHERE_SELECTOR).toBe("[data-whitelist-everywhere-person]");
    expect(FRIEND_WHITELIST_NOWHERE_SELECTOR).toBe("[data-whitelist-nowhere-person]");
    expect(page.match(/data-whitelist-everywhere-person=/g)?.length).toBe(1);
    expect(page.match(/data-whitelist-nowhere-person=/g)?.length).toBe(1);
  });
});
