import { describe, expect, it } from "vitest";
import { HonestSendContract, type ComposerPhase, type SendMode } from "./state";

function gate(overrides: Partial<Parameters<typeof HonestSendContract.gate>[0]> = {}) {
  return HonestSendContract.gate({
    mode: "clipboard",
    phase: "idle",
    consent: true,
    binding: true,
    authority: true,
    distinctUserGesture: false,
    ...overrides,
  });
}

describe("HonestSendContract", () => {
  it("defines only the honest tri-state send outcomes and never retries them automatically", () => {
    expect(HonestSendContract.outcomes).toEqual(["sent", "not-sent", "unknown"]);

    for (const outcome of HonestSendContract.outcomes) {
      expect(HonestSendContract.reportOutcome(outcome)).toMatchObject({
        outcome,
        autoRetry: false,
      });
    }

    expect(HonestSendContract.reportOutcome("platform-accepted-maybe")).toEqual({
      outcome: "unknown",
      autoRetry: false,
      preserveDraft: true,
    });
  });

  it("treats missing consent, binding, or authority as refusal, never permission", () => {
    expect(gate({ consent: false })).toEqual({
      allowed: false,
      action: "refuse",
      reason: "missing-consent",
      outcome: "not-sent",
      autoRetry: false,
    });
    expect(gate({ binding: false })).toEqual({
      allowed: false,
      action: "refuse",
      reason: "missing-binding",
      outcome: "not-sent",
      autoRetry: false,
    });
    expect(gate({ authority: false })).toEqual({
      allowed: false,
      action: "refuse",
      reason: "missing-authority",
      outcome: "not-sent",
      autoRetry: false,
    });
  });

  it("keeps Manual, Clipboard, and Double Enter as ordinary modes with Single Enter outside the ordinary set", () => {
    expect(HonestSendContract.modes.manual.ordinary).toBe(true);
    expect(HonestSendContract.modes.clipboard.ordinary).toBe(true);
    expect(HonestSendContract.modes.double.ordinary).toBe(true);
    expect(HonestSendContract.modes.single.ordinary).toBe(false);
  });

  it.each([
    ["manual", "idle", "prepare-manual"],
    ["clipboard", "idle", "prepare-clipboard"],
    ["double", "idle", "place"],
    ["double", "prepared", "place"],
    ["double", "placed", "send"],
    ["single", "idle", "send"],
  ] as const)("maps %s/%s to the configured action", (mode: SendMode, phase: ComposerPhase, action) => {
    expect(gate({ mode, phase, distinctUserGesture: true })).toEqual({
      allowed: true,
      action,
      autoRetry: false,
    });
  });

  it("requires a distinct second user gesture before Double Enter may send", () => {
    expect(gate({ mode: "double", phase: "placed", distinctUserGesture: false })).toEqual({
      allowed: false,
      action: "refuse",
      reason: "missing-distinct-user-gesture",
      outcome: "not-sent",
      autoRetry: false,
    });

    expect(gate({ mode: "double", phase: "placed", distinctUserGesture: true })).toEqual({
      allowed: true,
      action: "send",
      autoRetry: false,
    });
  });
});
