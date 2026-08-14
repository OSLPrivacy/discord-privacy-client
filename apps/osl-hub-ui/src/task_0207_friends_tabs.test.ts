import { describe, expect, it } from "vitest";
import {
  friendsTabsFixtureMarkup,
  friendsTabsMarkup,
} from "./friends-tabs";

interface TabFacts {
  tab: string;
  label: string;
}

interface AddButtonFacts {
  label: string;
}

function tabFacts(html: string): TabFacts[] {
  return [...html.matchAll(/<button\b([^>]*)data-friends-tab="([^"]*)"\b([^>]*)>([\s\S]*?)<\/button>/gu)]
    .map((match) => ({
      tab: match[2],
      label: match[4].trim(),
    }));
}

function addButtonFacts(html: string): AddButtonFacts[] {
  return [...html.matchAll(/<button\b([^>]*)data-add-friend-action="add-friend"\b([^>]*)>([\s\S]*?)<\/button>/gu)]
    .map((match) => ({
      label: match[3].trim(),
    }));
}

describe("TASK 0207 friends tabs fixture", () => {
  it("renders all four tab labels and an Add Friend button", () => {
    const fixture = friendsTabsFixtureMarkup();
    const tabs = tabFacts(fixture);
    const buttons = addButtonFacts(fixture);

    for (const tab of tabs) {
      console.log(`TASK0207_TAB tab=${tab.tab} label="${tab.label}"`);
    }
    console.log(`TASK0207_BUTTON label="${buttons[0]?.label ?? "missing"}"`);
    console.log(`TASK0207_DONE tabs=${tabs.map((tab) => tab.tab).join(",")} button=${buttons.length}`);

    expect(fixture).toContain('data-ui-fixture="task-0207-friends-tabs"');
    expect(tabs).toHaveLength(4);
    expect(tabs.map((tab) => tab.tab)).toEqual([
      "online",
      "all",
      "pending",
      "blocked",
    ]);
    expect(tabs.map((tab) => tab.label)).toEqual([
      "Online",
      "All",
      "Pending",
      "Blocked",
    ]);

    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toMatchObject({ label: "Add Friend" });
  });

  it("reflects the active tab in the production markup", () => {
    const markup = friendsTabsMarkup({ activeTab: "pending" });
    const tabs = tabFacts(markup);
    const pending = tabs.find((tab) => tab.tab === "pending");
    expect(pending).toBeDefined();
    expect(markup).toContain('data-friends-tab="pending"');
  });
});
