export type SendMode = "manual" | "clipboard" | "double" | "single";
export type PlacementMode = "atomic" | "compatibility";
export type ProtectionMode = "native" | "protected";
export type ComposerPhase = "idle" | "prepared" | "placed";
export type ComposerAction = "prepare-manual" | "prepare-clipboard" | "place" | "send";

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
}

export interface RustOnboardingPreferences {
  onboardingComplete: boolean;
  sendMode: SendMode;
  placementMode: PlacementMode;
  showPlaintextPreview: boolean;
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
};

const sendModeValues: readonly SendMode[] = ["manual", "clipboard", "double", "single"];
const placementModeValues: readonly PlacementMode[] = ["atomic", "compatibility"];

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
