import { beforeEach, describe, expect, it, vi } from "vitest";

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

describe("Home primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Route Home primary action to the most important issue", async () => {
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
});
