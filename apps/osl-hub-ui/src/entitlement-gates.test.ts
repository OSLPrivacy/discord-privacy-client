import { describe, expect, it } from "vitest";
import type { HubLicenseState } from "./core";
import {
  aiCarrierForEntitlement,
  chatPreviewHidingVisible,
  uiProGateProblems,
  uiProGates,
} from "./entitlement-gates";

describe("UI entitlement gates", () => {
  it("records every Pro gate as native-enforced or cosmetic with a reason", () => {
    expect(uiProGateProblems()).toEqual([]);
    expect(uiProGates.every((gate) => gate.enforcement === "native" || gate.enforcement === "cosmetic")).toBe(true);
    expect(uiProGates.every((gate) => gate.reason.trim().length > 0)).toBe(true);
  });

  it("keeps local chat preview hiding free", () => {
    expect(chatPreviewHidingVisible(true)).toBe(true);
    expect(chatPreviewHidingVisible(false)).toBe(false);
  });

  it("rejects an unclassified UI gate", () => {
    expect(uiProGateProblems([{ id: "new-gate", enforcement: "unknown" as never, reason: "A reason." }])).toEqual([
      "UI Pro gate new-gate must be native-enforced or cosmetic.",
    ]);
  });

  it("keeps the word-bank carrier working for Free and lapsed installs", () => {
    expect(aiCarrierForEntitlement({
      access: "free",
      requestedCarrier: "local-ai",
      localModelAvailable: true,
      cloudConsent: "granted",
    })).toBe("word-bank");
    const lapsedPro = {
      access: "free",
      status: "EXPIRED",
      currentPeriodEnd: 1_700_000_000,
      lastValidatedAt: 1_700_000_000,
    } as const satisfies HubLicenseState;

    expect(aiCarrierForEntitlement({
      access: lapsedPro.access,
      requestedCarrier: "cloud",
      localModelAvailable: true,
      cloudConsent: "granted",
    })).toBe("word-bank");
  });

  it("allows only an entitled, available local model or consented cloud carrier", () => {
    expect(aiCarrierForEntitlement({
      access: "pro",
      requestedCarrier: "local-ai",
      localModelAvailable: false,
      cloudConsent: "granted",
    })).toBe("word-bank");
    expect(aiCarrierForEntitlement({
      access: "offlineGrace",
      requestedCarrier: "local-ai",
      localModelAvailable: true,
      cloudConsent: "declined",
    })).toBe("local-ai");
    expect(aiCarrierForEntitlement({
      access: "pro",
      requestedCarrier: "cloud",
      localModelAvailable: false,
      cloudConsent: "declined",
    })).toBe("word-bank");
    expect(aiCarrierForEntitlement({
      access: "pro",
      requestedCarrier: "cloud",
      localModelAvailable: false,
      cloudConsent: "granted",
    })).toBe("cloud");
  });
});
