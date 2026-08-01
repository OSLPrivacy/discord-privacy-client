export type AutoScrubConsent = Readonly<{
  serviceId: string;
  understandsAccountTerminationRisk: boolean;
  action: "install-autoscrub";
}>;

export type AutoScrubInstallDecision =
  | Readonly<{ allowed: true; serviceId: string; attendedScrubAvailable: true }>
  | Readonly<{
    allowed: false;
    serviceId: string;
    attendedScrubAvailable: true;
    reason: "explicit-consent-required" | "account-termination-risk-not-acknowledged";
  }>;

export function autoScrubConsentPrompt(serviceName: string): string {
  return `AutoScrub uses UI automation for ${serviceName}. This may break ${serviceName}'s rules and could terminate your account. Install it only if you understand and accept that risk.`;
}

/**
 * This gate deliberately has no default-allow path: callers must collect a
 * separate affirmative action for each service before installing AutoScrub.
 */
export function decideAutoScrubInstall(
  serviceId: string,
  consent: AutoScrubConsent | null,
): AutoScrubInstallDecision {
  if (consent === null || consent.serviceId !== serviceId || consent.action !== "install-autoscrub") {
    return { allowed: false, serviceId, attendedScrubAvailable: true, reason: "explicit-consent-required" };
  }
  if (!consent.understandsAccountTerminationRisk) {
    return { allowed: false, serviceId, attendedScrubAvailable: true, reason: "account-termination-risk-not-acknowledged" };
  }
  return { allowed: true, serviceId, attendedScrubAvailable: true };
}
