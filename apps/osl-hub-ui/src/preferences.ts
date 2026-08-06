import { invoke } from "@tauri-apps/api/core";
import {
  defaultOnboardingPreferences,
  needsRiskAcceptance,
  parseRustOnboardingPreferences,
  parseSetupState,
  toRustOnboardingPreferences,
  type OnboardingPreferences,
  type SendMode,
} from "./state";

const browserSetupKey = "osl-preview-setup";
const browserCoverInsertionKey = "osl-preview-cover-insertion";
const browserCompleteKey = "osl-preview-onboarded";

type FirstRunProtectionPreset = "basic" | "balanced" | "maximum";
type FirstRunOrdinarySendMode = Extract<SendMode, "manual" | "clipboard" | "double">;
type FirstRunSendBehavior =
  | "inherited"
  | {
    mode: FirstRunOrdinarySendMode;
    acknowledgedExperimentalSendRisk?: boolean;
  };

const firstRunProtectionPresetValues: readonly FirstRunProtectionPreset[] = ["basic", "balanced", "maximum"];
const firstRunOrdinarySendModeValues: readonly FirstRunOrdinarySendMode[] = ["manual", "clipboard", "double"];
const firstRunPresetSendDefaults: Record<FirstRunProtectionPreset, FirstRunOrdinarySendMode> = {
  basic: "manual",
  balanced: "manual",
  maximum: "manual",
};

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
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
    coverInsertion: parseRustOnboardingPreferences({
      ...toRustOnboardingPreferences(defaultOnboardingPreferences),
      coverInsertion: localStorage.getItem(browserCoverInsertionKey),
    }).coverInsertion,
    showPlaintextPreview: true,
    windowCaptureEnabled: true,
    forwardSecrecyMode: "keepGroupDelivery",
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
    if (wirePreferences.coverInsertion === null) localStorage.removeItem(browserCoverInsertionKey);
    else localStorage.setItem(browserCoverInsertionKey, wirePreferences.coverInsertion);
    localStorage.setItem(browserCompleteKey, String(preferences.onboardingComplete));
  }
  return parseRustOnboardingPreferences(wirePreferences);
}

export async function saveFirstRunOnboardingPreferences(selection: {
  protectionPreset: FirstRunProtectionPreset;
  sendBehavior: FirstRunSendBehavior;
  windowCaptureEnabled?: boolean;
}): Promise<OnboardingPreferences> {
  if (!firstRunProtectionPresetValues.includes(selection.protectionPreset)) {
    throw new Error("First-run protection preset was refused");
  }

  let sendMode: FirstRunOrdinarySendMode;
  let acknowledged = false;

  if (selection.sendBehavior === "inherited") {
    sendMode = firstRunPresetSendDefaults[selection.protectionPreset];
  } else {
    sendMode = selection.sendBehavior.mode;
    acknowledged = needsRiskAcceptance(sendMode)
      && selection.sendBehavior.acknowledgedExperimentalSendRisk === true;
  }

  if (!firstRunOrdinarySendModeValues.includes(sendMode)) {
    throw new Error("First-run send behavior was refused");
  }

  if (needsRiskAcceptance(sendMode) && !acknowledged) {
    throw new Error("First-run send behavior requires explicit acknowledgement");
  }

  return saveOnboardingPreferences({
    onboardingComplete: true,
    setup: {
      sendMode,
      placementMode: "atomic",
      acceptedRisk: acknowledged,
      acceptedRiskForMode: acknowledged ? sendMode : null,
    },
    coverInsertion: "insert-on-send",
    showPlaintextPreview: true,
    windowCaptureEnabled: selection.windowCaptureEnabled !== false,
    forwardSecrecyMode: "keepGroupDelivery",
  });
}
