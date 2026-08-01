import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AutoScrubFleetStatus } from "./autoscrub-contract";

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  invoke: vi.fn(),
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
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  providerLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  serviceLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
}));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

const openFleet = {
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
      runId: "discord-reviewed-run",
      serviceId: "discord",
      phase: "running",
      reviewedItemCount: 4,
      remainingItemCount: 2,
      stopRequested: false,
      mutationAllowed: false,
      lastOutcome: "prepared",
    },
    {
      runId: "signal-reviewed-run",
      serviceId: "signal",
      phase: "reviewRequired",
      reviewedItemCount: 1,
      remainingItemCount: 1,
      stopRequested: false,
      mutationAllowed: false,
      lastOutcome: "none",
    },
  ],
} satisfies AutoScrubFleetStatus;

describe("fleet indicator", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("is absent from every route shell while there is no cleanup state to monitor", async () => {
    // The pill is a live monitor. With no fleet status at all -- a fresh
    // install, and the permanent resting state of a build without AutoScrub --
    // it projected to "Unavailable in this build / No cleanup running" and sat
    // in the titlebar beside the window controls from the very first frame,
    // making a feature's absence the loudest element on screen before the owner
    // had done anything. Nothing to monitor, so nothing to show.
    const { __oslHubUiTest } = await loadUi();
    for (const route of ["onboarding", "home", "settings"] as const) {
      __oslHubUiTest.reset({ route, autoScrubFleetStatus: null });
      const shell = __oslHubUiTest.renderRouteShell(route);
      expect(shell, route).not.toContain("data-fleet-indicator");
      expect(shell, route).not.toContain("Unavailable in this build");
      expect(shell, route).not.toContain("No cleanup running");
      // the window controls are still there -- the row itself did not go away
      expect(shell, route).toContain('id="window-close"');
    }
  });

  it("is reachable from every route shell and names every open cleanup run", async () => {
    const { __oslHubUiTest } = await loadUi();
    const routes = [
      "onboarding",
      "home",
      "inbox",
      "people",
      "privacy",
      "activity",
      "connections",
      "settings",
      "mullvad",
      "osl-chat",
      "osl-servers",
      "service",
    ] as const;

    for (const route of routes) {
      __oslHubUiTest.reset({ route, autoScrubFleetStatus: openFleet });
      const shell = __oslHubUiTest.renderRouteShell(route);

      expect(shell.match(/data-fleet-indicator/gu)?.length ?? 0, route).toBe(1);
      expect(shell, route).toContain('role="status"');
      expect(shell, route).toContain('data-open-run-count="2"');
      expect(shell, route).toContain('data-open-run-names="Discord, Signal"');
      expect(shell, route).toContain("2 open runs");
      expect(shell, route).toContain("Discord, Signal");
      expect(shell, route).not.toContain("discord-reviewed-run");
      expect(shell, route).not.toContain("signal-reviewed-run");
    }
  });
});
