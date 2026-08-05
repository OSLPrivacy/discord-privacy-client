import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// the first test drives the state it renders from through
// `__oslHubUiTest.reset(...)`, and the second test only calls the pure
// `homePrimaryActionPlan` function, which reads no module state -- and the
// stubbed `localStorage` is emptied before each test -- which is exactly the
// state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
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

describe("Home primary action", () => {
  it("routes Home primary action to the most important stateful issue", () => {
    const { __oslHubUiTest, homePrimaryAction } = ui;
    __oslHubUiTest.reset({
      coreReady: true,
      storageMethod: "tpm-pcp",
      hubPeople: [
        { personId: "pending", alias: "Pending friend", safetyNumberVerified: false },
        { personId: "verified", alias: "Verified friend", safetyNumberVerified: true },
      ],
      notificationsEnabled: true,
      appNotifications: [
        { id: "activity-1", title: "Later activity", detail: "Activity waits behind trust review", createdAt: "Now" },
      ],
    });

    expect(__oslHubUiTest.snapshot().homePrimaryIssue).toBe("trusted-people-review");

    homePrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "people",
      homePrimaryIssue: "trusted-people-review",
    });
  });

  it("plans Home primary action priority without rendering", () => {
    const { homePrimaryActionPlan } = ui;
    const ready = {
      coreReady: true,
      storageProtected: true,
      storageDetail: "Hardware protected",
      coreDetail: "Ready",
      pendingFriendReviews: 0,
      connectedApps: 1,
      verifiedFriends: 1,
      hasRecentActivity: false,
    };

    expect(homePrimaryActionPlan({ ...ready, coreReady: false, pendingFriendReviews: 2, connectedApps: 0 }).issue).toBe("account-protection");
    expect(homePrimaryActionPlan({ ...ready, storageProtected: false, pendingFriendReviews: 2, connectedApps: 0 }).issue).toBe("local-storage");
    expect(homePrimaryActionPlan({ ...ready, pendingFriendReviews: 2, connectedApps: 0 }).issue).toBe("trusted-people-review");
    expect(homePrimaryActionPlan({ ...ready, connectedApps: 0 }).target).toEqual({ kind: "route", route: "connections", settingsSection: null });
    expect(homePrimaryActionPlan({ ...ready, verifiedFriends: 0 }).issue).toBe("add-trusted-person");
    expect(homePrimaryActionPlan({ ...ready, hasRecentActivity: true }).target).toEqual({ kind: "route", route: "activity", settingsSection: null });
    expect(homePrimaryActionPlan(ready).target).toEqual({ kind: "home-module", module: "osl-chats" });
  });
});
