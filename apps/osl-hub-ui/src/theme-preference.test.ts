import { describe, expect, it } from "vitest";
import { initializeThemePreference, themeStorageKey } from "./theme-preference";

function memoryStorage(initial: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(initial));
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

describe("initial theme preference", () => {
  it("persists dark for a truly fresh OSL profile", () => {
    const storage = memoryStorage();

    expect(initializeThemePreference(storage)).toBe("dark");
    expect(storage.getItem(themeStorageKey)).toBe("dark");
  });

  it("migrates an existing OSL profile without a theme to system", () => {
    const storage = memoryStorage({ "osl-preview-onboarded": "true" });

    expect(initializeThemePreference(storage)).toBe("system");
    expect(storage.getItem(themeStorageKey)).toBe("system");
  });

  it.each(["light", "dark", "system"] as const)("preserves an explicit %s choice", (theme) => {
    const storage = memoryStorage({ [themeStorageKey]: theme, "osl-hub-sidebar": "[]" });

    expect(initializeThemePreference(storage)).toBe(theme);
    expect(storage.getItem(themeStorageKey)).toBe(theme);
  });

  it("does not mistake unrelated origin storage for existing OSL state", () => {
    const storage = memoryStorage({ "unrelated-product-setting": "present" });

    expect(initializeThemePreference(storage)).toBe("dark");
    expect(storage.getItem(themeStorageKey)).toBe("dark");
  });
});
