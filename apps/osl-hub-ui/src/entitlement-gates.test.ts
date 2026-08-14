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
    })).toBe("word-bank");
    const lapsedPro = {
      access: "free",
      status: "EXPIRED",
      currentPeriodEnd: 1_700_000_000,
      lastValidatedAt: 1_700_000_000,
    } as const satisfies HubLicenseState;

    expect(aiCarrierForEntitlement({
      access: lapsedPro.access,
      requestedCarrier: "local-ai",
      localModelAvailable: true,
    })).toBe("word-bank");
  });

  it("allows only an entitled, available local model", () => {
    expect(aiCarrierForEntitlement({
      access: "pro",
      requestedCarrier: "local-ai",
      localModelAvailable: false,
    })).toBe("word-bank");
    expect(aiCarrierForEntitlement({
      access: "offlineGrace",
      requestedCarrier: "local-ai",
      localModelAvailable: true,
    })).toBe("local-ai");
  });
});
