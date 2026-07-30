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

describe("Activity destination", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Implement Activity as the proof destination", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      route: "activity",
      notificationsEnabled: true,
      appNotifications: [
        { id: "event-1", title: "Connection failed", detail: "Discord needs review", createdAt: "Now" },
      ],
    });

    const html = __oslHubUiTest.renderWorkspaceContent("activity");

    expect(html).toContain('class="content-viewport activity-destination"');
    expect(html).toContain("Warnings, scheduled work, cleanup verification, connection failures, and outcomes OSL can prove locally.");
    expect(html).toContain('aria-label="Activity proof history"');
    expect(html).toContain('data-activity-proof="event-1"');
    expect(html).toContain("Connection failed");
  });
});
