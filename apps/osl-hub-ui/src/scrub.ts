import type { LocalPrivacyFinding, PrivacyRiskCategory } from "./adapters";

export type ScrubSignalGroup = "language" | "sexual" | "personal" | "substances" | "conduct" | "work";

export interface ScrubSignalDefinition {
  id: ScrubSignalGroup;
  label: string;
  detail: string;
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

/** Permanent fail-closed contract for any future service-specific delete adapter. */
export const scrubDeletionContract = Object.freeze({
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
