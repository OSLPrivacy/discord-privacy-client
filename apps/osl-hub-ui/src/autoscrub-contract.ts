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
  return typeof raw.production === "boolean"
    && typeof raw.unattendedAllowed === "boolean"
    && typeof raw.reviewRequiredEveryBatch === "boolean"
    && typeof raw.externalSecurityReviewPassed === "boolean"
    ? raw as unknown as AutoscrubUnattendedContract
    : null;
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
  return {
    state: "started",
    command: AUTOSCRUB_UNATTENDED_RUN_COMMAND,
    runId: raw.runId,
    working: Number(raw.working),
    totalRuns: Number(raw.totalRuns),
  };
}
