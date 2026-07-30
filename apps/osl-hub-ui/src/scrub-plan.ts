import type { ScrubSignalGroup } from "./scrub";

export type ScrubMode = "skip" | "scrub" | "autoscrub";

export interface ScrubSetupTarget {
  id: string;
  serviceId: string;
  accountId: string;
  label: string;
  detail: string;
}

export interface ScrubSetupPlan {
  mode: ScrubMode;
  targetIds: string[];
  signalGroups: ScrubSignalGroup[];
}

export interface ScrubCoverageReceipt {
  targetId: string;
  messagesScanned: number;
  oldestReachableUnixMs: number | null;
  newestReachableUnixMs: number | null;
  providerReportedComplete: boolean;
  gaps: string[];
  textChecked: boolean;
  imagesChecked: boolean;
}

export const SCRUB_TARGET_LIMIT = 32;
export const SCRUB_COVERAGE_GAP_LIMIT = 32;

export function targetId(serviceId: string, accountId: string): string {
  return `${serviceId}:${accountId}`;
}

export function defaultScrubSetupPlan(signalGroups: readonly ScrubSignalGroup[]): ScrubSetupPlan {
  return { mode: "skip", targetIds: [], signalGroups: [...signalGroups] };
}

export function parseScrubSetupPlan(
  raw: string | null,
  availableTargetIds: ReadonlySet<string>,
  allowedSignalGroups: readonly ScrubSignalGroup[],
  proActive: boolean,
): ScrubSetupPlan {
  const fallback = defaultScrubSetupPlan(allowedSignalGroups);
  if (raw === null) return fallback;
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!exactRecord(parsed, ["mode", "targetIds", "signalGroups"])) return fallback;
    const mode = parsed.mode === "skip" || parsed.mode === "scrub" || parsed.mode === "autoscrub"
      ? (parsed.mode === "autoscrub" && !proActive ? "scrub" : parsed.mode)
      : null;
    const allowedGroups = new Set(allowedSignalGroups);
    if (mode === null
      || !Array.isArray(parsed.targetIds)
      || parsed.targetIds.length > SCRUB_TARGET_LIMIT
      || !parsed.targetIds.every((id): id is string => typeof id === "string" && availableTargetIds.has(id))
      || new Set(parsed.targetIds).size !== parsed.targetIds.length
      || !Array.isArray(parsed.signalGroups)
      || parsed.signalGroups.length > allowedGroups.size
      || !parsed.signalGroups.every((group): group is ScrubSignalGroup => typeof group === "string" && allowedGroups.has(group as ScrubSignalGroup))
      || new Set(parsed.signalGroups).size !== parsed.signalGroups.length) return fallback;
    return { mode, targetIds: [...parsed.targetIds], signalGroups: [...parsed.signalGroups] };
  } catch {
    return fallback;
  }
}

export function validateCoverageReceipt(raw: unknown, selectedTargetIds: ReadonlySet<string>): ScrubCoverageReceipt | null {
  if (!exactRecord(raw, [
    "targetId", "messagesScanned", "oldestReachableUnixMs", "newestReachableUnixMs",
    "providerReportedComplete", "gaps", "textChecked", "imagesChecked",
  ])
    || typeof raw.targetId !== "string"
    || !selectedTargetIds.has(raw.targetId)
    || !boundedInteger(raw.messagesScanned)
    || !nullableTimestamp(raw.oldestReachableUnixMs)
    || !nullableTimestamp(raw.newestReachableUnixMs)
    || typeof raw.providerReportedComplete !== "boolean"
    || !Array.isArray(raw.gaps)
    || raw.gaps.length > SCRUB_COVERAGE_GAP_LIMIT
    || !raw.gaps.every((gap): gap is string => typeof gap === "string" && gap.length > 0 && gap.length <= 256)
    || typeof raw.textChecked !== "boolean"
    || typeof raw.imagesChecked !== "boolean") return null;
  if (raw.providerReportedComplete && raw.gaps.length > 0) return null;
  return raw as unknown as ScrubCoverageReceipt;
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function boundedInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function nullableTimestamp(value: unknown): value is number | null {
  return value === null || boundedInteger(value);
}
