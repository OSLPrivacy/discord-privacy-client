import { defaultScrubSignalGroups, type ScrubSignalGroup } from "./scrub";

export type ScrubMode = "skip" | "scrub" | "autoscrub";

export interface ScrubSetupPlan {
  mode: ScrubMode;
  targetIds: string[];
  signalGroups: ScrubSignalGroup[];
}

export const SCRUB_TARGET_LIMIT = 32;

export interface ScrubCoverageReceipt {
  messagesScanned: number;
  oldestReachableAtUnixMs: number | null;
  newestReachableAtUnixMs: number | null;
  providerReportedComplete: boolean;
  gaps: string[];
  textChecked: boolean;
  imagesChecked: boolean;
}

export function validateCoverageReceipt(receipt: ScrubCoverageReceipt): boolean {
  const validTimestamp = (value: number | null): boolean => value === null || (Number.isSafeInteger(value) && value >= 0);
  return Number.isSafeInteger(receipt.messagesScanned)
    && receipt.messagesScanned >= 0
    && validTimestamp(receipt.oldestReachableAtUnixMs)
    && validTimestamp(receipt.newestReachableAtUnixMs)
    && (receipt.oldestReachableAtUnixMs === null
      || receipt.newestReachableAtUnixMs === null
      || receipt.oldestReachableAtUnixMs <= receipt.newestReachableAtUnixMs)
    && typeof receipt.providerReportedComplete === "boolean"
    && Array.isArray(receipt.gaps)
    && receipt.gaps.length <= 32
    && receipt.gaps.every((gap) => typeof gap === "string" && gap.length > 0 && gap.length <= 240)
    && !(receipt.providerReportedComplete && receipt.gaps.length > 0)
    && receipt.textChecked === true
    && receipt.imagesChecked === false;
}

export const defaultScrubSetupPlan: ScrubSetupPlan = {
  mode: "scrub",
  targetIds: [],
  signalGroups: [...defaultScrubSignalGroups],
};

export function targetId(serviceId: string, accountId: string): string {
  return `${serviceId}:${accountId}`;
}

export function parseScrubSetupPlan(
  raw: string | null,
  availableTargetIds: Set<string>,
  allowedSignalGroups: readonly ScrubSignalGroup[],
  proActive: boolean,
): ScrubSetupPlan {
  let value: unknown = null;
  try {
    value = JSON.parse(raw ?? "null") as unknown;
  } catch {
    value = null;
  }
  const parsed = typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Partial<ScrubSetupPlan>
    : {};
  const mode: ScrubMode = parsed.mode === "skip" || parsed.mode === "autoscrub" || parsed.mode === "scrub"
    ? parsed.mode === "autoscrub" && !proActive ? "scrub" : parsed.mode
    : defaultScrubSetupPlan.mode;
  const targetIds = Array.isArray(parsed.targetIds)
    ? [...new Set(parsed.targetIds.filter((id): id is string => typeof id === "string" && availableTargetIds.has(id)))].slice(0, SCRUB_TARGET_LIMIT)
    : [];
  const allowed = new Set(allowedSignalGroups);
  const signalGroups = Array.isArray(parsed.signalGroups)
    ? [...new Set(parsed.signalGroups.filter((group): group is ScrubSignalGroup => typeof group === "string" && allowed.has(group as ScrubSignalGroup)))]
    : [...allowedSignalGroups];
  return { mode, targetIds, signalGroups };
}
