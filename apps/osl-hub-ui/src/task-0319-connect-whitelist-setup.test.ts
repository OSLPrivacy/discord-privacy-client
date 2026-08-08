import { describe, expect, it, vi } from "vitest";
import {
  bindWhitelistSetupControls,
  openWhitelistSetup,
  whitelistSetupMarkup,
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
  saveCount = 0;

  async load(): Promise<WhitelistSetupSavedState | null> {
    return this.saved === null ? null : {
      allowedConversationIds: [...this.saved.allowedConversationIds],
      newlyFoundConversationRule: this.saved.newlyFoundConversationRule,
    };
  }

  async save(state: WhitelistSetupSavedState): Promise<void> {
    this.saveCount += 1;
    this.saved = {
      allowedConversationIds: [...state.allowedConversationIds],
      newlyFoundConversationRule: state.newlyFoundConversationRule,
    };
  }
}

const conversations = [
  { id: "discord:alpha", account: "Discord · Personal", name: "Alpha room", kind: "Channel" },
  { id: "signal:bravo", account: "Signal · Personal", name: "Bravo", kind: "Direct messages" },
  { id: "email:charlie", account: "Mail · Work", name: "Charlie project", kind: "Thread" },
] as const;

function controls() {
  const search = new FakeControl();
  const selectAll = new FakeControl();
  const clearAll = new FakeControl();
  const alpha = new FakeControl("discord:alpha");
  const bravo = new FakeControl("signal:bravo");
  const charlie = new FakeControl("email:charlie");
  const deny = new FakeControl("deny", true);
  const ask = new FakeControl("ask");
  const count = new FakeControl();
  const error = new FakeControl();
  const continueButton = new FakeControl();
  const back = new FakeControl();
  const alphaRow = new FakeControl();
  const bravoRow = new FakeControl();
  const charlieRow = new FakeControl();
  const root = new FakeRoot({
    "#whitelist-setup-search": [search],
    "#select-all-whitelist-setup": [selectAll],
    "#clear-all-whitelist-setup": [clearAll],
    'input[name="whitelist-setup-conversation"]': [alpha, bravo, charlie],
    'input[name="whitelist-setup-rule"]': [deny, ask],
    "#whitelist-setup-count": [count],
    "#whitelist-setup-error": [error],
    "#continue-whitelist-setup": [continueButton],
    "#back-whitelist-setup": [back],
    '[data-whitelist-setup-row="discord:alpha"]': [alphaRow],
    '[data-whitelist-setup-row="signal:bravo"]': [bravoRow],
    '[data-whitelist-setup-row="email:charlie"]': [charlieRow],
  });
  return { root, search, selectAll, clearAll, alpha, bravo, charlie, deny, ask, count, continueButton, back, alphaRow, bravoRow, charlieRow };
}

describe("TASK 0319 connected whitelist setup", () => {
  it("saves Ask and exactly two conversations, restores them on reopen, and keeps fresh-profile defaults", async () => {
    const profile = new ProfileStore();
    const fresh = await openWhitelistSetup(conversations, profile);
    expect(fresh.newlyFoundConversationRule).toBe("deny");
    expect(whitelistSetupSelectedCount(fresh)).toBe(0);

    const ui = controls();
    const continued = vi.fn();
    const back = vi.fn();
    bindWhitelistSetupControls(ui.root as unknown as ParentNode, fresh, profile, {
      onContinue: continued,
      onBack: back,
    });

    ui.search.value = "alpha";
    await ui.search.dispatch("input");
    expect(ui.alphaRow.hidden).toBe(false);
    expect(ui.bravoRow.hidden).toBe(true);
    expect(ui.charlieRow.hidden).toBe(true);
    expect(whitelistSetupSelectedCount(fresh)).toBe(0);

    await ui.selectAll.dispatch("click");
    expect(fresh.draft).toEqual(["discord:alpha"]);
    await ui.clearAll.dispatch("click");
    expect(fresh.draft).toEqual([]);

    ui.search.value = "";
    await ui.search.dispatch("input");
    ui.alpha.checked = true;
    await ui.alpha.dispatch("change");
    ui.charlie.checked = true;
    await ui.charlie.dispatch("change");
    expect(whitelistSetupSelectedCount(fresh)).toBe(2);
    expect(ui.count.textContent).toBe("2 selected conversations");

    ui.ask.checked = true;
    await ui.ask.dispatch("change");
    expect(fresh.newlyFoundConversationRule).toBe("ask");
    await ui.continueButton.dispatch("click");
    expect(profile.saveCount).toBe(1);
    expect(continued).toHaveBeenCalledWith({
      allowedConversationIds: ["discord:alpha", "email:charlie"],
      newlyFoundConversationRule: "ask",
    });

    const reopened = await openWhitelistSetup(conversations, profile);
    const reopenedMarkup = whitelistSetupMarkup(reopened);
    expect(reopened.newlyFoundConversationRule).toBe("ask");
    expect(whitelistSetupSelectedCount(reopened)).toBe(2);
    expect(reopenedMarkup).toContain('name="whitelist-setup-rule" value="ask" checked');
    expect(reopenedMarkup.match(/name="whitelist-setup-conversation"[^>]* checked/gu)).toHaveLength(2);
    expect(reopenedMarkup).not.toContain('name="whitelist-setup-rule" value="deny" checked');
    expect(reopenedMarkup).not.toContain("0 selected conversations");

    const freshProfile = await openWhitelistSetup(conversations, new ProfileStore());
    expect(freshProfile.newlyFoundConversationRule).toBe("deny");
    expect(whitelistSetupSelectedCount(freshProfile)).toBe(0);

    await ui.back.dispatch("click");
    expect(back).toHaveBeenCalledTimes(1);
    expect(profile.saveCount).toBe(1);

    console.info("TASK0319_REOPENED_RULE=Ask");
    console.info("TASK0319_REOPENED_SELECTED_CONVERSATIONS=2");
    console.info("TASK0319_REOPENED_DEFAULT_RULE_VISIBLE=false");
    console.info("TASK0319_REOPENED_ZERO_SELECTED_VISIBLE=false");
    console.info("TASK0319_FRESH_RULE=Deny");
    console.info("TASK0319_FRESH_SELECTED_CONVERSATIONS=0");
    console.info("TASK0319_BACK_SAVE_COUNT=0");
  });
});
