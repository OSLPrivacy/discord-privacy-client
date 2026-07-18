import { describe, expect, it } from "vitest";
import {
  advanceSendMode,
  canCompleteSetup,
  makeCapsulePreview,
  needsRiskAcceptance,
  parseRustOnboardingPreferences,
  parseSetupState,
  toRustOnboardingPreferences,
} from "./state";

describe("setup safety", () => {
  it("allows manual and clipboard modes without experimental acceptance", () => {
    expect(needsRiskAcceptance("manual")).toBe(false);
    expect(needsRiskAcceptance("clipboard")).toBe(false);
  });

  it("requires acceptance for Enter automation", () => {
    expect(canCompleteSetup({ sendMode: "single", placementMode: "atomic", acceptedRisk: false, acceptedRiskForMode: null })).toBe(false);
    expect(canCompleteSetup({ sendMode: "single", placementMode: "atomic", acceptedRisk: true, acceptedRiskForMode: "single" })).toBe(true);
    expect(canCompleteSetup({ sendMode: "single", placementMode: "atomic", acceptedRisk: true, acceptedRiskForMode: "double" })).toBe(false);
  });
});

describe("capsule preview", () => {
  it("never includes message plaintext", () => {
    const preview = makeCapsulePreview("private launch details");
    expect(preview).toContain("osl://v1/demo");
    expect(preview).not.toContain("private");
  });
});

describe("persisted setup validation", () => {
  it("rejects unknown enum and non-boolean values", () => {
    expect(parseSetupState('{"sendMode":"stealth","placementMode":"typing","acceptedRisk":"yes"}')).toEqual({
      sendMode: "manual",
      placementMode: "atomic",
      acceptedRisk: false,
      acceptedRiskForMode: null,
    });
  });

  it("accepts only known settings", () => {
    expect(parseSetupState('{"sendMode":"double","placementMode":"compatibility","acceptedRisk":true,"acceptedRiskForMode":"double"}')).toEqual({
      sendMode: "double",
      placementMode: "compatibility",
      acceptedRisk: true,
      acceptedRiskForMode: "double",
    });
  });
});

describe("send-mode transitions", () => {
  it("requires two distinct advances for double Enter", () => {
    expect(advanceSendMode("double", "idle")).toEqual({ phase: "placed", action: "place" });
    expect(advanceSendMode("double", "placed")).toEqual({ phase: "idle", action: "send" });
  });

  it("never sends in manual or clipboard preparation", () => {
    expect(advanceSendMode("manual", "idle").action).toBe("prepare-manual");
    expect(advanceSendMode("clipboard", "idle").action).toBe("prepare-clipboard");
  });

  it("single Enter advances directly to a local send simulation", () => {
    expect(advanceSendMode("single", "idle")).toEqual({ phase: "idle", action: "send" });
  });
});

describe("Rust preference conversion", () => {
  it("strictly accepts the Rust camelCase contract", () => {
    expect(parseRustOnboardingPreferences({
      onboardingComplete: true,
      sendMode: "single",
      placementMode: "compatibility",
      showPlaintextPreview: false,
      acknowledgeExperimentalSendRisk: true,
    })).toEqual({
      onboardingComplete: true,
      setup: {
        sendMode: "single",
        placementMode: "compatibility",
        acceptedRisk: true,
        acceptedRiskForMode: "single",
      },
      showPlaintextPreview: false,
    });
  });

  it("fails conservatively on malformed Rust values", () => {
    expect(parseRustOnboardingPreferences({
      onboardingComplete: true,
      sendMode: "hidden-auto",
      placementMode: "compatibility",
      showPlaintextPreview: false,
      acknowledgeExperimentalSendRisk: true,
    }).onboardingComplete).toBe(false);
  });

  it("rejects unknown Rust response fields", () => {
    expect(parseRustOnboardingPreferences({
      onboardingComplete: true,
      sendMode: "manual",
      placementMode: "atomic",
      showPlaintextPreview: true,
      acknowledgeExperimentalSendRisk: false,
      silentlyEnableAutomation: true,
    }).onboardingComplete).toBe(false);
  });

  it("never serializes stale risk acceptance for another mode", () => {
    const serialized = toRustOnboardingPreferences({
      onboardingComplete: true,
      setup: {
        sendMode: "double",
        placementMode: "atomic",
        acceptedRisk: true,
        acceptedRiskForMode: "single",
      },
      showPlaintextPreview: true,
    });
    expect(serialized.acknowledgeExperimentalSendRisk).toBe(false);
    expect(serialized.onboardingComplete).toBe(false);
  });

  it("reopens setup when a risky persisted mode lacks acknowledgement", () => {
    expect(parseRustOnboardingPreferences({
      onboardingComplete: true,
      sendMode: "double",
      placementMode: "atomic",
      showPlaintextPreview: true,
      acknowledgeExperimentalSendRisk: false,
    }).onboardingComplete).toBe(false);
  });
});
