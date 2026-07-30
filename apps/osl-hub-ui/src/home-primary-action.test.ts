import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
}

describe("Home primary action", () => {
  it("Route Home primary action to the most important issue", async () => {
    installGlobals();
    const { homePrimaryActionPlan } = await import("./main");
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

    expect(homePrimaryActionPlan({ ...ready, coreReady: false, pendingFriendReviews: 2, connectedApps: 0 }).issue)
      .toBe("account-protection");
    expect(homePrimaryActionPlan({ ...ready, storageProtected: false, pendingFriendReviews: 2, connectedApps: 0 }).issue)
      .toBe("local-storage");
    expect(homePrimaryActionPlan({ ...ready, pendingFriendReviews: 2, connectedApps: 0 }).issue)
      .toBe("trusted-people-review");
    expect(homePrimaryActionPlan({ ...ready, connectedApps: 0 }).target)
      .toEqual({ kind: "route", route: "connections", settingsSection: null });
    expect(homePrimaryActionPlan({ ...ready, verifiedFriends: 0 }).issue)
      .toBe("trusted-person");
    expect(homePrimaryActionPlan({ ...ready, hasRecentActivity: true }).target)
      .toEqual({ kind: "route", route: "activity", settingsSection: null });
    expect(homePrimaryActionPlan(ready).target).toEqual({ kind: "home-module", module: "osl-chats" });
  });
});
