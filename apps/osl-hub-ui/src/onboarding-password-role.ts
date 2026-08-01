import { isValidMainPassword, isValidNewMainPassword } from "./core";

export const BURN_PASSWORD_CONFIRMATION = "ERASE";

export type OnboardingPasswordRole = "stealth" | "burn";

export interface OnboardingPasswordRoleValues {
  current: string;
  alternate: string;
  confirm: string;
  burnConfirmation: string;
}

export function canSetOnboardingPasswordRole(
  role: OnboardingPasswordRole,
  { current, alternate, confirm, burnConfirmation }: OnboardingPasswordRoleValues,
): boolean {
  const passwordsAreValid = isValidMainPassword(current)
    && isValidNewMainPassword(alternate)
    && alternate === confirm
    && alternate !== current;
  return passwordsAreValid && (role !== "burn" || burnConfirmation === BURN_PASSWORD_CONFIRMATION);
}
