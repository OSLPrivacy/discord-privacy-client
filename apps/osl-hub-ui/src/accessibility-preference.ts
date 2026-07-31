export const accessibilityStorageKey = "osl-accessibility-v1";

export type TextScale = 100 | 112 | 125 | 150;

export interface AccessibilityPreferences {
  textScale: TextScale;
  highContrast: boolean;
  reduceMotion: boolean;
  largeTargets: boolean;
}

export const defaultAccessibilityPreferences: AccessibilityPreferences = Object.freeze({
  textScale: 100,
  highContrast: false,
  reduceMotion: false,
  largeTargets: false,
});

export function parseAccessibilityPreferences(raw: string | null): AccessibilityPreferences {
  if (!raw) return { ...defaultAccessibilityPreferences };
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    const keys = Object.keys(value).sort().join(",");
    if (keys !== "highContrast,largeTargets,reduceMotion,textScale") return { ...defaultAccessibilityPreferences };
    if (![100, 112, 125, 150].includes(Number(value.textScale))
      || typeof value.highContrast !== "boolean"
      || typeof value.reduceMotion !== "boolean"
      || typeof value.largeTargets !== "boolean") return { ...defaultAccessibilityPreferences };
    return value as unknown as AccessibilityPreferences;
  } catch {
    return { ...defaultAccessibilityPreferences };
  }
}

export function loadAccessibilityPreferences(storage: Pick<Storage, "getItem">): AccessibilityPreferences {
  return parseAccessibilityPreferences(storage.getItem(accessibilityStorageKey));
}

export function saveAccessibilityPreferences(
  storage: Pick<Storage, "setItem">,
  preferences: AccessibilityPreferences,
): void {
  storage.setItem(accessibilityStorageKey, JSON.stringify(preferences));
}

export function applyAccessibilityPreferences(
  root: HTMLElement,
  preferences: AccessibilityPreferences,
): void {
  root.dataset.textScale = String(preferences.textScale);
  root.dataset.highContrast = String(preferences.highContrast);
  root.dataset.reduceMotion = String(preferences.reduceMotion);
  root.dataset.largeTargets = String(preferences.largeTargets);
}
