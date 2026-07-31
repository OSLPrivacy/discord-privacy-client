import type { LocalPrivacyFinding, PrivacyRiskCategory } from "./adapters";

export type ScrubSignalGroup = "language" | "sexual" | "personal" | "substances" | "conduct" | "work";
export type ScrubReceiptStatus = "verified_gone" | "still_present" | "unknown";

export interface ScrubSignalDefinition {
  id: ScrubSignalGroup;
  label: string;
  detail: string;
}

export interface ScrubReceipt {
  itemOrdinal: number;
  status: ScrubReceiptStatus;
  verifiedAtUnixMs: number | null;
}

export interface CompletedScrubRunStatusProjection {
  runState: "complete";
  completedAtUnixMs: number;
  userReviewed: true;
  accountBinding: "verified";
  cleanupAuthority: "user_confirmed";
  receipts: readonly ScrubReceipt[];
}

export const scrubSignalDefinitions: readonly ScrubSignalDefinition[] = [
  { id: "language", label: "Strong language", detail: "Profanity and cursing" },
  { id: "sexual", label: "Sexual content", detail: "Sexual or pornographic messages" },
  { id: "personal", label: "Personal information", detail: "Identity, money, health, location, and passwords" },
  { id: "substances", label: "Drug-related messages", detail: "Controlled substances or drug use" },
  { id: "conduct", label: "Possible illegal activity", detail: "A review reminder, not a legal judgment" },
  { id: "work", label: "Work and company secrets", detail: "Internal plans, customer data, and confidential files" },
] as const;

export const defaultScrubSignalGroups: readonly ScrubSignalGroup[] = scrubSignalDefinitions.map(({ id }) => id);

const scrubReceiptStatuses = new Set<ScrubReceiptStatus>(["verified_gone", "still_present", "unknown"]);
const maxScrubReceiptRows = 256;
const maxScrubTimestampUnixMs = 4_102_444_800_000;

const scrubReceiptCopy: Record<ScrubReceiptStatus, { label: string; detail: string }> = {
  verified_gone: {
    label: "Gone",
    detail: "OSL rechecked the original place and did not find the item.",
  },
  still_present: {
    label: "Still there",
    detail: "OSL rechecked and the item was still visible. Review it manually.",
  },
  unknown: {
    label: "Unknown",
    detail: "OSL could not verify the final state. Treat it as still needing review.",
  },
};

/** Permanent fail-closed contract for any future service-specific delete adapter. */
export const scrubDeletionContract = Object.freeze({
  privateApiAllowed: false,
  unattendedDeletionAllowed: false,
  completeEditableReviewRequiredEveryBatch: true,
  finalConfirmationRequiredEveryBatch: true,
  requestedDeletionCountsAsVerified: false,
  browserUiAutomationAllowed: false,
  desktopUiAutomationAllowed: false,
  privateProviderApisAllowed: false,
  humanBehaviorMimicryAllowed: false,
  documentedProviderDeleteApiRequired: true,
  stopOn: ["rate_limit", "challenge", "content_mismatch", "verification_failure"] as const,
});

const categoryGroups: Record<PrivacyRiskCategory, ScrubSignalGroup> = {
  credential: "personal",
  recovery_material: "personal",
  payment_card: "personal",
  government_identity: "personal",
  precise_location: "personal",
  sensitive_health: "personal",
  profanity: "language",
  sexual_content: "sexual",
  controlled_substances: "substances",
  potentially_unlawful_conduct: "conduct",
  work_sensitive_information: "work",
};

export function scrubSignalGroupFor(category: PrivacyRiskCategory): ScrubSignalGroup {
  return categoryGroups[category];
}

export function parseScrubSignalGroups(raw: string | null): Set<ScrubSignalGroup> {
  if (raw === null) return new Set(defaultScrubSignalGroups);
  try {
    const candidate = JSON.parse(raw) as unknown;
    if (!Array.isArray(candidate) || candidate.length > scrubSignalDefinitions.length) return new Set(defaultScrubSignalGroups);
    const allowed = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
    if (!candidate.every((value): value is ScrubSignalGroup => typeof value === "string" && allowed.has(value as ScrubSignalGroup))) {
      return new Set(defaultScrubSignalGroups);
    }
    return new Set(candidate);
  } catch {
    return new Set(defaultScrubSignalGroups);
  }
}

export function enabledScrubFindings(
  findings: readonly LocalPrivacyFinding[],
  enabled: ReadonlySet<ScrubSignalGroup>,
): LocalPrivacyFinding[] {
  return findings.filter((finding) => enabled.has(scrubSignalGroupFor(finding.category)));
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function boundedUnixMs(value: unknown): value is number {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= 0
    && value <= maxScrubTimestampUnixMs;
}

function parseScrubReceipt(raw: unknown): ScrubReceipt | null {
  if (!exactRecord(raw, ["itemOrdinal", "status", "verifiedAtUnixMs"])
    || typeof raw.itemOrdinal !== "number"
    || !Number.isSafeInteger(raw.itemOrdinal)
    || raw.itemOrdinal < 1
    || raw.itemOrdinal > maxScrubReceiptRows
    || !scrubReceiptStatuses.has(raw.status as ScrubReceiptStatus)
    || !(raw.verifiedAtUnixMs === null || boundedUnixMs(raw.verifiedAtUnixMs))) {
    return null;
  }
  if (raw.status !== "unknown" && raw.verifiedAtUnixMs === null) return null;
  return {
    itemOrdinal: raw.itemOrdinal,
    status: raw.status as ScrubReceiptStatus,
    verifiedAtUnixMs: raw.verifiedAtUnixMs,
  };
}

export function parseCompletedScrubRunStatusProjection(raw: unknown): CompletedScrubRunStatusProjection | null {
  if (!exactRecord(raw, ["runState", "completedAtUnixMs", "userReviewed", "accountBinding", "cleanupAuthority", "receipts"])
    || raw.runState !== "complete"
    || !boundedUnixMs(raw.completedAtUnixMs)
    || raw.userReviewed !== true
    || raw.accountBinding !== "verified"
    || raw.cleanupAuthority !== "user_confirmed"
    || !Array.isArray(raw.receipts)
    || raw.receipts.length > maxScrubReceiptRows) {
    return null;
  }
  const receipts = raw.receipts.map(parseScrubReceipt);
  if (receipts.some((receipt) => receipt === null)) return null;
  const ordinals = new Set(receipts.map((receipt) => receipt?.itemOrdinal));
  if (ordinals.size !== receipts.length) return null;
  return {
    runState: "complete",
    completedAtUnixMs: raw.completedAtUnixMs,
    userReviewed: true,
    accountBinding: "verified",
    cleanupAuthority: "user_confirmed",
    receipts: receipts as ScrubReceipt[],
  };
}

export function scrubRunStatusProjectionCounts(projection: CompletedScrubRunStatusProjection): Record<ScrubReceiptStatus, number> {
  return projection.receipts.reduce<Record<ScrubReceiptStatus, number>>((counts, receipt) => {
    counts[receipt.status] += 1;
    return counts;
  }, { verified_gone: 0, still_present: 0, unknown: 0 });
}

function scrubProjectionTime(value: number): string {
  return new Date(value).toISOString().replace(/\.\d{3}Z$/u, "Z");
}

export function completedScrubRunStatusMarkup(raw: unknown): string {
  const projection = parseCompletedScrubRunStatusProjection(raw);
  if (projection === null) {
    return '<section class="cleanup-run-results refused" role="status"><h3>Cleanup status unavailable</h3><p>OSL could not verify this completed cleanup run. Treat every item as still needing review.</p></section>';
  }

  const counts = scrubRunStatusProjectionCounts(projection);
  const summary = [
    `${counts.verified_gone} gone`,
    `${counts.still_present} still there`,
    `${counts.unknown} unknown`,
  ].join(" / ");
  const rows = projection.receipts
    .slice()
    .sort((left, right) => left.itemOrdinal - right.itemOrdinal)
    .map((receipt) => {
      const copy = scrubReceiptCopy[receipt.status];
      const checked = receipt.verifiedAtUnixMs === null ? "Not verified" : `Checked ${scrubProjectionTime(receipt.verifiedAtUnixMs)}`;
      return `<li class="cleanup-result ${receipt.status}" data-cleanup-status="${receipt.status}"><span>Item ${receipt.itemOrdinal}</span><strong>${copy.label}</strong><small>${copy.detail} ${checked}.</small></li>`;
    })
    .join("");

  return `<section class="cleanup-run-results complete" role="status" aria-live="polite"><header><h3>Cleanup results</h3><small>Completed ${scrubProjectionTime(projection.completedAtUnixMs)}</small></header><p>${summary}</p>${rows ? `<ol>${rows}</ol>` : '<div class="empty-state"><strong>No cleanup items were selected</strong><p>There is nothing to review for this run.</p></div>'}</section>`;
}
