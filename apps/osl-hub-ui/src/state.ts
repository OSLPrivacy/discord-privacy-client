export const oslPrimaryDestinationValues = [
  "home",
  "inbox",
  "people",
  "privacy",
  "activity",
  "connections",
] as const;

export type OslPrimaryDestination = typeof oslPrimaryDestinationValues[number];

export interface OslPrimaryDestinationDefinition {
  id: OslPrimaryDestination;
  label: string;
  userQuestion: string;
  mainContent: string;
  primaryAction: string;
}

export const oslPrimaryDestinations = [
  {
    id: "home",
    label: "Home",
    userQuestion: "Am I protected, and what needs attention?",
    mainContent: "Overall state, one recommended action, connected-service health, and recent protection.",
    primaryAction: "Fix the most important issue",
  },
  {
    id: "inbox",
    label: "Inbox",
    userQuestion: "Where are my conversations?",
    mainContent: "Conversations across OSL Chat, OSL Circles, OSL Mail, and supported connected-account views.",
    primaryAction: "Start a private conversation",
  },
  {
    id: "people",
    label: "People",
    userQuestion: "Who do I trust and where do I know them?",
    mainContent: "Verified OSL contacts, platform identities, groups, audiences, and approval policy.",
    primaryAction: "Add or verify a person",
  },
  {
    id: "privacy",
    label: "Privacy",
    userQuestion: "What will OSL do for me?",
    mainContent: "Preset, global policy, platform exceptions, cleanup, and solo privacy tools.",
    primaryAction: "Review or change protection",
  },
  {
    id: "activity",
    label: "Activity",
    userQuestion: "What did OSL actually do?",
    mainContent: "Warnings, scheduled jobs, deletion verification, connection failures, and recent outcomes.",
    primaryAction: "Review an item needing attention",
  },
  {
    id: "connections",
    label: "Connections",
    userQuestion: "Which accounts and devices are connected?",
    mainContent: "Platform accounts, OSL services, Mullvad, and Android Workspace.",
    primaryAction: "Connect a service",
  },
] as const satisfies readonly OslPrimaryDestinationDefinition[];

export const oslSettingsDestination = "settings" as const;

export function isOslPrimaryDestination(value: unknown): value is OslPrimaryDestination {
  return typeof value === "string" && oslPrimaryDestinationValues.includes(value as OslPrimaryDestination);
}

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

export type HonestSendOutcome = "sent" | "not-sent" | "unknown";
export type HonestSendAction = "prepare-manual" | "prepare-clipboard" | "place" | "send";
export type HonestSendRefusalReason =
  | "missing-consent"
  | "missing-binding"
  | "missing-authority"
  | "missing-distinct-user-gesture";

export interface HonestSendGateInput {
  mode: SendMode;
  phase: ComposerPhase;
  consent: boolean;
  binding: boolean;
  authority: boolean;
  distinctUserGesture: boolean;
}

export type HonestSendGateDecision =
  | {
    allowed: true;
    action: HonestSendAction;
    autoRetry: false;
  }
  | {
    allowed: false;
    action: "refuse";
    reason: HonestSendRefusalReason;
    outcome: "not-sent";
    autoRetry: false;
  };

export interface HonestSendOutcomeReport {
  outcome: HonestSendOutcome;
  autoRetry: false;
  preserveDraft: boolean;
}

export interface HonestSendModeRule {
  ordinary: boolean;
  firstAction: HonestSendAction;
  finalAction: HonestSendAction | null;
  requiresDistinctFinalGesture: boolean;
}

export interface HonestSendContractSpec {
  outcomes: readonly HonestSendOutcome[];
  modes: Record<SendMode, HonestSendModeRule>;
  gate(input: HonestSendGateInput): HonestSendGateDecision;
  reportOutcome(value: unknown): HonestSendOutcomeReport;
}

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

const honestSendOutcomes: readonly HonestSendOutcome[] = ["sent", "not-sent", "unknown"];

function refuseHonestSend(reason: HonestSendRefusalReason): HonestSendGateDecision {
  return {
    allowed: false,
    action: "refuse",
    reason,
    outcome: "not-sent",
    autoRetry: false,
  };
}

function actionForHonestSendMode(mode: SendMode, phase: ComposerPhase): HonestSendAction {
  if (mode === "manual") return "prepare-manual";
  if (mode === "clipboard") return "prepare-clipboard";
  if (mode === "double" && phase !== "placed") return "place";
  return "send";
}

export const HonestSendContract: HonestSendContractSpec = {
  outcomes: honestSendOutcomes,
  modes: {
    manual: {
      ordinary: true,
      firstAction: "prepare-manual",
      finalAction: null,
      requiresDistinctFinalGesture: false,
    },
    clipboard: {
      ordinary: true,
      firstAction: "prepare-clipboard",
      finalAction: null,
      requiresDistinctFinalGesture: false,
    },
    double: {
      ordinary: true,
      firstAction: "place",
      finalAction: "send",
      requiresDistinctFinalGesture: true,
    },
    single: {
      ordinary: false,
      firstAction: "send",
      finalAction: "send",
      requiresDistinctFinalGesture: false,
    },
  },
  gate(input: HonestSendGateInput): HonestSendGateDecision {
    if (!input.consent) return refuseHonestSend("missing-consent");
    if (!input.binding) return refuseHonestSend("missing-binding");
    if (!input.authority) return refuseHonestSend("missing-authority");
    if (input.mode === "double" && input.phase === "placed" && !input.distinctUserGesture) {
      return refuseHonestSend("missing-distinct-user-gesture");
    }
    return {
      allowed: true,
      action: actionForHonestSendMode(input.mode, input.phase),
      autoRetry: false,
    };
  },
  reportOutcome(value: unknown): HonestSendOutcomeReport {
    const outcome = honestSendOutcomes.includes(value as HonestSendOutcome)
      ? value as HonestSendOutcome
      : "unknown";
    return {
      outcome,
      autoRetry: false,
      preserveDraft: outcome !== "sent",
    };
  },
};

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
