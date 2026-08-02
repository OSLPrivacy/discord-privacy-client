import type { HubLicenseState } from "./core";

/** The generation path requested by the reported send state. */
export type CoverGenerationCarrier = "local-ai" | "cloud";

/**
 * Cloud consent is reported separately from the entitlement.  In particular,
 * activating Pro must not be treated as granting consent to cloud processing.
 */
export type ReportedCloudGenerationConsent = "granted" | "declined" | "unavailable";

export interface CoverGenerationEligibilityInput {
  readonly carrier: CoverGenerationCarrier;
  readonly entitlement: HubLicenseState;
  readonly cloudConsent: ReportedCloudGenerationConsent;
}

export interface CoverGenerationPresentationEligibility {
  readonly renderProgress: boolean;
  readonly renderPopup: boolean;
  readonly reason: "eligible" | "pro-required" | "cloud-consent-required";
}

const hiddenForPro = (): CoverGenerationPresentationEligibility => ({
  renderProgress: false,
  renderPopup: false,
  reason: "pro-required",
});

const hiddenForCloudConsent = (): CoverGenerationPresentationEligibility => ({
  renderProgress: false,
  renderPopup: false,
  reason: "cloud-consent-required",
});

const visible = (): CoverGenerationPresentationEligibility => ({
  renderProgress: true,
  renderPopup: true,
  reason: "eligible",
});

/**
 * Determines whether T7 may render generation progress from authoritative,
 * reported state.  The default is hidden: Free remains word-bank-only, and a
 * cloud request remains hidden until a separate cloud-consent grant arrives.
 */
export function coverGenerationPresentationEligibility(
  input: CoverGenerationEligibilityInput,
): CoverGenerationPresentationEligibility {
  const hasProEntitlement = input.entitlement.access === "pro" || input.entitlement.access === "offlineGrace";
  if (!hasProEntitlement) return hiddenForPro();
  if (input.carrier === "cloud" && input.cloudConsent !== "granted") return hiddenForCloudConsent();
  return visible();
}
