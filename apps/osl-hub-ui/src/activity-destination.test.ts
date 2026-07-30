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

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Activity destination", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders Activity as the proof destination with recorded events", async () => {
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
    expect(html).toContain("cleanup verification, connection failures, and outcomes OSL can prove locally");
    expect(html).toContain('aria-label="Activity proof history"');
    expect(html).toContain('data-activity-proof="event-1"');
    expect(html).toContain("Connection failed");
  });

  it("exposes Activity IA route and avoids implementation-facing copy", async () => {
    const { activityDestinationContent, fixedIaRoutePreview } = await loadUi();

    const route = fixedIaRoutePreview().find((target) => target.destination === "activity");
    const markup = activityDestinationContent();
    const copy = visibleText(markup);

    expect(route).toMatchObject({ route: "activity", settingsSection: null });
    expect(markup).toContain('id="activity-attention-review"');
    expect(markup).toContain('data-review-target="attention-review"');
    expect(copy).toMatch(/Local proof of what OSL did/iu);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });
});
