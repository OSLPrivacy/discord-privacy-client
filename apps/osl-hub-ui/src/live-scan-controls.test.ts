import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", async () => {
  const actual = await vi.importActual<typeof import("./preferences")>("./preferences");
  return { ...actual, isTauriRuntime: mocks.isTauriRuntime };
});

import {
  bindLiveScanControls,
  getLiveScanControlsState,
  handleLiveScanKeepScanning,
  handleLiveScanPause,
  handleLiveScanResume,
  handleLiveScanStopNow,
  handleLiveScanStopScrub,
  liveScanControlsMarkup,
  resetLiveScanControlsStateForTest,
} from "./live-scan-controls";

const fleetStatus = {
  contract: "autoscrubRunFleet.v1",
  openRunCount: 1,
  globalStopRequested: false,
  stopConfirmation: {
    required: false,
    keepScanningLabel: "Keep scanning",
    stopNowLabel: "Stop now",
  },
  unattendedExecutionAllowed: false,
  quitGuard: {
    state: "notRequested",
    honestRemainingSecondsEstimate: null,
    reason: "No stop request is active.",
  },
  fleetActions: [],
  runs: [
    {
      runId: "run-001",
      serviceId: "discord",
      accountId: "account-1",
      phase: "running",
      reviewedItemCount: 3,
      remainingItemCount: 1,
      paceMilliseconds: 1_000,
      stopRequested: false,
      mutationAllowed: false,
      lastOutcome: "none",
      accountActions: [],
    },
  ],
};

describe("live scan controls", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
    resetLiveScanControlsStateForTest();
  });

  it("renders all five buttons plus a state label", () => {
    const markup = liveScanControlsMarkup();
    expect(markup).toContain('id="live-scan-pause"');
    expect(markup).toContain('id="live-scan-resume"');
    expect(markup).toContain('id="live-scan-stop-scrub"');
    expect(markup).toContain('id="live-scan-keep-scanning"');
    expect(markup).toContain('id="live-scan-stop-now"');
    expect(markup).toContain("data-live-scan-label");
  });

  it("Pause calls the pause command and flips the displayed state without a native round trip", async () => {
    expect(getLiveScanControlsState().paused).toBe(false);
    await handleLiveScanPause();
    expect(getLiveScanControlsState().paused).toBe(true);
    expect(getLiveScanControlsState().label).toBe("Paused");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("Resume calls get_autoscrub_run_fl and clears the paused state", async () => {
    mocks.invoke.mockResolvedValueOnce(fleetStatus);
    await handleLiveScanPause();
    expect(getLiveScanControlsState().paused).toBe(true);
    await handleLiveScanResume();
    expect(mocks.invoke).toHaveBeenCalledWith("get_autoscrub_run_fl");
    expect(getLiveScanControlsState().paused).toBe(false);
    expect(getLiveScanControlsState().label).toBe("1 open run");
  });

  it("Stop Scrub calls request_autoscrub_global_stop and shows Confirm stop", async () => {
    mocks.invoke.mockResolvedValueOnce({
      ...fleetStatus,
      globalStopRequested: true,
      stopConfirmation: { ...fleetStatus.stopConfirmation, required: true },
    });
    await handleLiveScanStopScrub();
    expect(mocks.invoke).toHaveBeenCalledWith("request_autoscrub_global_stop");
    expect(getLiveScanControlsState().label).toBe("Confirm stop");
  });

  it("Keep scanning calls keep_scanning_after_autoscrub_stop_request and shows the live run count", async () => {
    mocks.invoke.mockResolvedValueOnce(fleetStatus);
    await handleLiveScanKeepScanning();
    expect(mocks.invoke).toHaveBeenCalledWith("keep_scanning_after_autoscrub_stop_request");
    expect(getLiveScanControlsState().label).toBe("1 open run");
  });

  it("Stop now calls stop_autoscrub_now_after_stop_request and shows the run is gone", async () => {
    mocks.invoke.mockResolvedValueOnce({ ...fleetStatus, openRunCount: 0, runs: [] });
    await handleLiveScanStopNow();
    expect(mocks.invoke).toHaveBeenCalledWith("stop_autoscrub_now_after_stop_request");
    expect(getLiveScanControlsState().label).toBe("Ready to review");
  });

  it("clicking a bound button calls its matching command", async () => {
    const handlers = new Map<string, () => void>();
    const fakeRoot = {
      querySelector: (selector: string) => {
        const id = selector.replace("#", "");
        return {
          addEventListener: (_type: string, handler: () => void) => handlers.set(id, handler),
        };
      },
    } as unknown as ParentNode;
    bindLiveScanControls(fakeRoot);
    mocks.invoke.mockResolvedValue(fleetStatus);

    handlers.get("live-scan-stop-scrub")?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.invoke).toHaveBeenCalledWith("request_autoscrub_global_stop");

    handlers.get("live-scan-pause")?.();
    await Promise.resolve();
    expect(getLiveScanControlsState().paused).toBe(true);
  });
});
