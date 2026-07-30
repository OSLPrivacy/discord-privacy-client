import type { ServiceId } from "./services";

export type AutoScrubRunPhase = "reviewRequired" | "running" | "stopping" | "blocked" | "complete" | "failed";
export type AutoScrubQuitGuardState = "notRequested" | "checking" | "estimated" | "stopped" | "unknown" | "refused";
export type AutoScrubDisplayTone = "neutral" | "working" | "warning" | "blocked";

export interface AutoScrubRunSummary {
  runId: string;
  serviceId: ServiceId;
  phase: AutoScrubRunPhase;
  reviewedItemCount: number;
  remainingItemCount: number;
  stopRequested: boolean;
  mutationAllowed: false;
  lastOutcome: "none" | "prepared" | "confirmed" | "held" | "unknown";
}

export interface AutoScrubQuitGuardEstimate {
  state: AutoScrubQuitGuardState;
  honestRemainingSecondsEstimate: number | null;
  reason: string;
}

export interface AutoScrubFleetStatus {
  contract: "autoscrubRunFleet.v1";
  openRunCount: number;
  globalStopRequested: boolean;
  unattendedExecutionAllowed: false;
  quitGuard: AutoScrubQuitGuardEstimate;
  runs: AutoScrubRunSummary[];
}

export interface AutoScrubStatusProjection {
  label: string;
  detail: string;
  tone: AutoScrubDisplayTone;
  stopAvailable: boolean;
}

const serviceIds: readonly ServiceId[] = [
  "discord", "telegram", "instagram", "snapchat", "email", "x", "slack", "linkedin", "teams", "messenger", "signal", "whatsapp",
];
const phases: readonly AutoScrubRunPhase[] = ["reviewRequired", "running", "stopping", "blocked", "complete", "failed"];
const quitGuardStates: readonly AutoScrubQuitGuardState[] = ["notRequested", "checking", "estimated", "stopped", "unknown", "refused"];
const outcomes: readonly AutoScrubRunSummary["lastOutcome"][] = ["none", "prepared", "confirmed", "held", "unknown"];

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function boundedText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/u.test(value);
}

function boundedCount(value: unknown, max: number): value is number {
  return Number.isInteger(value) && value >= 0 && value <= max;
}

function parseRun(raw: unknown): AutoScrubRunSummary {
  if (!exactRecord(raw, ["runId", "serviceId", "phase", "reviewedItemCount", "remainingItemCount", "stopRequested", "mutationAllowed", "lastOutcome"])) {
    throw new Error("invalid AutoScrub run");
  }
  if (!boundedText(raw.runId, 80)
    || !serviceIds.includes(raw.serviceId as ServiceId)
    || !phases.includes(raw.phase as AutoScrubRunPhase)
    || !boundedCount(raw.reviewedItemCount, 10_000)
    || !boundedCount(raw.remainingItemCount, 10_000)
    || typeof raw.stopRequested !== "boolean"
    || raw.mutationAllowed !== false
    || !outcomes.includes(raw.lastOutcome as AutoScrubRunSummary["lastOutcome"])) {
    throw new Error("invalid AutoScrub run");
  }
  return raw as unknown as AutoScrubRunSummary;
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
  return raw as unknown as AutoScrubQuitGuardEstimate;
}

export function parseAutoScrubFleetStatus(raw: unknown): AutoScrubFleetStatus {
  if (!exactRecord(raw, ["contract", "openRunCount", "globalStopRequested", "unattendedExecutionAllowed", "quitGuard", "runs"])
    || raw.contract !== "autoscrubRunFleet.v1"
    || !boundedCount(raw.openRunCount, 2)
    || typeof raw.globalStopRequested !== "boolean"
    || raw.unattendedExecutionAllowed !== false
    || !Array.isArray(raw.runs)
    || raw.runs.length !== raw.openRunCount) {
    throw new Error("invalid AutoScrub fleet status");
  }
  const quitGuard = parseQuitGuard(raw.quitGuard);
  const runs = raw.runs.map(parseRun);
  if (new Set(runs.map((run) => run.runId)).size !== runs.length) {
    throw new Error("invalid AutoScrub fleet status");
  }
  return { ...raw, quitGuard, runs } as AutoScrubFleetStatus;
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
      detail: "Nothing runs until OSL can show a reviewed local list.",
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
      detail: "Only reviewed items can be prepared; every batch still needs confirmation.",
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
