export type SendMode = "manual" | "clipboard" | "double" | "single";
export type PlacementMode = "atomic" | "compatibility";
export type ProtectionMode = "native" | "protected";
export type ComposerPhase = "idle" | "prepared" | "placed";
export type ComposerAction = "prepare-manual" | "prepare-clipboard" | "place" | "send";
export type FirstRunOnboardingStep =
  | "welcome"
  | "choose-protection"
  | "choose-apps"
  | "choose-send"
  | "review-defaults"
  | "secure-recovery";

export interface FirstRunOnboardingStepDefinition {
  step: FirstRunOnboardingStep;
  title: string;
  requiredGuarantees: readonly string[];
}

export const firstRunOnboardingStepOrder: readonly FirstRunOnboardingStep[] = [
  "welcome",
  "choose-protection",
  "choose-apps",
  "choose-send",
  "review-defaults",
  "secure-recovery",
];

export const firstRunOnboardingStepContract: readonly FirstRunOnboardingStepDefinition[] = [
  {
    step: "welcome",
    title: "Welcome",
    requiredGuarantees: [
      "protects-existing-accounts",
      "offers-private-osl-communication",
    ],
  },
  {
    step: "choose-protection",
    title: "Choose protection",
    requiredGuarantees: [
      "basic-balanced-maximum-only",
      "balanced-recommended",
    ],
  },
  {
    step: "choose-apps",
    title: "Choose apps",
    requiredGuarantees: [
      "detect-supported-windows-clients",
      "native-client-sign-in-only",
      "skippable",
    ],
  },
  {
    step: "choose-send",
    title: "Choose how Send works",
    requiredGuarantees: [
      "manual-recommended",
      "ordinary-modes-manual-clipboard-double-enter",
      "no-silent-send",
      "no-auto-retry",
    ],
  },
  {
    step: "review-defaults",
    title: "Review defaults",
    requiredGuarantees: [
      "show-warn-sanitize-retain-delete-effects",
      "destructive-automation-off",
    ],
  },
  {
    step: "secure-recovery",
    title: "Secure recovery",
    requiredGuarantees: [
      "establish-recovery-first",
      "mullvad-or-android-optional",
    ],
  },
];

export interface SetupState {
  sendMode: SendMode;
  placementMode: PlacementMode;
  acceptedRisk: boolean;
  acceptedRiskForMode: SendMode | null;
}

export interface OnboardingPreferences {
  onboardingComplete: boolean;
  setup: SetupState;
  showPlaintextPreview: boolean;
  windowCaptureEnabled: boolean;
}

export interface RustOnboardingPreferences {
  onboardingComplete: boolean;
  sendMode: SendMode;
  placementMode: PlacementMode;
  showPlaintextPreview: boolean;
  windowCaptureEnabled: boolean;
  acknowledgeExperimentalSendRisk: boolean;
}

export const defaultSetup: SetupState = {
  sendMode: "manual",
  placementMode: "atomic",
  acceptedRisk: false,
  acceptedRiskForMode: null,
};

export const defaultOnboardingPreferences: OnboardingPreferences = {
  onboardingComplete: false,
  setup: { ...defaultSetup },
  showPlaintextPreview: true,
  windowCaptureEnabled: true,
};

const sendModeValues: readonly SendMode[] = ["manual", "clipboard", "double", "single"];
const placementModeValues: readonly PlacementMode[] = ["atomic", "compatibility"];
const firstRunOnboardingStepValues: readonly FirstRunOnboardingStep[] = firstRunOnboardingStepOrder;

export function parseFirstRunOnboardingStep(raw: unknown): FirstRunOnboardingStep {
  return firstRunOnboardingStepValues.includes(raw as FirstRunOnboardingStep)
    ? raw as FirstRunOnboardingStep
    : firstRunOnboardingStepOrder[0];
}

export function parseSetupState(raw: string | null): SetupState {
  if (!raw) return { ...defaultSetup };
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    const sendMode = sendModeValues.includes(value.sendMode as SendMode) ? value.sendMode as SendMode : defaultSetup.sendMode;
    const acceptedRiskForMode = sendModeValues.includes(value.acceptedRiskForMode as SendMode) ? value.acceptedRiskForMode as SendMode : null;
    return {
      sendMode,
      placementMode: placementModeValues.includes(value.placementMode as PlacementMode) ? value.placementMode as PlacementMode : defaultSetup.placementMode,
      acceptedRisk: value.acceptedRisk === true && acceptedRiskForMode === sendMode,
      acceptedRiskForMode,
    };
  } catch {
    return { ...defaultSetup };
  }
}

export function parseRustOnboardingPreferences(raw: unknown): OnboardingPreferences {
  if (!isRecord(raw)) return cloneDefaultPreferences();
  const allowedKeys = new Set([
    "onboardingComplete",
    "sendMode",
    "placementMode",
    "showPlaintextPreview",
    "windowCaptureEnabled",
    "acknowledgeExperimentalSendRisk",
  ]);
  if (Object.keys(raw).length !== allowedKeys.size || Object.keys(raw).some((key) => !allowedKeys.has(key))) {
    return cloneDefaultPreferences();
  }
  const sendMode = sendModeValues.includes(raw.sendMode as SendMode) ? raw.sendMode as SendMode : null;
  const placementMode = placementModeValues.includes(raw.placementMode as PlacementMode) ? raw.placementMode as PlacementMode : null;
  if (
    sendMode === null
    || placementMode === null
    || typeof raw.onboardingComplete !== "boolean"
    || typeof raw.showPlaintextPreview !== "boolean"
    || typeof raw.windowCaptureEnabled !== "boolean"
    || typeof raw.acknowledgeExperimentalSendRisk !== "boolean"
  ) return cloneDefaultPreferences();

  const riskAccepted = needsRiskAcceptance(sendMode) && raw.acknowledgeExperimentalSendRisk;
  const onboardingComplete = raw.onboardingComplete
    && (!needsRiskAcceptance(sendMode) || riskAccepted);
  return {
    onboardingComplete,
    setup: {
      sendMode,
      placementMode,
      acceptedRisk: riskAccepted,
      acceptedRiskForMode: riskAccepted ? sendMode : null,
    },
    showPlaintextPreview: raw.showPlaintextPreview,
    windowCaptureEnabled: raw.windowCaptureEnabled,
  };
}

export function toRustOnboardingPreferences(preferences: OnboardingPreferences): RustOnboardingPreferences {
  const parsedSetup = parseSetupState(JSON.stringify(preferences.setup));
  const acknowledged = needsRiskAcceptance(parsedSetup.sendMode)
    && parsedSetup.acceptedRisk
    && parsedSetup.acceptedRiskForMode === parsedSetup.sendMode;
  return {
    onboardingComplete: preferences.onboardingComplete === true
      && (!needsRiskAcceptance(parsedSetup.sendMode) || acknowledged),
    sendMode: parsedSetup.sendMode,
    placementMode: parsedSetup.placementMode,
    showPlaintextPreview: preferences.showPlaintextPreview === true,
    windowCaptureEnabled: preferences.windowCaptureEnabled === true,
    acknowledgeExperimentalSendRisk: acknowledged,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function cloneDefaultPreferences(): OnboardingPreferences {
  return {
    onboardingComplete: false,
    setup: { ...defaultSetup },
    showPlaintextPreview: true,
    windowCaptureEnabled: true,
  };
}

export function needsRiskAcceptance(mode: SendMode): boolean {
  return mode === "double" || mode === "single";
}

export function canCompleteSetup(state: SetupState): boolean {
  return !needsRiskAcceptance(state.sendMode) || (state.acceptedRisk && state.acceptedRiskForMode === state.sendMode);
}

export function makeCapsulePreview(message: string): string {
  if (!message.trim()) return "Encrypted message preview will appear here";
  const bytes = new TextEncoder().encode(message.trim()).length;
  const padded = Math.max(128, Math.ceil(bytes / 64) * 64);
  return `osl://v1/demo.${padded.toString(36)}.••••••••••••••••`;
}

export function formatSendMode(mode: SendMode): string {
  const labels: Record<SendMode, string> = {
    manual: "Manual",
    clipboard: "Clipboard",
    double: "Double Enter",
    single: "Single Enter",
  };
  return labels[mode];
}

export function advanceSendMode(mode: SendMode, phase: ComposerPhase): { phase: ComposerPhase; action: ComposerAction } {
  if (mode === "manual") return { phase: "prepared", action: "prepare-manual" };
  if (mode === "clipboard") return { phase: "prepared", action: "prepare-clipboard" };
  if (mode === "double" && phase !== "placed") return { phase: "placed", action: "place" };
  return { phase: "idle", action: "send" };
}
