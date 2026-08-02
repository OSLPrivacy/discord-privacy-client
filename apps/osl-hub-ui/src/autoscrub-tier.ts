import {
  runAutoScrubBatch,
  type AutoScrubRunOptions,
  type AutoScrubRunResult,
} from "./autoscrub-flow";

export type ScrubTier = "free" | "pro";
export type ScrubProviderRiskRank = 1 | 2 | 3 | 4 | 5;

export interface AutoScrubTierStatus {
  readonly tier: ScrubTier;
  readonly label: string;
  readonly detail: string;
  readonly unattendedExecutionAllowed: boolean;
  readonly requiresHumanPresence: boolean;
  readonly optionalProModuleInstalled: boolean;
}

export interface Rank4ConsentCheck {
  readonly serviceId: string;
  readonly state: "live" | "missing" | "revoked" | "expired";
}

/** Native authority for the high-risk, service-specific consent check. */
export interface AutoScrubConsentAuthority {
  checkLiveConsent(serviceId: string): Promise<Rank4ConsentCheck>;
}

export interface TieredRunOptions extends AutoScrubRunOptions {
  readonly tier: ScrubTier;
  readonly optionalProModuleInstalled: boolean;
  readonly providerRiskRank: ScrubProviderRiskRank;
  readonly consentAuthority: AutoScrubConsentAuthority;
}

export type TieredRunResult =
  | { readonly state: "refused"; readonly reason: "pro-module-required" | "rank-4-live-consent-required" }
  | { readonly state: "completed"; readonly result: AutoScrubRunResult };

/**
 * Free is a reviewed, one-time attended flow.  The optional Pro module is the
 * only tier permitted to repeat an approved plan unattended, and only while
 * its native authority, content bounds, pacing, and stop conditions remain
 * live.  Do not collapse this distinction into an "attended Pro" label:
 * owner decision D85 overrides the older master-spec wording.
 */
export function autoScrubTierStatus(
  tier: ScrubTier,
  optionalProModuleInstalled: boolean,
): AutoScrubTierStatus {
  if (tier === "free") {
    return Object.freeze({
      tier,
      label: "Attended Scrub",
      detail: "Review, confirm, attempt, verify, and read the receipt while you are present. No Pro module is required.",
      unattendedExecutionAllowed: false,
      requiresHumanPresence: true,
      optionalProModuleInstalled,
    });
  }
  if (!optionalProModuleInstalled) {
    return Object.freeze({
      tier,
      label: "AutoScrub unavailable",
      detail: "The optional Pro module is not installed. Free Scrub remains a reviewed one-time flow.",
      unattendedExecutionAllowed: false,
      requiresHumanPresence: false,
      optionalProModuleInstalled,
    });
  }
  return Object.freeze({
    tier,
    label: "Unattended AutoScrub",
    detail: "Repeat a reviewed plan unattended only while native authority, approved content bounds, pacing, and stop conditions remain satisfied. It never restarts after friction or Stop/Revoke.",
    unattendedExecutionAllowed: true,
    requiresHumanPresence: false,
    optionalProModuleInstalled,
  });
}

/**
 * Starts exactly one tiered run. Rank-4 consent is read from native
 * authority for every invocation; the underlying flow then obtains its own
 * fresh step-up rather than accepting one captured when the plan was reviewed.
 * The optional module owns unattended replay scheduling; this UI helper never
 * pretends an attended launch is the Pro distinction.
 */
export async function runTieredScrub(
  options: TieredRunOptions,
): Promise<TieredRunResult> {
  const status = autoScrubTierStatus(options.tier, options.optionalProModuleInstalled);
  if (options.tier === "pro" && !status.optionalProModuleInstalled) {
    return { state: "refused", reason: "pro-module-required" };
  }
  if (options.providerRiskRank === 4) {
    const consent = await options.consentAuthority.checkLiveConsent(options.target.providerId);
    if (consent.serviceId !== options.target.providerId || consent.state !== "live") {
      return { state: "refused", reason: "rank-4-live-consent-required" };
    }
  }
  return { state: "completed", result: await runAutoScrubBatch(options) };
}

/** @deprecated Use `runTieredScrub`; Pro must not be described as attended. */
export const runAttendedTieredScrub = runTieredScrub;
