import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { NativeBrowserImportReceipt } from "./services";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("./preferences", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./preferences")>()),
  isTauriRuntime: () => true,
}));

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

function installGlobals(): void {
  const body = { append: vi.fn() };
  vi.stubGlobal("localStorage", new MemoryStorage());
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    createElement: vi.fn(() => ({
      classList: { add: vi.fn() },
      addEventListener: vi.fn(),
      remove: vi.fn(),
    })),
    body,
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
}

const chromeReceipt: NativeBrowserImportReceipt = {
  browserId: "chrome",
  profile: "Default",
  account: "history-footprint",
  scope: "history-footprint",
  runId: "run-chrome-default",
  observationCount: 1,
  snapshotDeleted: true,
};

const firefoxReceipt: NativeBrowserImportReceipt = {
  browserId: "firefox",
  profile: "work",
  account: "history-footprint",
  scope: "history-footprint",
  runId: "run-firefox-work",
  observationCount: 1,
  snapshotDeleted: true,
};

let ui: typeof import("./main");

beforeAll(async () => {
  installGlobals();
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  (localStorage as MemoryStorage).clear();
  mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (command === "grant_browser_profile_consent") {
      const request = args?.request as { browserId: string; profile: string };
      return Promise.resolve({
        browserId: request.browserId,
        profile: request.profile,
        grantId: "a".repeat(64),
        expiresAtUnixMs: Date.now() + 30_000,
      });
    }
    if (command === "scan_consented_browser_profile") {
      const request = args?.request as { browserId: string; profile: string };
      return Promise.resolve(request.browserId === "firefox" ? firefoxReceipt : chromeReceipt);
    }
    if (command === "load_detected_browser_footprint") {
      const consents = args?.consents as Array<{
        browserId: "chrome" | "firefox";
        browserProfileAccount: string;
        browserProfileId: string;
        importRunId: string;
      }>;
      return Promise.resolve(consents.map((consent, index) => ({
        browserId: consent.browserId,
        browserProfileAccount: consent.browserProfileAccount,
        browserProfileId: consent.browserProfileId,
        importRunId: consent.importRunId,
        observedAtUnixMs: index + 1,
      })));
    }
    if (command === "revoke_detected_browser_footprint"
      || command === "finish_protected_browser_import") {
      return Promise.resolve(undefined);
    }
    if (command === "list_native_apps") return Promise.resolve([]);
    throw new Error(`unexpected command ${command}`);
  });
  ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "browser", coreReady: true });
  ui.__oslHubUiTest.setBrowserProfilesForTest([
    { browserId: "chrome", profile: "Default", displayName: "Personal" },
    { browserId: "firefox", profile: "work", displayName: "Work" },
  ]);
});

describe("TASK0347 browser account finder controls", () => {
  it("records Area tick, Check selected, Delete area, Not now, Back, and unselected refusal", async () => {
    const initial = ui.__oslHubUiTest.browserAccountFinderSnapshotForTest();
    expect(initial.areaNames).toEqual(["Chrome · Personal", "Firefox · Work"]);
    expect(initial.accountCount).toBe(0);
    console.log(`TASK0347 initial area_count=${initial.areaNames.length} account_count=${initial.accountCount} areas="${initial.areaNames.join("|")}"`);

    const tick = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("areaTick", {
      areaKey: "chrome:Default",
      checked: true,
    });
    expect(tick.accepted).toBe(true);
    expect(tick.selectedAreaNames).toEqual(["Chrome · Personal"]);
    console.log(`TASK0347 Area tick accepted=${tick.accepted} selected="${tick.selectedAreaNames.join("|")}" unselected="Firefox · Work"`);

    const checked = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("checkSelected");
    expect(checked.accepted).toBe(true);
    expect(checked.recordedAccounts).toEqual(["Chrome · Personal=history-footprint"]);
    expect(checked.route).toBe("onboarding");
    expect(checked.onboardingRoute).toBe("detected");
    console.log(`TASK0347 Check selected accepted=${checked.accepted} accounts="${checked.recordedAccounts.join("|")}" account_count=${checked.accountCount} route=${checked.onboardingRoute}`);

    ui.__oslHubUiTest.seedBrowserFootprintsForTest([chromeReceipt, firefoxReceipt]);
    const deleteArea = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("deleteArea", {
      areaKey: "chrome:Default",
    });
    expect(deleteArea.accepted).toBe(true);
    expect(deleteArea.recordedAreaNames).toEqual(["Firefox · Work"]);
    expect(deleteArea.accountCount).toBe(1);
    console.log(`TASK0347 Delete area accepted=${deleteArea.accepted} remaining_recorded_areas="${deleteArea.recordedAreaNames.join("|")}" account_count=${deleteArea.accountCount}`);

    const notNow = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("notNow");
    expect(notNow.accepted).toBe(true);
    expect(notNow.onboardingRoute).toBe("detected");
    expect(notNow.recordedAreaNames).toEqual(["Firefox · Work"]);
    console.log(`TASK0347 Not now accepted=${notNow.accepted} route=${notNow.onboardingRoute} remaining_recorded_areas="${notNow.recordedAreaNames.join("|")}" account_count=${notNow.accountCount}`);

    const beforeRefusal = ui.__oslHubUiTest.browserAccountFinderSnapshotForTest();
    const refused = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("checkSelected");
    expect(refused.accepted).toBe(false);
    expect(refused.accountCount).toBe(beforeRefusal.accountCount);
    expect(refused.recordedAreaNames).toEqual(beforeRefusal.recordedAreaNames);
    console.log(`TASK0347 unselected Check selected accepted=${refused.accepted} account_count_before=${beforeRefusal.accountCount} account_count_after=${refused.accountCount} remaining_before="${beforeRefusal.recordedAreaNames.join("|")}" remaining_after="${refused.recordedAreaNames.join("|")}"`);

    const back = await ui.__oslHubUiTest.callBrowserAccountFinderControlForTest("back");
    expect(back.accepted).toBe(true);
    expect(back.onboardingRoute).toBe("mullvad");
    console.log(`TASK0347 Back accepted=${back.accepted} route=${back.onboardingRoute}`);
  });
});
