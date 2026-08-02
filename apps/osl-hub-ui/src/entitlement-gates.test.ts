import { describe, expect, it } from "vitest";
import { chatPreviewHidingVisible, uiProGateProblems, uiProGates } from "./entitlement-gates";

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
});
