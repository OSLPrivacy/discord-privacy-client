import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
  });
}

describe("autoscrub unattended run contract", () => {
  it("Wire autoscrub-unattended-run.ts + autoscrub-contract.ts as production", async () => {
    installGlobals();
    const { autoscrubUnattendedContractGate, autoscrubUnattendedProductionRun } = await import("./main");
    const nativeInvoke = vi.fn();

    expect(autoscrubUnattendedContractGate({
      production: false,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    })).toEqual({ state: "refused", reason: "not-production" });
    expect(autoscrubUnattendedContractGate({
      production: true,
      unattendedAllowed: false,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    })).toEqual({ state: "refused", reason: "unattended-disabled" });
    expect(autoscrubUnattendedContractGate({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: true,
      externalSecurityReviewPassed: true,
    })).toEqual({ state: "refused", reason: "review-required" });
    expect(autoscrubUnattendedContractGate({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: false,
    })).toEqual({ state: "refused", reason: "external-review-required" });
    expect(autoscrubUnattendedContractGate({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    })).toEqual({ state: "ready", command: "autoscrub_unattended_run" });

    await expect(autoscrubUnattendedProductionRun({
      production: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    }, nativeInvoke)).resolves.toEqual({ state: "refused", reason: "invalid-contract" });
    expect(nativeInvoke).not.toHaveBeenCalled();

    nativeInvoke.mockResolvedValueOnce({ runId: "autoscrub-run-0001", working: 1, totalRuns: 1 });
    await expect(autoscrubUnattendedProductionRun({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    }, nativeInvoke)).resolves.toEqual({
      state: "started",
      command: "autoscrub_unattended_run",
      runId: "autoscrub-run-0001",
      working: 1,
      totalRuns: 1,
    });
    expect(nativeInvoke).toHaveBeenCalledWith("autoscrub_unattended_run", {
      contract: {
        production: true,
        unattendedAllowed: true,
        reviewRequiredEveryBatch: false,
        externalSecurityReviewPassed: true,
      },
    });

    nativeInvoke.mockResolvedValueOnce({ runId: "autoscrub-run-0002", working: 3, totalRuns: 3 });
    await expect(autoscrubUnattendedProductionRun({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    }, nativeInvoke)).resolves.toEqual({ state: "refused", reason: "native-refused" });
  });
});
