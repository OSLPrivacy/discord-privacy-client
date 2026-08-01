import { describe, expect, it } from "vitest";

import { autoScrubConsentPrompt, decideAutoScrubInstall, type AutoScrubConsent } from "./component-consent";

const discordConsent: AutoScrubConsent = {
  serviceId: "discord",
  understandsAccountTerminationRisk: true,
  action: "install-autoscrub",
};

describe("AutoScrub component consent", () => {
  it("refuses picker installation until the account-termination risk is explicitly acknowledged", () => {
    expect(decideAutoScrubInstall("discord", null)).toEqual({
      allowed: false,
      serviceId: "discord",
      attendedScrubAvailable: true,
      reason: "explicit-consent-required",
    });
    expect(decideAutoScrubInstall("discord", { ...discordConsent, understandsAccountTerminationRisk: false })).toEqual({
      allowed: false,
      serviceId: "discord",
      attendedScrubAvailable: true,
      reason: "account-termination-risk-not-acknowledged",
    });
  });

  it("requires the consent step to name the same service that will receive AutoScrub", () => {
    expect(decideAutoScrubInstall("gmail", discordConsent)).toEqual({
      allowed: false,
      serviceId: "gmail",
      attendedScrubAvailable: true,
      reason: "explicit-consent-required",
    });
  });

  it("permits only a separate, acknowledged install action for that service", () => {
    expect(decideAutoScrubInstall("discord", discordConsent)).toEqual({
      allowed: true,
      serviceId: "discord",
      attendedScrubAvailable: true,
    });
  });

  it("presents the account-termination risk before the user can consent", () => {
    expect(autoScrubConsentPrompt("Discord")).toContain("could terminate your account");
  });
});
