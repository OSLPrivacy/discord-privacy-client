import { RETAINED_SETUP_ROUTES } from "./onboarding-route-contract";

/**
 * The canonical order of the account-setup flow. Entry routes (`create`,
 * `import`, and `unlock`) are deliberately absent:
 * they branch into or out of this setup spine rather than being setup steps.
 *
 * TASK 6802: the spine is now exactly `RETAINED_SETUP_ROUTES`. Tutorial, Silent
 * Visible, Onboarding Detected, the separate Install route and Onboarding Apps
 * were deleted by owner rulings D4/D5 and replaced by one `setup-apps` page, so
 * there is no longer any branch in this sequence — every retained step is
 * always reachable and always in the same order.
 */
export const ONBOARDING_SEQUENCE = RETAINED_SETUP_ROUTES;

export type OnboardingSequenceRoute = (typeof ONBOARDING_SEQUENCE)[number];

/**
 * Retained for callers, deliberately empty of decisions. The two branch flags
 * it used to carry (`detected`, `install`) named deleted routes; keeping the
 * shape without them means a caller cannot resurrect a branch by setting one.
 */
export type OnboardingSequenceBranch = Record<string, never>;

function isEnabled(_route: OnboardingSequenceRoute, _branch: OnboardingSequenceBranch): boolean {
  return true;
}

function adjacentRoute(
  current: string,
  branch: OnboardingSequenceBranch,
  direction: -1 | 1,
): OnboardingSequenceRoute | null {
  let index = ONBOARDING_SEQUENCE.indexOf(current as OnboardingSequenceRoute);
  while (index >= 0 && index < ONBOARDING_SEQUENCE.length) {
    index += direction;
    const candidate = ONBOARDING_SEQUENCE[index];
    if (!candidate) return null;
    if (isEnabled(candidate, branch)) return candidate;
  }
  return null;
}

export function previousOnboardingRoute(
  current: string,
  branch: OnboardingSequenceBranch,
): OnboardingSequenceRoute | null {
  return adjacentRoute(current, branch, -1);
}

export function nextOnboardingRoute(
  current: string,
  branch: OnboardingSequenceBranch,
): OnboardingSequenceRoute | null {
  return adjacentRoute(current, branch, 1);
}

/**
 * T15 owns only the Pro step's placement and exit guarantees. T16 owns code
 * redemption and returns whether access is active; this contract deliberately
 * does not interpret activation codes or define their billing lifetime.
 */
export type ProOnboardingOutcome = "skipped" | "failed" | "activated";

export interface ProOnboardingContinuation {
  route: "forward-secrecy";
  access: "free" | "pro";
}

export const proOnboardingStepContract = {
  skippable: true,
  worksOffline: true,
} as const;

export function continueFromProOnboarding(outcome: ProOnboardingOutcome): ProOnboardingContinuation {
  return {
    route: "forward-secrecy",
    access: outcome === "activated" ? "pro" : "free",
  };
}
