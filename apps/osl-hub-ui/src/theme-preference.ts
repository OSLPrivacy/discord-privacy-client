export type ThemeChoice = "system" | "dark" | "light";

type ThemeStorage = Pick<Storage, "getItem" | "setItem">;

export const themeStorageKey = "osl-hub-theme";

const existingOslUiStorageKeys = [
  "osl-hub-sidebar",
  "osl-hub-sidebar-hidden",
  "osl-hub-notifications",
  "osl-hub-notification-apps",
  "osl-hub-notification-previews",
  "osl-hub-notification-scope-suggestions",
  "osl-hub-screenshot-protection",
  "osl-hub-scrub-signals-v1",
  "osl-hub-service-guide-v1",
  "osl-home-tile-order-v1",
  "osl-home-tile-hidden-v1",
  "osl-saved-account-mode-v1",
  "osl-saved-native-apps-v1",
  "osl-browser-accounts-ready-v1",
  "osl-browser-detected-services-v1",
  "osl-onboarding-resume-v1",
  "osl-onboarding-branch-v1",
  "osl-experimental-send-consent-v1",
  "osl-preview-setup",
  "osl-preview-onboarded",
] as const;

function explicitTheme(raw: string | null): ThemeChoice | null {
  return raw === "light" || raw === "dark" || raw === "system" ? raw : null;
}

export function initializeThemePreference(storage: ThemeStorage): ThemeChoice {
  const rawTheme = storage.getItem(themeStorageKey);
  const selectedTheme = explicitTheme(rawTheme);
  if (selectedTheme) return selectedTheme;

  const hasExistingOslUiState = rawTheme !== null
    || existingOslUiStorageKeys.some((key) => storage.getItem(key) !== null);
  const initialTheme: ThemeChoice = hasExistingOslUiState ? "system" : "dark";
  storage.setItem(themeStorageKey, initialTheme);
  return initialTheme;
}
