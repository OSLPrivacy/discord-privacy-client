import { hasTauriBridge, invoke } from "./dev-preview";
import {
  defaultOnboardingPreferences,
  parseRustOnboardingPreferences,
  parseSetupState,
  toRustOnboardingPreferences,
  type OnboardingPreferences,
} from "./state";

const browserSetupKey = "osl-preview-setup";
const browserCompleteKey = "osl-preview-onboarded";

export function isTauriRuntime(): boolean {
  return hasTauriBridge();
}

export async function loadOnboardingPreferences(): Promise<OnboardingPreferences> {
  if (isTauriRuntime()) {
    const raw = await invoke<unknown>("get_onboarding_preferences");
    return parseRustOnboardingPreferences(raw);
  }

  if (typeof localStorage === "undefined") return structuredClone(defaultOnboardingPreferences);
  return {
    onboardingComplete: localStorage.getItem(browserCompleteKey) === "true",
    setup: parseSetupState(localStorage.getItem(browserSetupKey)),
    showPlaintextPreview: true,
  };
}

export async function saveOnboardingPreferences(preferences: OnboardingPreferences): Promise<OnboardingPreferences> {
  const wirePreferences = toRustOnboardingPreferences(preferences);
  if (isTauriRuntime()) {
    const saved = await invoke<unknown>("save_onboarding_preferences", { preferences: wirePreferences });
    return parseRustOnboardingPreferences(saved);
  }

  if (typeof localStorage !== "undefined") {
    localStorage.setItem(browserSetupKey, JSON.stringify(preferences.setup));
    localStorage.setItem(browserCompleteKey, String(preferences.onboardingComplete));
  }
  return parseRustOnboardingPreferences(wirePreferences);
}
