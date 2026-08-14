import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  whitelistingClearAll,
  whitelistingPendingChanges,
  whitelistingReset,
  whitelistingSaved,
  whitelistingScreenMarkup,
  whitelistingScreenView,
  whitelistingSelectAll,
  whitelistingSetSearch,
  whitelistingToggleConversation,
  type WhitelistingScreenState,
} from "./whitelisting-screen";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

const CONVERSATIONS = [
  { id: "d-study-circle", account: "Discord · @ada.lovelace", name: "Study Circle", kind: "Group" },
  { id: "d-study-beta", account: "Discord · @ada.lovelace", name: "Study Group Beta", kind: "Group" },
  { id: "s-weekend-study", account: "Signal · +44 7700 900461", name: "Weekend Study", kind: "Group" },
  { id: "s-family", account: "Signal · +44 7700 900461", name: "Family", kind: "Group" },
  { id: "d-standup", account: "Discord · @ada.lovelace", name: "Team Standup", kind: "Channel" },
] as const;

function state(overrides: Partial<WhitelistingScreenState> = {}): WhitelistingScreenState {
  return {
    conversations: CONVERSATIONS,
    saved: ["d-study-circle", "s-family"],
    draft: ["d-study-circle", "s-family"],
    search: "",
    busy: false,
    ...overrides,
  };
}

function ticks(markup: string): Array<{ id: string; checked: boolean }> {
  return [...markup.matchAll(/<li class="whitelisting-row[^"]*" data-whitelisting-row="([^"]+)" data-whitelisting-allowed="(true|false)"/gu)]
    .map((match) => ({ id: match[1] ?? "", checked: match[2] === "true" }));
}

describe("TASK0764 Whitelisting screen", () => {
  it("ships the screen from Settings, not only from the capture fixture", () => {
    expect(mainSource).toContain('import { whitelistingClearAll,');
    expect(mainSource).toContain("whitelistingScreenMarkup(whitelistingScreenState())");
    expect(mainSource).toContain("[data-whitelisting-select-all]");
    expect(mainSource).toContain("[data-whitelisting-clear-all]");
    expect(mainSource).toContain("[data-whitelisting-save]");
    expect(mainSource).toContain("[data-whitelisting-reset]");
    expect(mainSource).toContain("#whitelisting-search");
  });

  it("renders all six controls and one row per conversation", () => {
    const markup = whitelistingScreenMarkup(state());
    const rows = ticks(markup);
    console.log(`TASK0764 rows=${rows.length} ticked=${rows.filter((row) => row.checked).length}`);

    expect(rows).toHaveLength(5);
    expect(markup).toContain('id="whitelisting-search"');
    expect(markup).toContain("data-whitelisting-select-all");
    expect(markup).toContain("data-whitelisting-clear-all");
    expect(markup).toContain(">Save<");
    expect(markup).toContain(">Reset<");
    expect(markup).toContain(">Whitelisting<");
  });

  it("search narrows the list and reports what it found", () => {
    const view = whitelistingScreenView(whitelistingSetSearch(state(), "study"));
    console.log(`TASK0764 search=study matches=${view.matchCount}/${view.totalCount} line=${view.resultLine}`);

    expect(view.matches.map((conversation) => conversation.id))
      .toEqual(["d-study-circle", "d-study-beta", "s-weekend-study"]);
    expect(view.matchCount).toBe(3);
    expect(view.totalCount).toBe(5);
    expect(view.resultLine).toBe('3 of 5 conversations match "study" · 1 allowed, 2 not allowed');
    expect(view.mixed).toBe(true);
  });

  it("select all and clear all act on the search result, never on hidden rows", () => {
    const searched = whitelistingSetSearch(state(), "study");

    const selected = whitelistingSelectAll(searched);
    console.log(`TASK0764 select_all draft=${selected.draft.join(",")}`);
    // s-family is not in the search result and stays ticked; d-standup is not
    // in it either and stays unticked.
    expect(selected.draft).toEqual(["d-study-circle", "d-study-beta", "s-weekend-study", "s-family"]);

    const cleared = whitelistingClearAll(searched);
    console.log(`TASK0764 clear_all draft=${cleared.draft.join(",")}`);
    expect(cleared.draft).toEqual(["s-family"]);
  });

  it("nothing is written until Save, and Reset puts the saved answer back", () => {
    const edited = whitelistingToggleConversation(
      whitelistingToggleConversation(state(), "d-study-circle", false),
      "d-standup",
      true,
    );
    const editedView = whitelistingScreenView(edited);
    const pending = whitelistingPendingChanges(edited);
    console.log(`TASK0764 dirty=${editedView.dirty} changes=${editedView.changeCount} allow=${pending.allow.join(",")} remove=${pending.remove.join(",")}`);

    expect(edited.saved).toEqual(["d-study-circle", "s-family"]);
    expect(editedView.dirty).toBe(true);
    expect(editedView.changeCount).toBe(2);
    expect(pending.allow).toEqual(["d-standup"]);
    expect(pending.remove).toEqual(["d-study-circle"]);
    expect(editedView.saveEnabled).toBe(true);
    expect(editedView.resetEnabled).toBe(true);

    const reset = whitelistingReset(edited);
    const resetView = whitelistingScreenView(reset);
    console.log(`TASK0764 after_reset draft=${reset.draft.join(",")} dirty=${resetView.dirty}`);
    expect(reset.draft).toEqual(["d-study-circle", "s-family"]);
    expect(resetView.dirty).toBe(false);
    expect(resetView.saveEnabled).toBe(false);
    expect(resetView.resetEnabled).toBe(false);

    const savedState = whitelistingSaved(edited);
    const savedView = whitelistingScreenView(savedState);
    console.log(`TASK0764 after_save saved=${savedState.saved.join(",")} dirty=${savedView.dirty}`);
    expect(savedState.saved).toEqual(["s-family", "d-standup"]);
    expect(savedView.dirty).toBe(false);
  });

  it("Reset is not Clear all: it restores ticks instead of removing them", () => {
    const cleared = whitelistingClearAll(state());
    const restored = whitelistingReset(cleared);
    console.log(`TASK0764 clear_all_draft=${cleared.draft.length} reset_draft=${restored.draft.join(",")}`);

    expect(cleared.draft).toEqual([]);
    expect(restored.draft).toEqual(["d-study-circle", "s-family"]);
  });

  it("the empty screen offers an empty state and no live controls", () => {
    const empty = state({ conversations: [], saved: [], draft: [] });
    const view = whitelistingScreenView(empty);
    const markup = whitelistingScreenMarkup(empty);
    console.log(`TASK0764 empty rows=${ticks(markup).length} line=${view.resultLine}`);

    expect(ticks(markup)).toHaveLength(0);
    expect(markup).toContain("data-whitelisting-empty");
    expect(view.resultLine).toBe("No conversations to search yet.");
    expect(view.selectAllEnabled).toBe(false);
    expect(view.clearAllEnabled).toBe(false);
    expect(view.saveEnabled).toBe(false);
    expect(view.resetEnabled).toBe(false);
  });

  it("escapes conversation names instead of letting them close a tag", () => {
    const markup = whitelistingScreenMarkup(state({
      conversations: [{ id: "x", account: "Discord", name: '</strong><script>alert(1)</script>', kind: "Group" }],
      saved: [],
      draft: [],
    }));
    expect(markup).not.toContain("<script>");
    expect(markup).toContain("&lt;script&gt;");
  });
});
