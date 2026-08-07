import { isValidNewMainPassword } from "./core";

export type PasswordSetupStep = "create";

export type PasswordSetupContinueRefusalReason = "invalid-password" | "mismatched-passwords";

export interface PasswordSetupContinueAccepted {
  accepted: true;
  route: "recovery";
}

export interface PasswordSetupContinueRefused {
  accepted: false;
  route: PasswordSetupStep;
  reason: PasswordSetupContinueRefusalReason;
  message: string;
}

export type PasswordSetupContinueDecision = PasswordSetupContinueAccepted | PasswordSetupContinueRefused;

export function continuePasswordSetup(
  route: PasswordSetupStep,
  password: string,
  confirm: string,
): PasswordSetupContinueDecision {
  if (!isValidNewMainPassword(password)) {
    return {
      accepted: false,
      route,
      reason: "invalid-password",
      message: "Password must be 6 to 128 printable keyboard characters.",
    };
  }
  if (password !== confirm) {
    return {
      accepted: false,
      route,
      reason: "mismatched-passwords",
      message: "Both passwords must match.",
    };
  }
  return { accepted: true, route: "recovery" };
}
