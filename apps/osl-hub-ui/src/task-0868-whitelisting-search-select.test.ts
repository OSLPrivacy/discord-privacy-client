import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  whitelistingClearAll,
  whitelistingScreenView,
  whitelistingSelectAll,
  whitelistingSetSearch,
  type WhitelistingConversation,
  type WhitelistingScreenState,
} from "./whitelisting-screen";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

// 40 conversations: 6 whose name/account/kind contains "elm" (the search
// term), 34 that do not. Matches the scale named in TASK 0868's finish line.
const ELM_NAMES = [
  "Elm Studio",
  "Elmstead Project Room",
  "North Elm Annex",
  "Helmi Torres Group",
  "Anselm Ward Circle",
  "Selma Price Chat",
];

function buildConversations(): WhitelistingConversation[] {
  const conversations: WhitelistingConversation[] = ELM_NAMES.map((name, index) => ({
    id: `elm-${index}`,
    account: "Discord · @ada.lovelace",
    name,
    kind: "Group",
  }));
  for (let index = 0; index < 34; index += 1) {
    conversations.push({
      id: `plain-${index}`,
      account: "Signal · +44 7700 900461",
      name: `Plain Conversation ${index}`,
      kind: "Direct messages",
    });
  }
  return conversations;
}

function state(overrides: Partial<WhitelistingScreenState> = {}): WhitelistingScreenState {
  return {
    conversations: buildConversations(),
    saved: [],
    draft: [],
    search: "",
    busy: false,
    ...overrides,
  };
}

describe("TASK0868 search connected to Select all / Clear all", () => {
  it("the shipped search box drives whitelistingSetSearch, and Select all/Clear all read the live search", () => {
    expect(mainSource).toContain("#whitelisting-search");
    expect(mainSource).toContain("whitelistingSetSearch(whitelistingScreenState(), input.value)");
    expect(mainSource).toContain("whitelistingSelectAll(whitelistingScreenState())");
    expect(mainSource).toContain("whitelistingClearAll(whitelistingScreenState())");
  });

  it("with a search showing 6 of 40 rows, Select all selects exactly 6 and leaves the other 34 unchanged", () => {
    const base = state();
    expect(base.conversations).toHaveLength(40);

    const searched = whitelistingSetSearch(base, "elm");
    const view = whitelistingScreenView(searched);
    console.log(`TASK0868 total=${base.conversations.length} matches=${view.matchCount} query=${searched.search}`);
    expect(view.matchCount).toBe(6);
    expect(view.totalCount).toBe(40);

    const before = new Set(searched.draft);
    const hiddenBefore = base.conversations
      .filter((conversation) => !view.matches.some((match) => match.id === conversation.id))
      .map((conversation) => ({ id: conversation.id, allowed: before.has(conversation.id) }));
    expect(hiddenBefore).toHaveLength(34);

    const selected = whitelistingSelectAll(searched);
    console.log(`TASK0868 select_all_draft_count=${selected.draft.length} selected=${selected.draft.join(",")}`);
    expect(selected.draft).toHaveLength(6);
    expect(new Set(selected.draft)).toEqual(new Set(view.matches.map((match) => match.id)));

    const hiddenAfter = base.conversations
      .filter((conversation) => !view.matches.some((match) => match.id === conversation.id))
      .map((conversation) => ({ id: conversation.id, allowed: selected.draft.includes(conversation.id) }));
    console.log(`TASK0868 hidden_unchanged=${JSON.stringify(hiddenAfter) === JSON.stringify(hiddenBefore)} hidden_count=${hiddenAfter.length}`);
    expect(hiddenAfter).toEqual(hiddenBefore);
    expect(hiddenAfter.every((row) => row.allowed === false)).toBe(true);
  });

  it("with the same search, Clear all clears exactly the visible 6 and leaves already-allowed hidden rows allowed", () => {
    const preAllowed = buildConversations().map((conversation) => conversation.id);
    const base = state({ saved: preAllowed, draft: preAllowed });
    const searched = whitelistingSetSearch(base, "elm");
    const view = whitelistingScreenView(searched);
    expect(view.matchCount).toBe(6);

    const before = new Set(searched.draft);
    const hiddenBefore = base.conversations
      .filter((conversation) => !view.matches.some((match) => match.id === conversation.id))
      .map((conversation) => ({ id: conversation.id, allowed: before.has(conversation.id) }));
    expect(hiddenBefore).toHaveLength(34);
    expect(hiddenBefore.every((row) => row.allowed === true)).toBe(true);

    const cleared = whitelistingClearAll(searched);
    console.log(`TASK0868 clear_all_draft_count=${cleared.draft.length}`);
    expect(cleared.draft).toHaveLength(34);

    const hiddenAfter = base.conversations
      .filter((conversation) => !view.matches.some((match) => match.id === conversation.id))
      .map((conversation) => ({ id: conversation.id, allowed: cleared.draft.includes(conversation.id) }));
    expect(hiddenAfter).toEqual(hiddenBefore);
  });
});
