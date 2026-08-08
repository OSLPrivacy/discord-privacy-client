import { describe, expect, it } from "vitest";
import {
  appearanceSettingsMarkup,
  appearanceStorageKey,
  defaultAppearancePreferences,
  loadAppearancePreferences,
  resetAppearancePreferences,
  saveAppearancePreferences,
} from "./appearance-preferences";

function memoryStorage(initial: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(initial));
  return { get length() { return values.size; }, clear: () => values.clear(), getItem: (key) => values.get(key) ?? null, key: (index) => [...values.keys()][index] ?? null, removeItem: (key) => { values.delete(key); }, setItem: (key, value) => { values.set(key, value); } };
}

describe("TASK 5020 Appearance", () => {
  it("persists accent, background, and avatar exactly across a restart", () => {
    const storage = memoryStorage();
    const changed = saveAppearancePreferences(storage, { ...defaultAppearancePreferences, accent: "coral", background: "paper", avatar: "spark" });
    const restarted = loadAppearancePreferences(storage);
    expect(restarted).toEqual(changed);
    expect(restarted).toMatchObject({ accent: "coral", background: "paper", avatar: "spark" });
  });

  it("renders all three changed values together in a real live message row", () => {
    let preview = { ...defaultAppearancePreferences };
    const updateTimes: number[] = [];
    for (const patch of [{ accent: "violet" }, { background: "slate" }, { avatar: "wave" }] as const) {
      const started = performance.now();
      preview = { ...preview, ...patch };
      appearanceSettingsMarkup(preview);
      updateTimes.push(performance.now() - started);
    }
    const markup = appearanceSettingsMarkup(preview);
    expect(markup).toContain('data-accent="violet"');
    expect(markup).toContain('data-background="slate"');
    expect(markup).toContain('data-avatar="wave"');
    expect(markup).toContain("preview-message-row");
    expect(markup).toContain("Let’s keep this conversation here.");
    expect((markup.match(/placeholder/giu) ?? [])).toHaveLength(0);
    expect(updateTimes.every((milliseconds) => milliseconds < 1_000)).toBe(true);
    console.log(`TASK5020 preview_updates=${updateTimes.length} under_1s=${updateTimes.filter((milliseconds) => milliseconds < 1_000).length} simultaneous_values=3 placeholder_marks=0 message=Let’s keep this conversation here.`);
  });

  it("reset changes only the Appearance record", () => {
    const storage = memoryStorage({ "other-screen": "kept" });
    saveAppearancePreferences(storage, { ...defaultAppearancePreferences, accent: "coral", background: "paper", avatar: "spark" });
    expect(resetAppearancePreferences(storage)).toEqual(defaultAppearancePreferences);
    expect(loadAppearancePreferences(storage)).toEqual(defaultAppearancePreferences);
    expect(storage.getItem("other-screen")).toBe("kept");
    expect(storage.getItem(appearanceStorageKey)).not.toBeNull();
    console.log("TASK5020 restart_values=3 reset_other_screens_changed=0");
  });
});
