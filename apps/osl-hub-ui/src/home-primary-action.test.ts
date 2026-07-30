import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("Home primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("routes Home primary action to the most important stateful issue", async () => {
    const { __oslHubUiTest, homePrimaryAction } = await loadUi();
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

  it("plans Home primary action priority without rendering", async () => {
    const { homePrimaryActionPlan } = await loadUi();
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
