import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  FRIENDING_VISIBILITY_STORAGE_KEY,
  FRIENDING_VISIBILITY_SWITCHES,
  activeFriendingVisibilityPreset,
  applyFriendingVisibilityPreset,
  defaultFriendingVisibilityState,
  friendingVisibilityAllows,
  friendingVisibilityMarkup,
  loadFriendingVisibility,
  saveFriendingVisibility,
  secondIdentityFriendingResultCount,
  setFriendingVisibilitySwitch,
  type FriendingVisibilityAction,
  type FriendingVisibilityState,
} from "./friending-visibility";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  };
}

function renderedRows(markup: string): string[] {
  return [...markup.matchAll(/data-friending-visibility-row="([^"]+)"/gu)].map((match) => match[1]);
}

function renderedRowNames(markup: string): string[] {
  return [...markup.matchAll(/data-friending-visibility-row="[^"]+"[\s\S]*?<strong>([^<]+)<\/strong>/gu)].map((match) => match[1]);
}

function renderedRowBindings(markup: string): Array<{ row: string; label: string; switch: string }> {
  return [...markup.matchAll(/data-friending-visibility-row="([^"]+)"[\s\S]*?<strong>([^<]+)<\/strong>[\s\S]*?data-friending-visibility-switch="([^"]+)"/gu)]
    .map((match) => ({ row: match[1], label: match[2], switch: match[3] }));
}

function litPresetChips(markup: string): string[] {
  return [...markup.matchAll(/data-friending-visibility-preset="([A-Z]+)" aria-pressed="true"/gu)].map((match) => match[1]);
}

function checkedRows(markup: string): string[] {
  return [...markup.matchAll(/data-friending-visibility-row="([^"]+)"(?:(?!<\/label>)[\s\S])*?<input[^>]*\schecked\/?/gu)].map((match) => match[1]);
}

const ACTION_BY_SWITCH: Readonly<Record<(typeof FRIENDING_VISIBILITY_SWITCHES)[number], FriendingVisibilityAction>> = {
  findableByPublicUsername: "publicUsernameSearch",
  friendRequestsAllowed: "friendRequest",
  messageRequestsAllowed: "messageRequest",
  profileViewable: "profileView",
};

const ROWS = [
  { key: "findableByPublicUsername", label: "findable by public username" },
  { key: "friendRequestsAllowed", label: "friend requests allowed" },
  { key: "messageRequestsAllowed", label: "message requests allowed" },
  { key: "profileViewable", label: "profile viewable" },
] as const;

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("TASK 6880 onboarding social visibility", () => {
  it("has exactly the four named Settings-backed rows in the required order on both surfaces", () => {
    const expected = [...FRIENDING_VISIBILITY_SWITCHES];
    const onboarding = friendingVisibilityMarkup(defaultFriendingVisibilityState(), "onboarding");
    const settings = friendingVisibilityMarkup(defaultFriendingVisibilityState(), "settings");
    for (const surface of ["onboarding", "settings"] as const) {
      const bindings = renderedRowBindings(surface === "onboarding" ? onboarding : settings);
      for (const expectedRow of ROWS) {
        expect(
          bindings.find((binding) => binding.row === expectedRow.key),
          `TASK6880_ROW_BINDING surface=${surface} row=${expectedRow.label} switch=${expectedRow.key}`,
        ).toEqual({ row: expectedRow.key, label: expectedRow.label, switch: expectedRow.key });
      }
    }
    expect(renderedRowNames(onboarding), "TASK6880_ROW_INVENTORY surface=onboarding expected=4").toEqual(ROWS.map((row) => row.label));
    expect(renderedRowNames(settings), "TASK6880_ROW_INVENTORY surface=settings expected=4").toEqual(ROWS.map((row) => row.label));
    expect(renderedRows(onboarding), "TASK6880_ROW_INVENTORY surface=onboarding expected=4").toEqual(expected);
    expect(renderedRows(settings), "TASK6880_ROW_INVENTORY surface=settings expected=4").toEqual(expected);
    expect(onboarding).toContain("Strangers see nothing about how you use OSL either way.");
    expect(onboarding).toContain("Settings, Privacy");
    expect(onboarding).toContain('id="continue-friending-visibility"');
    console.info("TASK6880_INVENTORY rows=findable by public username|friend requests allowed|message requests allowed|profile viewable count=4 settings_rows=4");
  });

  it("persists each individual named switch and lets the second identity observe only that effect after restart", () => {
    const storage = memoryStorage();
    for (const changed of FRIENDING_VISIBILITY_SWITCHES) {
      let state = defaultFriendingVisibilityState();
      state = setFriendingVisibilitySwitch(state, changed, true);
      saveFriendingVisibility(storage, state);
      const restarted = loadFriendingVisibility(storage);
      const settingsAfterRestart = friendingVisibilityMarkup(restarted, "settings");
      expect(checkedRows(settingsAfterRestart), `${changed}: Settings read after restart`).toEqual([changed]);
      for (const candidate of FRIENDING_VISIBILITY_SWITCHES) {
        expect(restarted[candidate], `${changed}: restart read for ${candidate}`).toBe(candidate === changed);
        expect(friendingVisibilityAllows(restarted, ACTION_BY_SWITCH[candidate]), `${changed}: second identity ${ACTION_BY_SWITCH[candidate]}`).toBe(candidate === changed);
        expect(secondIdentityFriendingResultCount(restarted, ACTION_BY_SWITCH[candidate]), `${changed}: second identity result count for ${ACTION_BY_SWITCH[candidate]}`).toBe(candidate === changed ? 1 : 0);
      }
      console.info(`TASK6880_ROW row=${changed} restart=${restarted[changed]} second_identity_effect=${ACTION_BY_SWITCH[changed]} allowed_results=1 other_results=0 only_own_switch=1`);
    }
    expect(FRIENDING_VISIBILITY_STORAGE_KEY).toBe("osl.friending-visibility-v1");
  });

  it("sets all four through SILENT and VISIBLE and lights only the exactly matching chip", () => {
    let state: FriendingVisibilityState = defaultFriendingVisibilityState();
    expect(activeFriendingVisibilityPreset(state)).toBe("SILENT");
    state = applyFriendingVisibilityPreset(state, "VISIBLE");
    for (const key of FRIENDING_VISIBILITY_SWITCHES) {
      expect(state[key], `TASK6880_PRESET preset=VISIBLE row=${key} switch=${key}`).toBe(true);
    }
    expect(activeFriendingVisibilityPreset(state), "TASK6880_CHIP preset=VISIBLE checkboxes=1111").toBe("VISIBLE");
    expect(litPresetChips(friendingVisibilityMarkup(state, "settings")), "TASK6880_CHIP preset=VISIBLE checkboxes=1111").toEqual(["VISIBLE"]);
    state = setFriendingVisibilitySwitch(state, "profileViewable", false);
    expect(activeFriendingVisibilityPreset(state), "TASK6880_CHIP preset=none checkboxes=1110").toBeNull();
    expect(litPresetChips(friendingVisibilityMarkup(state, "onboarding")), "TASK6880_CHIP preset=none checkboxes=1110").toEqual([]);
    state = applyFriendingVisibilityPreset(state, "SILENT");
    for (const key of FRIENDING_VISIBILITY_SWITCHES) {
      expect(state[key], `TASK6880_PRESET preset=SILENT row=${key} switch=${key}`).toBe(false);
    }
    expect(activeFriendingVisibilityPreset(state), "TASK6880_CHIP preset=SILENT checkboxes=0000").toBe("SILENT");
    expect(litPresetChips(friendingVisibilityMarkup(state, "settings")), "TASK6880_CHIP preset=SILENT checkboxes=0000").toEqual(["SILENT"]);
    console.info("TASK6880_PRESETS silent=0000 visible=1111 changed_one=1110 chips_after_change=0 return_silent=0000 lit=SILENT");
  });

  it("renders both surfaces from the one Settings record rather than an onboarding-only store", () => {
    expect(
      mainSource,
      "TASK6880_STORE surface=onboarding row=findable by public username switch=findableByPublicUsername must load Settings storage",
    ).toContain("let friendingVisibility: FriendingVisibilityState = loadFriendingVisibility(localStorage);");
    expect(
      mainSource,
      "TASK6880_STORE surface=onboarding row=findable by public username switch=findableByPublicUsername must share Settings state",
    ).toContain('if (onboardingRoute === "visibility") return friendingVisibilityMarkup(friendingVisibility, "onboarding");');
    expect(
      mainSource,
      "TASK6880_STORE surface=settings row=findable by public username switch=findableByPublicUsername must share Settings state",
    ).toContain('if (settingsSection === "privacy") return friendingVisibilityMarkup(friendingVisibility, "settings");');
    expect(
      mainSource,
      "TASK6880_STORE surface=both row=findable by public username switch=findableByPublicUsername must save one record",
    ).toContain("saveFriendingVisibility(localStorage, friendingVisibility);");
    console.info("TASK6880_STORE onboarding=settings storage=osl.friending-visibility-v1 restart=shared second_identity=shared");
  });
});
