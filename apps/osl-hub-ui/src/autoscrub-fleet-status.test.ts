import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
  emitTo: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  window: {
    isFullscreen: vi.fn(() => Promise.resolve(false)),
    setFullscreen: vi.fn(() => Promise.resolve()),
    onResized: vi.fn(() => Promise.resolve(() => undefined)),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  providerLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  serviceLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
}));

import { loadAutoScrubRunFleetStatus } from "./autoscrub-unattended-run";
import { parseAutoScrubFleetStatus, projectAutoScrubFleetStatus } from "./autoscrub-contract";

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
  fleetActions: [{
    action: "stopAllScanning",
    label: "Stop all scanning",
  }],
  runs: [{
    runId: "run-001",
    serviceId: "discord",
    accountId: "acct-discord-1",
    phase: "running",
    reviewedItemCount: 3,
    remainingItemCount: 2,
    stopRequested: false,
    mutationAllowed: false,
    lastOutcome: "prepared",
    accountActions: [
      { action: "openAccount", label: "Open account" },
      { action: "skipThisAccount", label: "Skip this account" },
    ],
  }],
} as const;

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of the one `it()` that needs the UI, where
// vitest's default 5,000 ms `testTimeout` applies, so a busy machine could turn
// the file red with `Test timed out in 5000ms` -- without ever reaching an
// assertion. The module is now loaded ONCE, in a hook that carries its own
// budget, and the test reads it synchronously. That is safe here and was
// checked, not assumed: the test drives the state it renders from through
// `__oslHubUiTest.reset(...)`, and the stubbed `localStorage` is emptied before
// each test -- which is exactly the state a fresh import would have seen. The
// old `beforeEach(() => vi.unstubAllGlobals())` is now an `afterAll`; its other
// per-test mock resets are unrelated to the hoisted globals and stay as they were.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("AutoScrub fleet status renderer contract", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("loads and returns the native fleet status", async () => {
    mocks.invoke.mockResolvedValue(fleetStatus);
    await expect(loadAutoScrubRunFleetStatus()).resolves.toEqual(fleetStatus);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("get_autoscrub_run_fl");
  });

  it("renders a loaded fleet status in the AutoScrub controls", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "settings", autoScrubFleetStatus: fleetStatus });

    const markup = __oslHubUiTest.renderSettingsSection("scrub");

    expect(markup).toContain("autoscrub-status-working");
    expect(markup).toContain('aria-disabled="false"');
    expect(markup).toContain('id="autoscrub-stop"');
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
