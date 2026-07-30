import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("./preferences", async () => {
  const actual = await vi.importActual<typeof import("./preferences")>("./preferences");
  return { ...actual, isTauriRuntime: mocks.isTauriRuntime };
});

import {
  loadAutoScrubRunFleetStatus,
  requestAutoScrubGlobalStop,
  startAutoScrubReviewedRun,
} from "./autoscrub-unattended-run";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const runnerSource = readFileSync(new URL("./autoscrub-unattended-run.ts", import.meta.url), "utf8");
const contractSource = readFileSync(new URL("./autoscrub-contract.ts", import.meta.url), "utf8");

const fleetStatus = {
  contract: "autoscrubRunFleet.v1",
  openRunCount: 2,
  globalStopRequested: false,
  unattendedExecutionAllowed: false,
  quitGuard: {
    state: "notRequested",
    honestRemainingSecondsEstimate: null,
    reason: "No stop request is active.",
  },
  runs: [
    {
      runId: "run-001",
      serviceId: "discord",
      phase: "running",
      reviewedItemCount: 3,
      remainingItemCount: 2,
      stopRequested: false,
      mutationAllowed: false,
      lastOutcome: "prepared",
    },
    {
      runId: "run-002",
      serviceId: "telegram",
      phase: "reviewRequired",
      reviewedItemCount: 1,
      remainingItemCount: 0,
      stopRequested: false,
      mutationAllowed: false,
      lastOutcome: "held",
    },
  ],
} as const;

const reviewedRequest = {
  serviceId: "discord",
  accountId: "acct-discord-1",
  reviewToken: "review-token-1",
  planDigest: "a".repeat(64),
  reviewedItemCount: 3,
  consent: "reviewedBatchOnly",
} as const;

describe("AutoScrub unattended run production wiring", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("wires the production fleet load and stop commands through strict contracts", async () => {
    mocks.invoke.mockResolvedValue(fleetStatus);
    await expect(loadAutoScrubRunFleetStatus()).resolves.toEqual(fleetStatus);
    await expect(requestAutoScrubGlobalStop()).resolves.toEqual(fleetStatus);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "get_autoscrub_run_fl");
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "request_autoscrub_global_stop");
  });

  it("starts only an explicitly reviewed batch and refuses absent consent before native invocation", async () => {
    mocks.invoke.mockResolvedValue(fleetStatus);
    await expect(startAutoScrubReviewedRun(reviewedRequest)).resolves.toEqual(fleetStatus);
    expect(mocks.invoke).toHaveBeenCalledWith("start_autoscrub_reviewed_run", { request: reviewedRequest });

    mocks.invoke.mockClear();
    await expect(startAutoScrubReviewedRun({ ...reviewedRequest, consent: "unattended" as "reviewedBatchOnly" }))
      .rejects.toThrow("invalid AutoScrub reviewed run request");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("keeps main.ts on the production AutoScrub modules rather than raw status IPC", () => {
    expect(mainSource).toContain('from "./autoscrub-contract"');
    expect(mainSource).toContain('from "./autoscrub-unattended-run"');
    expect(mainSource).toContain("projectAutoScrubFleetStatus(autoScrubFleetStatus)");
    expect(mainSource).toContain("#autoscrub-stop");
    expect(runnerSource).toContain("get_autoscrub_run_fl");
    expect(runnerSource).not.toContain("get_autoscrub_run_status");
    expect(contractSource).toContain("unattendedExecutionAllowed: false");
  });
});


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
    const { autoscrubUnattendedContractGate, autoscrubUnattendedProductionRun } = await import("./autoscrub-unattended-run");
    const nativeInvoke = vi.fn();

    expect(mainSource).toContain("autoscrubUnattendedProductionRun");

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
