/**
 * T15 owns only the Pro step's placement and exit guarantees. T16 owns code
 * redemption and returns whether access is active; this contract deliberately
 * does not interpret activation codes or define their billing lifetime.
 */
export type ProOnboardingOutcome = "skipped" | "failed" | "activated";

export interface ProOnboardingContinuation {
  route: "privacy";
  access: "free" | "pro";
}

export const proOnboardingStepContract = {
  skippable: true,
  worksOffline: true,
} as const;

export function continueFromProOnboarding(outcome: ProOnboardingOutcome): ProOnboardingContinuation {
  return {
    route: "privacy",
    access: outcome === "activated" ? "pro" : "free",
  };
}
