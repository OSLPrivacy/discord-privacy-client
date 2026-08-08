import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { vi } from "vitest";
import type { AutoScrubFleetStatus } from "./autoscrub-contract";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251 pattern: main.ts is large and slow to import, so it is loaded once in
// a hook with its own budget rather than inside every `it()`.
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

function cardCount(markup: string): number {
  return markup.match(/data-home-scrub-run-card=/gu)?.length ?? 0;
}

function fleetWithRuns(runs: AutoScrubFleetStatus["runs"]): AutoScrubFleetStatus {
  return {
    contract: "autoscrubRunFleet.v1",
    openRunCount: runs.filter((run) => ["reviewRequired", "running", "stopping", "blocked"].includes(run.phase)).length,
    globalStopRequested: false,
    stopConfirmation: { required: false, keepScanningLabel: "Keep scanning", stopNowLabel: "Stop now" },
    unattendedExecutionAllowed: false,
    quitGuard: { state: "notRequested", honestRemainingSecondsEstimate: null, reason: "No stop request is active." },
    fleetActions: [{ action: "stopAllScanning", label: "Stop all scanning" }],
    runs,
  } satisfies AutoScrubFleetStatus;
}

const runningDiscordRun = {
  runId: "discord-background-run",
  serviceId: "discord",
  accountId: "acct-discord-1",
  phase: "running",
  reviewedItemCount: 3,
  remainingItemCount: 5,
  paceMilliseconds: 500,
  stopRequested: false,
  mutationAllowed: false,
  lastOutcome: "prepared",
  accountActions: [{ action: "openAccount", label: "Open account" }],
} satisfies AutoScrubFleetStatus["runs"][number];

describe("TASK1428 Home run card", () => {
  it("shows no card on Home when there is no fleet status at all", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "home", autoScrubFleetStatus: null });
    const markup = __oslHubUiTest.renderWorkspaceContent("home");
    expect(cardCount(markup)).toBe(0);
  });

  it("starting a background run adds exactly one card, and finishing removes it, leaving zero", () => {
    const { __oslHubUiTest } = ui;

    __oslHubUiTest.reset({ route: "home", autoScrubFleetStatus: null });
    const before = __oslHubUiTest.renderWorkspaceContent("home");
    expect(cardCount(before)).toBe(0);

    __oslHubUiTest.reset({ route: "home", autoScrubFleetStatus: fleetWithRuns([runningDiscordRun]) });
    const running = __oslHubUiTest.renderWorkspaceContent("home");
    expect(cardCount(running)).toBe(1);
    expect(running).toContain('data-home-scrub-run-card="discord"');
    expect(running).toContain('data-scrub-run-phase="running"');
    expect(running).toContain("Discord Scrub running");

    const finished = { ...runningDiscordRun, phase: "complete" } satisfies AutoScrubFleetStatus["runs"][number];
    __oslHubUiTest.reset({ route: "home", autoScrubFleetStatus: fleetWithRuns([finished]) });
    const after = __oslHubUiTest.renderWorkspaceContent("home");
    expect(cardCount(after)).toBe(0);

    console.log(`TASK1428 before=${cardCount(before)} running=${cardCount(running)} after=${cardCount(after)}`);
  });

  it("shows one card per open run and none for closed runs", () => {
    const { __oslHubUiTest } = ui;
    const signalRun = { ...runningDiscordRun, runId: "signal-background-run", serviceId: "signal", phase: "reviewRequired" } satisfies AutoScrubFleetStatus["runs"][number];
    const failedRun = { ...runningDiscordRun, runId: "telegram-failed-run", serviceId: "telegram", phase: "failed" } satisfies AutoScrubFleetStatus["runs"][number];
    __oslHubUiTest.reset({ route: "home", autoScrubFleetStatus: fleetWithRuns([runningDiscordRun, signalRun, failedRun]) });
    const markup = __oslHubUiTest.renderWorkspaceContent("home");
    expect(cardCount(markup)).toBe(2);
    expect(markup).toContain('data-home-scrub-run-card="discord"');
    expect(markup).toContain('data-home-scrub-run-card="signal"');
    expect(markup).not.toContain('data-home-scrub-run-card="telegram"');
  });
});
