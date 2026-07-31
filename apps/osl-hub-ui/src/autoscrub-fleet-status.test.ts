import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { loadAutoScrubRunFleetStatus } from "./autoscrub-unattended-run";
import { parseAutoScrubFleetStatus, projectAutoScrubFleetStatus } from "./autoscrub-contract";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const runnerSource = readFileSync(new URL("./autoscrub-unattended-run.ts", import.meta.url), "utf8");

const fleetStatus = {
  contract: "autoscrubRunFleet.v1",
  openRunCount: 1,
  globalStopRequested: false,
  unattendedExecutionAllowed: false,
  quitGuard: {
    state: "notRequested",
    honestRemainingSecondsEstimate: null,
    reason: "No stop request is active.",
  },
  runs: [{
    runId: "run-001",
    serviceId: "discord",
    phase: "running",
    reviewedItemCount: 3,
    remainingItemCount: 2,
    stopRequested: false,
    mutationAllowed: false,
    lastOutcome: "prepared",
  }],
} as const;

describe("AutoScrub fleet status renderer contract", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("loads the fleet status command instead of the retired single-run command", async () => {
    mocks.invoke.mockResolvedValue(fleetStatus);
    await expect(loadAutoScrubRunFleetStatus()).resolves.toEqual(fleetStatus);
    expect(mocks.invoke).toHaveBeenCalledWith("get_autoscrub_run_fl");
    expect(runnerSource).not.toContain("get_autoscrub_run_status");
    expect(mainSource).toContain("loadAutoScrubRunFleetStatus");
  });

  it("strictly rejects optimistic or legacy run status payloads", () => {
    expect(parseAutoScrubFleetStatus(fleetStatus).runs[0].mutationAllowed).toBe(false);
    expect(() => parseAutoScrubFleetStatus({ ...fleetStatus, unattendedExecutionAllowed: true })).toThrow();
    expect(() => parseAutoScrubFleetStatus({ ...fleetStatus, runs: [] })).toThrow();
    expect(() => parseAutoScrubFleetStatus({ status: "running", runId: "legacy-single-run" })).toThrow();
  });

  it("projects stop state from the fleet quit-guard honest estimate", () => {
    const stopping = parseAutoScrubFleetStatus({
      ...fleetStatus,
      globalStopRequested: true,
      quitGuard: {
        state: "estimated",
        honestRemainingSecondsEstimate: 125,
        reason: "Two checked items still need visible app confirmation.",
      },
      runs: [{
        ...fleetStatus.runs[0],
        phase: "stopping",
        stopRequested: true,
      }],
    });
    const projection = projectAutoScrubFleetStatus(stopping);
    expect(projection.label).toBe("Stop requested");
    expect(projection.detail).toContain("about 3 minutes");
    expect(projection.stopAvailable).toBe(false);

    const unknown = projectAutoScrubFleetStatus(parseAutoScrubFleetStatus({
      ...stopping,
      quitGuard: {
        state: "unknown",
        honestRemainingSecondsEstimate: null,
        reason: "The app stopped answering before OSL could estimate shutdown.",
      },
    }));
    expect(unknown.detail).toContain("stop time unknown");
    expect(unknown.tone).toBe("warning");
  });

  it("projects missing native stop authority as refusal, never permission", () => {
    const refused = projectAutoScrubFleetStatus(parseAutoScrubFleetStatus({
      ...fleetStatus,
      globalStopRequested: true,
      quitGuard: {
        state: "refused",
        honestRemainingSecondsEstimate: null,
        reason: "No reviewed stop authority is bound for this account.",
      },
      runs: [{
        ...fleetStatus.runs[0],
        phase: "stopping",
        stopRequested: true,
      }],
    }));
    expect(refused.label).toBe("Stopped");
    expect(refused.detail).toContain("refused to continue");
    expect(refused.tone).toBe("blocked");
    expect(refused.stopAvailable).toBe(false);
  });
});
