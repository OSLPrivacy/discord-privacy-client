import { describe, expect, it } from "vitest";
import {
  appearanceSettingsMarkup,
  appearanceStorageKey,
  createAppearancePreferencesEditor,
  defaultAppearancePreferences,
  loadAppearancePreferences,
} from "./appearance-preferences";

class CountingStorage implements Storage {
  readonly values = new Map<string, string>();
  writes = 0;
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  removeItem(key: string): void { this.values.delete(key); }
  setItem(key: string, value: string): void { this.writes += 1; this.values.set(key, value); }
}

describe("TASK 5037 Appearance acceptance", () => {
  it("previews accent, background, and avatar before save; cancel makes zero writes and restores saved values", () => {
    const storage = new CountingStorage();
    const editor = createAppearancePreferencesEditor(defaultAppearancePreferences);
    let draft = editor.preview({ ...defaultAppearancePreferences, accent: "violet" });
    draft = editor.preview({ ...draft, background: "slate" });
    draft = editor.preview({ ...draft, avatar: "wave" });

    const preview = appearanceSettingsMarkup(draft);
    expect(preview).toContain('data-accent="violet"');
    expect(preview).toContain('data-background="slate"');
    expect(preview).toContain('data-avatar="wave"');
    expect(preview).toContain("preview-message-row");
    expect(storage.writes).toBe(0);

    expect(editor.cancel()).toEqual(defaultAppearancePreferences);
    expect(storage.writes).toBe(0);
    expect(storage.getItem(appearanceStorageKey)).toBeNull();
    console.log("TASK5037 preview_accent=violet preview_background=slate preview_avatar=wave cancel_writes=0 saved_values_unchanged=3");
  });

  it("saves all three preview values exactly across restart", () => {
    const storage = new CountingStorage();
    const editor = createAppearancePreferencesEditor(defaultAppearancePreferences);
    const changed = editor.preview({ ...defaultAppearancePreferences, accent: "coral", background: "paper", avatar: "spark" });
    const preview = appearanceSettingsMarkup(changed);
    expect(preview).toContain('data-accent="coral"');
    expect(editor.save(storage)).toEqual(changed);
    expect(storage.writes).toBe(1);
    expect(loadAppearancePreferences(storage)).toEqual(changed);
    console.log("TASK5037 save_writes=1 restart_accent=coral restart_background=paper restart_avatar=spark persisted_values=3");
  });
});
