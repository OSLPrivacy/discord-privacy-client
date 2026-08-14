import { describe, expect, it } from "vitest";
import {
  bindWhitelistSetupControls,
  openWhitelistSetup,
  whitelistSetupSelectedCount,
  type WhitelistSetupSavedState,
  type WhitelistSetupStore,
} from "./whitelist-setup";

type Listener = () => void | Promise<void>;

class FakeControl {
  checked: boolean;
  disabled = false;
  hidden = false;
  textContent = "";
  value: string;
  private readonly listeners = new Map<string, Listener[]>();

  constructor(value = "", checked = false) {
    this.value = value;
    this.checked = checked;
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  async dispatch(type: string): Promise<void> {
    for (const listener of this.listeners.get(type) ?? []) await listener();
  }
}

class FakeRoot {
  constructor(private readonly selectors: Record<string, FakeControl[]>) {}

  querySelector<T>(selector: string): T | null {
    return (this.selectors[selector]?.[0] ?? null) as T | null;
  }

  querySelectorAll<T>(selector: string): T[] {
    return (this.selectors[selector] ?? []) as T[];
  }
}

class ProfileStore implements WhitelistSetupStore {
  saved: WhitelistSetupSavedState | null = null;
  saves = 0;

  async load(): Promise<WhitelistSetupSavedState | null> {
    return this.saved;
  }

  async save(state: WhitelistSetupSavedState): Promise<void> {
    this.saves += 1;
    this.saved = { allowedConversationIds: [...state.allowedConversationIds], newlyFoundConversationRule: state.newlyFoundConversationRule };
  }
}

const conversations = [
  { id: "alpha", account: "Discord · Personal", name: "Alpha room", kind: "Channel" },
  { id: "bravo", account: "Signal · Personal", name: "Bravo chat", kind: "Direct messages" },
] as const;

function controls() {
  const search = new FakeControl();
  const selectAll = new FakeControl();
  const clearAll = new FakeControl();
  const alpha = new FakeControl("alpha");
  const bravo = new FakeControl("bravo");
  const deny = new FakeControl("deny", true);
  const ask = new FakeControl("ask");
  const invalid = new FakeControl("always");
  const count = new FakeControl();
  const error = new FakeControl();
  const continueButton = new FakeControl();
  const back = new FakeControl();
  const alphaRow = new FakeControl();
  const bravoRow = new FakeControl();
  const root = new FakeRoot({
    "#whitelist-setup-search": [search],
    "#select-all-whitelist-setup": [selectAll],
    "#clear-all-whitelist-setup": [clearAll],
    'input[name="whitelist-setup-conversation"]': [alpha, bravo],
    'input[name="whitelist-setup-rule"]': [deny, ask, invalid],
    "#whitelist-setup-count": [count],
    "#whitelist-setup-error": [error],
    "#continue-whitelist-setup": [continueButton],
    "#back-whitelist-setup": [back],
    '[data-whitelist-setup-row="alpha"]': [alphaRow],
    '[data-whitelist-setup-row="bravo"]': [bravoRow],
  });
  return { root, search, selectAll, clearAll, alpha, bravo, deny, ask, invalid, count, continueButton, back, alphaRow, bravoRow };
}

describe("TASK 0351 whitelist setup controls", () => {
  it("executes every named control and refuses an invalid rule without changing the setup", async () => {
    const store = new ProfileStore();
    const draft = await openWhitelistSetup(conversations, store);
    const ui = controls();
    let page = "whitelist setup";
    let continued: WhitelistSetupSavedState | null = null;
    bindWhitelistSetupControls(ui.root as unknown as ParentNode, draft, store, {
      onContinue: (saved) => { continued = saved; page = "next setup"; },
      onBack: () => { page = "Scrub setup"; },
    });

    // Each named conversation tick both selects and clears only that conversation.
    ui.alpha.checked = true;
    await ui.alpha.dispatch("change");
    expect(draft.draft).toEqual(["alpha"]);
    ui.alpha.checked = false;
    await ui.alpha.dispatch("change");
    expect(draft.draft).toEqual([]);
    ui.bravo.checked = true;
    await ui.bravo.dispatch("change");
    expect(draft.draft).toEqual(["bravo"]);
    ui.bravo.checked = false;
    await ui.bravo.dispatch("change");
    expect(draft.draft).toEqual([]);

    await ui.selectAll.dispatch("click");
    expect(draft.draft).toEqual(["alpha", "bravo"]);
    await ui.clearAll.dispatch("click");
    expect(draft.draft).toEqual([]);
    expect(whitelistSetupSelectedCount(draft)).toBe(0);

    ui.search.value = "Alpha";
    await ui.search.dispatch("input");
    expect(ui.alphaRow.hidden).toBe(false);
    expect(ui.bravoRow.hidden).toBe(true);
    expect(whitelistSetupSelectedCount(draft)).toBe(0);

    // An unrecognised rule is ignored: it cannot change selection, rule, page, or persistence.
    ui.invalid.checked = true;
    await ui.invalid.dispatch("change");
    expect(draft.newlyFoundConversationRule).toBe("deny");
    expect(whitelistSetupSelectedCount(draft)).toBe(0);
    expect(page).toBe("whitelist setup");
    expect(store.saves).toBe(0);

    ui.search.value = "";
    await ui.search.dispatch("input");
    ui.alpha.checked = true;
    await ui.alpha.dispatch("change");
    ui.bravo.checked = true;
    await ui.bravo.dispatch("change");
    ui.ask.checked = true;
    await ui.ask.dispatch("change");
    expect(draft.newlyFoundConversationRule).toBe("ask");
    expect(ui.count.textContent).toBe("2 selected conversations");

    await ui.continueButton.dispatch("click");
    expect(continued).toEqual({ allowedConversationIds: ["alpha", "bravo"], newlyFoundConversationRule: "ask" });
    expect(store.saved).toEqual(continued);
    expect(page).toBe("next setup");

    await ui.back.dispatch("click");
    expect(page).toBe("Scrub setup");

    console.info("TASK0351_TICKS=alpha:select+clear,bravo:select+clear");
    console.info("TASK0351_SELECT_ALL=2");
    console.info("TASK0351_CLEAR_ALL=0");
    console.info("TASK0351_SEARCH=Alpha room only; selected=0");
    console.info("TASK0351_RULE=ask");
    console.info("TASK0351_CONTINUE=next setup; selected=alpha,bravo; rule=ask");
    console.info("TASK0351_BACK=Scrub setup");
    console.info("TASK0351_INVALID_RULE=refused; selected=0; rule=deny; page=whitelist setup");
  });
});
