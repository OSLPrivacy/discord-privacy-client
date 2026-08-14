import { describe, expect, it } from "vitest";
import { defaultWindowSoundsSettings, loadWindowSoundsSettings, parseWindowSoundsSettings, saveWindowSoundsSettings, windowSoundsSettingsMarkup } from "./window-sounds-settings";

describe("TASK 0780 window and sounds settings", () => {
  it("renders every behaviour choice with its current saved state", () => {
    const settings = { ...defaultWindowSoundsSettings, position: "last" as const, muted: true, quietHours: false };
    const markup = windowSoundsSettingsMarkup(settings);
    for (const label of ["Centre of this screen", "Last place", "Top-left corner", "Remember window place", "Allow window movement", "Show picture in tray", "Play notification sounds", "Mute all OSL sounds", "Quiet hours · 22:00–07:00", "Reset controls"]) expect(markup).toContain(label);
    expect(markup).toContain('value="last" checked');
    expect(markup).toContain('id="window-sound-muted" type="checkbox" checked');
    expect(markup).toContain("Saved · Position: Last place · Sounds: Muted · Quiet hours: Off");
  });

  it("rejects incomplete saved data and restores explicit defaults", () => {
    expect(parseWindowSoundsSettings({ position: "outside", muted: "yes" })).toEqual(defaultWindowSoundsSettings);
  });

  it("persists changes and lets reset restore the named defaults", () => {
    const values = new Map<string, string>();
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value) };
    saveWindowSoundsSettings({ ...defaultWindowSoundsSettings, position: "top-left", sounds: false }, storage);
    expect(loadWindowSoundsSettings(storage)).toEqual({ ...defaultWindowSoundsSettings, position: "top-left", sounds: false });
    saveWindowSoundsSettings(defaultWindowSoundsSettings, storage);
    expect(loadWindowSoundsSettings(storage)).toEqual(defaultWindowSoundsSettings);
  });
});
