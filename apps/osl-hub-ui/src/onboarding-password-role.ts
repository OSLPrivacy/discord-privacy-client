import { isValidMainPassword, isValidNewMainPassword } from "./core";

export const BURN_PASSWORD_CONFIRMATION = "ERASE";

export type OnboardingPasswordRole = "stealth" | "burn";

export interface OnboardingPasswordRoleValues {
  current: string;
  alternate: string;
  confirm: string;
  burnConfirmation: string;
}

export type OnboardingPasswordRoleRoute = "visibility" | "passwords" | "burnpass" | "pro";

export interface OnboardingPasswordRoleAction<T = never> {
  accepted: boolean;
  saved: boolean;
  route: OnboardingPasswordRoleRoute;
  status: T | null;
  reason: "invalid-passwords" | null;
}

const ROLE_ROUTES: Record<OnboardingPasswordRole, {
  current: OnboardingPasswordRoleRoute;
  previous: OnboardingPasswordRoleRoute;
  next: OnboardingPasswordRoleRoute;
}> = {
  stealth: { current: "passwords", previous: "visibility", next: "burnpass" },
  burn: { current: "burnpass", previous: "passwords", next: "pro" },
};

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

/**
 * Validate at the submit boundary and await the durable write before exposing
 * the next route. Button state alone is not authority: keyboard and scripted
 * submissions can race a stale disabled state.
 */
export async function continueOnboardingPasswordRole<T>(
  role: OnboardingPasswordRole,
  values: OnboardingPasswordRoleValues,
  save: (role: OnboardingPasswordRole, current: string, alternate: string) => Promise<T>,
): Promise<OnboardingPasswordRoleAction<T>> {
  if (!canSetOnboardingPasswordRole(role, values)) {
    return {
      accepted: false,
      saved: false,
      route: ROLE_ROUTES[role].current,
      status: null,
      reason: "invalid-passwords",
    };
  }

  const status = await save(role, values.current, values.alternate);
  return {
    accepted: true,
    saved: true,
    route: ROLE_ROUTES[role].next,
    status,
    reason: null,
  };
}

/** Skip is navigation-only and therefore cannot accidentally persist input. */
export function skipOnboardingPasswordRole(role: OnboardingPasswordRole): OnboardingPasswordRoleAction {
  return {
    accepted: true,
    saved: false,
    route: ROLE_ROUTES[role].next,
    status: null,
    reason: null,
  };
}

/** Back is navigation-only and leaves the saved role unchanged. */
export function backOnboardingPasswordRole(role: OnboardingPasswordRole): OnboardingPasswordRoleAction {
  return {
    accepted: true,
    saved: false,
    route: ROLE_ROUTES[role].previous,
    status: null,
    reason: null,
  };
}

export interface PasswordVisibilityAction {
  type: "password" | "text";
  label: "Show password" | "Hide password";
  visible: boolean;
}

export function togglePasswordVisibility(type: "password" | "text"): PasswordVisibilityAction {
  const visible = type === "password";
  return {
    type: visible ? "text" : "password",
    label: visible ? "Hide password" : "Show password",
    visible,
  };
}
