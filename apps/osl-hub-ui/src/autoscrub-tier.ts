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
  readonly unattendedExecutionAllowed: false;
  readonly requiresHumanPresence: true;
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

export interface AttendedTieredRunOptions extends AutoScrubRunOptions {
  readonly tier: ScrubTier;
  readonly optionalProModuleInstalled: boolean;
  readonly providerRiskRank: ScrubProviderRiskRank;
  readonly consentAuthority: AutoScrubConsentAuthority;
}

export type AttendedTieredRunResult =
  | { readonly state: "refused"; readonly reason: "rank-4-live-consent-required" }
  | { readonly state: "completed"; readonly result: AutoScrubRunResult };

/**
 * Both shipped tiers are attended. Pro may offer reviewed-plan replay, but it
 * never turns a destructive run into background or set-and-forget work.
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
  return Object.freeze({
    tier,
    label: "Attended AutoScrub",
    detail: "Repeat only a reviewed plan while you are present. It pauses on friction and never restarts itself.",
    unattendedExecutionAllowed: false,
    requiresHumanPresence: true,
    optionalProModuleInstalled,
  });
}

/**
 * Starts exactly one attended run. Rank-4 consent is read from native
 * authority for every invocation; the underlying flow then obtains its own
 * fresh step-up rather than accepting one captured when the plan was reviewed.
 */
export async function runAttendedTieredScrub(
  options: AttendedTieredRunOptions,
): Promise<AttendedTieredRunResult> {
  const status = autoScrubTierStatus(options.tier, options.optionalProModuleInstalled);
  if (status.unattendedExecutionAllowed || !status.requiresHumanPresence) {
    throw new Error("AutoScrub tier contract must remain attended");
  }
  if (options.providerRiskRank === 4) {
    const consent = await options.consentAuthority.checkLiveConsent(options.target.providerId);
    if (consent.serviceId !== options.target.providerId || consent.state !== "live") {
      return { state: "refused", reason: "rank-4-live-consent-required" };
    }
  }
  return { state: "completed", result: await runAutoScrubBatch(options) };
}
