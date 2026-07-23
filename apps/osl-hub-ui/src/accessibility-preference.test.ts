import { describe, expect, it } from "vitest";
import {
  accessibilityStorageKey,
  applyAccessibilityPreferences,
  defaultAccessibilityPreferences,
  loadAccessibilityPreferences,
  parseAccessibilityPreferences,
  saveAccessibilityPreferences,
} from "./accessibility-preference";

describe("accessibility preferences", () => {
  it("fails closed to bounded defaults", () => {
    expect(parseAccessibilityPreferences(null)).toEqual(defaultAccessibilityPreferences);
    expect(parseAccessibilityPreferences("{}")) .toEqual(defaultAccessibilityPreferences);
    expect(parseAccessibilityPreferences(JSON.stringify({
      textScale: 900,
      highContrast: true,
      reduceMotion: true,
      largeTargets: true,
    }))).toEqual(defaultAccessibilityPreferences);
  });

  it("roundtrips an exact preference record", () => {
    const values = new Map<string, string>();
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value); },
    };
    const chosen = { textScale: 125 as const, highContrast: true, reduceMotion: true, largeTargets: true };
    saveAccessibilityPreferences(storage, chosen);
    expect(values.has(accessibilityStorageKey)).toBe(true);
    expect(loadAccessibilityPreferences(storage)).toEqual(chosen);
  });

  it("applies only inert root data attributes", () => {
    const root = { dataset: {} } as unknown as HTMLElement;
    applyAccessibilityPreferences(root, {
      textScale: 112,
      highContrast: true,
      reduceMotion: true,
      largeTargets: false,
    });
    expect(root.dataset).toMatchObject({
      textScale: "112",
      highContrast: "true",
      reduceMotion: "true",
      largeTargets: "false",
    });
  });
});
