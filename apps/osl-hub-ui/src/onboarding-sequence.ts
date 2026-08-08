/**
 * The canonical order of the account-setup flow. Entry routes (`create`,
 * `import`, and `unlock`) and the decoy workspace are deliberately absent:
 * they branch into or out of this setup spine rather than being setup steps.
 */
export const ONBOARDING_SEQUENCE = [
  "welcome",
  "recovery",
  "recovery-check",
  "identity-choice",
  // 2026-08-08, owner's review note (UI-FEEDBACK.txt): the stealth and burn
  // passwords come IMMEDIATELY BEFORE the Pro code, not nine steps after it.
  "passwords",
  "burnpass",
  "pro",
  "forward-secrecy",
  "privacy",
  "tor",
  "defaults",
  "sending",
  "cover",
  "silent-visible",
  "visibility",
  "mullvad",
  "browser",
  // 2026-08-06: the tour left the onboarding spine on the owner's instruction.
  // The route and its five steps still exist -- Settings -> About replays them --
  // but nobody is walked through it on first run any more.
  "detected",
  "install",
  "apps",
] as const;

export type OnboardingSequenceRoute = (typeof ONBOARDING_SEQUENCE)[number];

export interface OnboardingSequenceBranch {
  detected: boolean;
  install: boolean;
}

function isEnabled(route: OnboardingSequenceRoute, branch: OnboardingSequenceBranch): boolean {
  if (route === "detected") return branch.detected;
  if (route === "install") return branch.install;
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
