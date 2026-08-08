import type { HubLicenseState } from "./core";

/** The generation path requested by the reported send state. */
export type CoverGenerationCarrier = "local-ai";

export interface CoverGenerationEligibilityInput {
  readonly carrier: CoverGenerationCarrier;
  readonly entitlement: HubLicenseState;
}

export interface CoverGenerationPresentationEligibility {
  readonly renderProgress: boolean;
  readonly renderPopup: boolean;
  readonly reason: "eligible" | "pro-required";
}

const hiddenForPro = (): CoverGenerationPresentationEligibility => ({
  renderProgress: false,
  renderPopup: false,
  reason: "pro-required",
});

const visible = (): CoverGenerationPresentationEligibility => ({
  renderProgress: true,
  renderPopup: true,
  reason: "eligible",
});

/**
 * Determines whether T7 may render generation progress from authoritative,
 * reported state. The default is hidden: Free remains word-bank-only.
 */
export function coverGenerationPresentationEligibility(
  input: CoverGenerationEligibilityInput,
): CoverGenerationPresentationEligibility {
  const hasProEntitlement = input.entitlement.access === "pro" || input.entitlement.access === "offlineGrace";
  if (!hasProEntitlement) return hiddenForPro();
  return visible();
}
