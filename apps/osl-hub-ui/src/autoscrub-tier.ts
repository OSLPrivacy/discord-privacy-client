import type { AutoScrubRunOptions, AutoScrubRunResult } from "./autoscrub-flow";

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

export interface Rank4ConsentCheck { readonly serviceId: string; readonly state: "live" | "missing" | "revoked" | "expired"; }
export interface AutoScrubConsentAuthority { checkLiveConsent(serviceId: string): Promise<Rank4ConsentCheck>; }
export interface TieredRunOptions extends AutoScrubRunOptions { readonly tier: ScrubTier; readonly optionalProModuleInstalled: boolean; readonly providerRiskRank: ScrubProviderRiskRank; readonly consentAuthority: AutoScrubConsentAuthority; }
export type TieredRunResult = { readonly state: "refused"; readonly reason: "pro-module-required" | "rank-4-live-consent-required" } | { readonly state: "completed"; readonly result: AutoScrubRunResult };

/**
 * Scheduled AutoScrub is Find only on every tier. Classification may create
 * review items, but it never authorizes or performs deletion.
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
    label: "Scheduled AutoScrub review",
    detail: "AutoScrub finds possible matches for review; it does not delete them automatically.",
    unattendedExecutionAllowed: false,
    requiresHumanPresence: false,
    optionalProModuleInstalled,
  });
}

/** Compatibility entrypoint retained outside the shipping execution graph. */
export async function runTieredScrub(options: TieredRunOptions): Promise<TieredRunResult> {
  const status = autoScrubTierStatus(options.tier, options.optionalProModuleInstalled);
  if (options.tier === "pro" && !status.optionalProModuleInstalled) return { state: "refused", reason: "pro-module-required" };
  if (options.providerRiskRank === 4) {
    const consent = await options.consentAuthority.checkLiveConsent(options.target.providerId);
    if (consent.serviceId !== options.target.providerId || consent.state !== "live") return { state: "refused", reason: "rank-4-live-consent-required" };
  }
  throw new Error("Scheduled AutoScrub is Find only; no batch deletion runner is installed");
}

export const runAttendedTieredScrub = runTieredScrub;
