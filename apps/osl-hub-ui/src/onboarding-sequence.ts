/**
 * The canonical order of the account-setup flow. Entry routes (`create`,
 * `import`, and `unlock`) and the decoy workspace are deliberately absent:
 * they branch into or out of this setup spine rather than being setup steps.
 */
export const ONBOARDING_SEQUENCE = [
  "welcome",
  "recovery",
  "pro",
  "privacy",
  "defaults",
  "sending",
  "cover",
  "passwords",
  "burnpass",
  "mullvad",
  "browser",
  "tutorial",
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
