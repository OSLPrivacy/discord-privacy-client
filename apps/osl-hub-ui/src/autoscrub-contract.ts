import type { ServiceId } from "./services";

export const AUTOSCRUB_UNATTENDED_RUN_COMMAND = "autoscrub_unattended_run" as const;

export interface AutoscrubUnattendedContract {
  readonly production: boolean;
  readonly unattendedAllowed: boolean;
  readonly reviewRequiredEveryBatch: boolean;
  readonly externalSecurityReviewPassed: boolean;
}

export type AutoscrubUnattendedRefusalReason =
  | "invalid-contract"
  | "not-production"
  | "unattended-disabled"
  | "review-required"
  | "external-review-required"
  | "native-refused";

export type AutoscrubUnattendedGateResult =
  | { readonly state: "refused"; readonly reason: AutoscrubUnattendedRefusalReason }
  | { readonly state: "ready"; readonly command: typeof AUTOSCRUB_UNATTENDED_RUN_COMMAND };

export interface AutoscrubUnattendedRunStarted {
  readonly state: "started";
  readonly command: typeof AUTOSCRUB_UNATTENDED_RUN_COMMAND;
  readonly runId: string;
  readonly working: number;
  readonly totalRuns: number;
}

export type AutoscrubUnattendedRunResult =
  | { readonly state: "refused"; readonly reason: AutoscrubUnattendedRefusalReason }
  | AutoscrubUnattendedRunStarted;


export type AutoScrubRunPhase = "reviewRequired" | "running" | "stopping" | "blocked" | "complete" | "failed";
export type AutoScrubQuitGuardState = "notRequested" | "confirming" | "checking" | "estimated" | "stopped" | "unknown" | "refused";
export type AutoScrubRunPhase = "reviewRequired" | "running" | "stopping" | "blocked" | "skipped" | "complete" | "failed";
export type AutoScrubQuitGuardState = "notRequested" | "checking" | "estimated" | "stopped" | "unknown" | "refused";
export type AutoScrubDisplayTone = "neutral" | "working" | "warning" | "blocked";
export type AutoScrubRunActionKind = "openAccount" | "tryAgainAfterSignIn" | "skipThisAccount" | "stopAllScanning";

export interface AutoScrubRunAction {
  readonly action: AutoScrubRunActionKind;
  readonly label: "Open account" | "Try again after sign-in" | "Skip this account" | "Stop all scanning";
}

export interface AutoScrubRunSummary {
  readonly runId: string;
  readonly serviceId: ServiceId;
  readonly accountId: string;
  readonly phase: AutoScrubRunPhase;
  readonly reviewedItemCount: number;
  readonly remainingItemCount: number;
  readonly paceMilliseconds: number;
  readonly stopRequested: boolean;
  readonly mutationAllowed: false;
  readonly lastOutcome: "none" | "prepared" | "confirmed" | "held" | "unknown";
  readonly accountActions: readonly AutoScrubRunAction[];
}

export interface AutoScrubQuitGuardEstimate {
  readonly state: AutoScrubQuitGuardState;
  readonly honestRemainingSecondsEstimate: number | null;
  readonly reason: string;
}

export interface AutoScrubStopConfirmationState {
  readonly required: boolean;
  readonly keepScanningLabel: "Keep scanning";
  readonly stopNowLabel: "Stop now";
}

export interface AutoScrubFleetStatus {
  readonly contract: "autoscrubRunFleet.v1";
  readonly openRunCount: number;
  readonly globalStopRequested: boolean;
  readonly stopConfirmation: AutoScrubStopConfirmationState;
  readonly unattendedExecutionAllowed: false;
  readonly quitGuard: AutoScrubQuitGuardEstimate;
  readonly fleetActions: readonly AutoScrubRunAction[];
  readonly runs: readonly AutoScrubRunSummary[];
}

export interface AutoScrubStatusProjection {
  label: string;
  detail: string;
  tone: AutoScrubDisplayTone;
  stopAvailable: boolean;
}

export interface AutoScrubReviewedRunRequest {
  readonly runId: string;
  readonly serviceId: ServiceId;
  readonly accountId: string;
  readonly reviewToken: string;
  readonly planDigest: string;
  readonly reviewedItemCount: number;
  readonly paceMilliseconds: number;
  readonly consent: "reviewedBatchOnly";
  readonly riskAgreement: true;
}

export type DeepReadonly<T> = T extends (...args: never[]) => unknown
  ? T
  : T extends readonly (infer U)[]
    ? readonly DeepReadonly<U>[]
    : T extends object
      ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
      : T;

export function deepFreeze<T>(value: T): DeepReadonly<T> {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) {
    return value as DeepReadonly<T>;
  }
  for (const key of Reflect.ownKeys(value)) {
    const child = (value as Record<PropertyKey, unknown>)[key];
    if (typeof child === "object" && child !== null) {
      deepFreeze(child);
    }
  }
  return Object.freeze(value) as DeepReadonly<T>;
}

export const AUTOSCRUB_UNATTENDED_BASE_CONTRACT = deepFreeze({
  production: true,
  unattendedAllowed: false,
  reviewRequiredEveryBatch: true,
  externalSecurityReviewPassed: false,
} satisfies AutoscrubUnattendedContract);

export const AUTOSCRUB_BASE_CONTRACT: AutoscrubUnattendedContract = AUTOSCRUB_UNATTENDED_BASE_CONTRACT;

export function createAutoscrubUnattendedContract(
  overrides: Partial<AutoscrubUnattendedContract> = {},
): AutoscrubUnattendedContract {
  return deepFreeze({ ...AUTOSCRUB_UNATTENDED_BASE_CONTRACT, ...overrides });
}

const serviceIds: readonly ServiceId[] = [
  "discord", "telegram", "email", "signal", "whatsapp", "messenger",
];
const phases: readonly AutoScrubRunPhase[] = ["reviewRequired", "running", "stopping", "blocked", "complete", "failed"];
const quitGuardStates: readonly AutoScrubQuitGuardState[] = ["notRequested", "confirming", "checking", "estimated", "stopped", "unknown", "refused"];
const phases: readonly AutoScrubRunPhase[] = ["reviewRequired", "running", "stopping", "blocked", "skipped", "complete", "failed"];
const quitGuardStates: readonly AutoScrubQuitGuardState[] = ["notRequested", "checking", "estimated", "stopped", "unknown", "refused"];
const outcomes: readonly AutoScrubRunSummary["lastOutcome"][] = ["none", "prepared", "confirmed", "held", "unknown"];
const MAX_RETAINED_FLEET_RUNS = 8;
const runActions: Readonly<Record<AutoScrubRunActionKind, AutoScrubRunAction["label"]>> = {
  openAccount: "Open account",
  tryAgainAfterSignIn: "Try again after sign-in",
  skipThisAccount: "Skip this account",
  stopAllScanning: "Stop all scanning",
};


function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

export function parseAutoscrubUnattendedContract(raw: unknown): AutoscrubUnattendedContract | null {
  if (!exactRecord(raw, ["production", "unattendedAllowed", "reviewRequiredEveryBatch", "externalSecurityReviewPassed"])) {
    return null;
  }
  if (typeof raw.production !== "boolean"
    || typeof raw.unattendedAllowed !== "boolean"
    || typeof raw.reviewRequiredEveryBatch !== "boolean"
    || typeof raw.externalSecurityReviewPassed !== "boolean") {
    return null;
  }
  return deepFreeze({
    production: raw.production,
    unattendedAllowed: raw.unattendedAllowed,
    reviewRequiredEveryBatch: raw.reviewRequiredEveryBatch,
    externalSecurityReviewPassed: raw.externalSecurityReviewPassed,
  });
}

export function autoscrubUnattendedContractGate(raw: unknown): AutoscrubUnattendedGateResult {
  const contract = parseAutoscrubUnattendedContract(raw);
  if (!contract) return { state: "refused", reason: "invalid-contract" };
  if (!contract.production) return { state: "refused", reason: "not-production" };
  if (!contract.unattendedAllowed) return { state: "refused", reason: "unattended-disabled" };
  if (contract.reviewRequiredEveryBatch) return { state: "refused", reason: "review-required" };
  if (!contract.externalSecurityReviewPassed) return { state: "refused", reason: "external-review-required" };
  return { state: "ready", command: AUTOSCRUB_UNATTENDED_RUN_COMMAND };
}

export function parseAutoscrubUnattendedRunStarted(raw: unknown): AutoscrubUnattendedRunStarted | null {
  if (!exactRecord(raw, ["runId", "working", "totalRuns"])) return null;
  if (typeof raw.runId !== "string"
    || !/^[A-Za-z0-9._:-]{8,180}$/u.test(raw.runId)
    || !Number.isSafeInteger(raw.working)
    || Number(raw.working) < 0
    || Number(raw.working) > 2
    || !Number.isSafeInteger(raw.totalRuns)
    || Number(raw.totalRuns) < Number(raw.working)
    || Number(raw.totalRuns) > 2) {
    return null;
  }
  return deepFreeze({
    state: "started",
    command: AUTOSCRUB_UNATTENDED_RUN_COMMAND,
    runId: raw.runId,
    working: Number(raw.working),
    totalRuns: Number(raw.totalRuns),
  });
}


function boundedText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/u.test(value);
}

function boundedCount(value: unknown, max: number): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= max;
}

function opaqueIdentifier(value: unknown, max: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.length <= max
    && /^[A-Za-z0-9_-]+$/u.test(value);
}

function sha256Hex(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{64}$/u.test(value);
}

function parseRun(raw: unknown): AutoScrubRunSummary {
  if (!exactRecord(raw, ["runId", "serviceId", "accountId", "phase", "reviewedItemCount", "remainingItemCount", "stopRequested", "mutationAllowed", "lastOutcome", "accountActions"])) {
  if (!exactRecord(raw, ["runId", "serviceId", "phase", "reviewedItemCount", "remainingItemCount", "paceMilliseconds", "stopRequested", "mutationAllowed", "lastOutcome"])) {
  if (!exactRecord(raw, ["runId", "serviceId", "accountId", "phase", "reviewedItemCount", "remainingItemCount", "stopRequested", "mutationAllowed", "lastOutcome"])) {
    throw new Error("invalid AutoScrub run");
  }
  if (!boundedText(raw.runId, 80)
    || !serviceIds.includes(raw.serviceId as ServiceId)
    || !opaqueIdentifier(raw.accountId, 64)
    || !phases.includes(raw.phase as AutoScrubRunPhase)
    || !boundedCount(raw.reviewedItemCount, 10_000)
    || !boundedCount(raw.remainingItemCount, 10_000)
    || raw.remainingItemCount > raw.reviewedItemCount
    || !boundedCount(raw.paceMilliseconds, 86_400_000)
    || raw.paceMilliseconds < 500
    || typeof raw.stopRequested !== "boolean"
    || raw.mutationAllowed !== false
    || !outcomes.includes(raw.lastOutcome as AutoScrubRunSummary["lastOutcome"])
    || !Array.isArray(raw.accountActions)
    || raw.accountActions.length > 3) {
    throw new Error("invalid AutoScrub run");
  }
  const accountActions = raw.accountActions.map(parseAction);
  return deepFreeze({
    runId: raw.runId,
    serviceId: raw.serviceId,
    accountId: raw.accountId,
    phase: raw.phase,
    reviewedItemCount: raw.reviewedItemCount,
    remainingItemCount: raw.remainingItemCount,
    paceMilliseconds: raw.paceMilliseconds,
    stopRequested: raw.stopRequested,
    mutationAllowed: false,
    lastOutcome: raw.lastOutcome,
    accountActions,
  } as AutoScrubRunSummary);
}

function parseAction(raw: unknown): AutoScrubRunAction {
  if (!exactRecord(raw, ["action", "label"])
    || typeof raw.action !== "string"
    || !(raw.action in runActions)
    || raw.label !== runActions[raw.action as AutoScrubRunActionKind]) {
    throw new Error("invalid AutoScrub run action");
  }
  return deepFreeze({
    action: raw.action,
    label: raw.label,
  } as AutoScrubRunAction);
}

function parseQuitGuard(raw: unknown): AutoScrubQuitGuardEstimate {
  if (!exactRecord(raw, ["state", "honestRemainingSecondsEstimate", "reason"])) {
    throw new Error("invalid AutoScrub quit guard");
  }
  if (!quitGuardStates.includes(raw.state as AutoScrubQuitGuardState)
    || !(raw.honestRemainingSecondsEstimate === null || boundedCount(raw.honestRemainingSecondsEstimate, 86_400))
    || !boundedText(raw.reason, 160)) {
    throw new Error("invalid AutoScrub quit guard");
  }
  return deepFreeze({
    state: raw.state,
    honestRemainingSecondsEstimate: raw.honestRemainingSecondsEstimate,
    reason: raw.reason,
  } as AutoScrubQuitGuardEstimate);
}

function parseStopConfirmation(raw: unknown): AutoScrubStopConfirmationState {
  if (!exactRecord(raw, ["required", "keepScanningLabel", "stopNowLabel"])
    || typeof raw.required !== "boolean"
    || raw.keepScanningLabel !== "Keep scanning"
    || raw.stopNowLabel !== "Stop now") {
    throw new Error("invalid AutoScrub stop confirmation");
  }
  return deepFreeze({
    required: raw.required,
    keepScanningLabel: "Keep scanning",
    stopNowLabel: "Stop now",
  } as AutoScrubStopConfirmationState);
}

export function parseAutoScrubFleetStatus(raw: unknown): AutoScrubFleetStatus {
  if (!exactRecord(raw, ["contract", "openRunCount", "globalStopRequested", "stopConfirmation", "unattendedExecutionAllowed", "quitGuard", "runs"])
  if (!exactRecord(raw, ["contract", "openRunCount", "globalStopRequested", "unattendedExecutionAllowed", "quitGuard", "fleetActions", "runs"])
    || raw.contract !== "autoscrubRunFleet.v1"
    || !boundedCount(raw.openRunCount, 2)
    || typeof raw.globalStopRequested !== "boolean"
    || raw.unattendedExecutionAllowed !== false
    || !Array.isArray(raw.runs)
    || raw.runs.length < raw.openRunCount
    || raw.runs.length > MAX_RETAINED_FLEET_RUNS
    || !Array.isArray(raw.fleetActions)
    || raw.fleetActions.length > 1) {
    throw new Error("invalid AutoScrub fleet status");
  }
  const quitGuard = parseQuitGuard(raw.quitGuard);
  const stopConfirmation = parseStopConfirmation(raw.stopConfirmation);
  const fleetActions = raw.fleetActions.map(parseAction);
  const runs = raw.runs.map(parseRun);
  if (new Set(runs.map((run) => run.runId)).size !== runs.length) {
    throw new Error("invalid AutoScrub fleet status");
  }
  if (runs.filter((run) => ["reviewRequired", "running", "stopping", "blocked"].includes(run.phase)).length !== raw.openRunCount) {
    throw new Error("invalid AutoScrub fleet status");
  }
  return deepFreeze({
    contract: "autoscrubRunFleet.v1",
    openRunCount: raw.openRunCount,
    globalStopRequested: raw.globalStopRequested,
    stopConfirmation,
    unattendedExecutionAllowed: false,
    quitGuard,
    fleetActions,
    runs,
  } as AutoScrubFleetStatus);
}

export function parseAutoScrubReviewedRunRequest(raw: unknown): AutoScrubReviewedRunRequest {
  if (!exactRecord(raw, ["serviceId", "accountId", "reviewToken", "planDigest", "reviewedItemCount", "paceMilliseconds", "consent"])
  if (!exactRecord(raw, ["runId", "serviceId", "accountId", "reviewToken", "planDigest", "reviewedItemCount", "consent", "riskAgreement"])
    || !opaqueIdentifier(raw.runId, 64)
    || !serviceIds.includes(raw.serviceId as ServiceId)
    || !opaqueIdentifier(raw.accountId, 64)
    || !opaqueIdentifier(raw.reviewToken, 96)
    || !sha256Hex(raw.planDigest)
    || !boundedCount(raw.reviewedItemCount, 500)
    || raw.reviewedItemCount < 1
    || !boundedCount(raw.paceMilliseconds, 86_400_000)
    || raw.paceMilliseconds < 500
    || raw.consent !== "reviewedBatchOnly") {
    || raw.consent !== "reviewedBatchOnly"
    || raw.riskAgreement !== true) {
    throw new Error("invalid AutoScrub reviewed run request");
  }
  return deepFreeze({
    runId: raw.runId,
    serviceId: raw.serviceId,
    accountId: raw.accountId,
    reviewToken: raw.reviewToken,
    planDigest: raw.planDigest,
    reviewedItemCount: raw.reviewedItemCount,
    paceMilliseconds: raw.paceMilliseconds,
    consent: "reviewedBatchOnly",
    riskAgreement: true,
  } as AutoScrubReviewedRunRequest);
}

function formatEstimate(seconds: number | null): string {
  if (seconds === null) return "stop time unknown";
  if (seconds < 60) return "under a minute";
  const minutes = Math.ceil(seconds / 60);
  return `about ${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
}

export function projectAutoScrubFleetStatus(status: AutoScrubFleetStatus | null): AutoScrubStatusProjection {
  if (!status) {
    return {
      label: "Unavailable in this build",
      detail: "Nothing runs until OSL can show a confirmed local list you approve.",
      tone: "neutral",
      stopAvailable: false,
    };
  }
  if (status.quitGuard.state === "refused") {
    return {
      label: "Stopped",
      detail: "OSL refused to continue because stop authority could not be proven.",
      tone: "blocked",
      stopAvailable: false,
    };
  }
  if (status.stopConfirmation.required || status.quitGuard.state === "confirming") {
    return {
      label: "Confirm stop",
      detail: "Choose Keep scanning or Stop now.",
      tone: "warning",
      stopAvailable: false,
    };
  }
  if (status.globalStopRequested || status.runs.some((run) => run.stopRequested) || status.quitGuard.state === "estimated" || status.quitGuard.state === "checking") {
    return {
      label: "Stop requested",
      detail: `Finishing checked items; ${formatEstimate(status.quitGuard.honestRemainingSecondsEstimate)}.`,
      tone: status.quitGuard.state === "unknown" ? "warning" : "working",
      stopAvailable: false,
    };
  }
  if (status.runs.some((run) => run.phase === "blocked" || run.phase === "failed")) {
    return {
      label: "Needs review",
      detail: "A cleanup run needs your review before anything else changes.",
      tone: "warning",
      stopAvailable: false,
    };
  }
  if (status.runs.some((run) => run.phase === "running")) {
    return {
      label: `${status.openRunCount} open ${status.openRunCount === 1 ? "run" : "runs"}`,
      detail: "Only selected items can be prepared; every batch still needs confirmation.",
      tone: "working",
      stopAvailable: true,
    };
  }
  return {
    label: "Ready to review",
    detail: "AutoScrub has no active local run.",
    tone: "neutral",
    stopAvailable: false,
  };
}
