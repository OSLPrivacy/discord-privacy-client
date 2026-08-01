import { describe, expect, it } from "vitest";
import {
  SCRUB_CONSENT_GATE_PRESENTATION,
  consentAcknowledgementForService,
  defaultScrubConsentGateState,
  evaluateScrubConsentGate,
  scrubConsentGateMarkup,
  type ScrubConsentGateRequest,
} from "./scrub-consent-gate";

const discord: ScrubConsentGateRequest = {
  serviceId: "discord",
  serviceName: "Discord",
  warning: "Discord prohibits this automation and can permanently terminate your account.",
};

describe("SCR-K3 scrub consent gate", () => {
  it("binds consent to one service and requires both an unticked control and exact typed acknowledgement", () => {
    const state = defaultScrubConsentGateState();
    expect(state.checked).toBe(false);

    const acknowledgement = consentAcknowledgementForService(discord.serviceName);
    expect(evaluateScrubConsentGate(discord, { checked: true, typedAcknowledgement: acknowledgement })).toEqual({
      allowed: true,
      serviceId: "discord",
    });
    expect(evaluateScrubConsentGate(discord, { checked: true, typedAcknowledgement: "" })).toEqual({
      allowed: false,
      reason: "typed-acknowledgement-required",
    });
    expect(evaluateScrubConsentGate(
      { ...discord, serviceId: "x", serviceName: "X" },
      { checked: true, typedAcknowledgement: acknowledgement },
    )).toEqual({ allowed: false, reason: "typed-acknowledgement-required" });
    expect(evaluateScrubConsentGate(discord, { checked: true, typedAcknowledgement: "I understand." })).toEqual({
      allowed: false,
      reason: "typed-acknowledgement-required",
    });
  });

  it("places the warning and explicit gate on the destructive action path without visually diminishing it", () => {
    const markup = scrubConsentGateMarkup(discord, defaultScrubConsentGateState());

    expect(markup).toContain('data-scrub-action-path="delete"');
    expect(markup).toContain('type="checkbox"');
    expect(markup).not.toContain('checked');
    expect(markup).toContain('disabled aria-disabled="true"');
    expect(SCRUB_CONSENT_GATE_PRESENTATION.warningFontSizePx)
      .toBeGreaterThanOrEqual(SCRUB_CONSENT_GATE_PRESENTATION.proceedFontSizePx);
    expect(SCRUB_CONSENT_GATE_PRESENTATION.warningFontWeight)
      .toBeGreaterThanOrEqual(SCRUB_CONSENT_GATE_PRESENTATION.proceedFontWeight);
  });
});
