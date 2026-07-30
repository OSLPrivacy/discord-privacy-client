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

describe("Activity primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("routes Activity primary action to attention review state", async () => {
    const { __oslHubUiTest, activityPrimaryAction } = await loadUi();
    __oslHubUiTest.reset({
      route: "activity",
      notificationsEnabled: true,
      appNotifications: [
        { id: "attention-1", title: "Friend verification changed", detail: "Review before protected sends", createdAt: "Now" },
      ],
    });

    expect(__oslHubUiTest.renderWorkspaceContent("activity")).not.toContain("data-activity-attention-review");

    activityPrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "activity",
      activityAttentionReviewOpen: true,
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain('data-activity-attention-review="attention-1"');
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("Attention review");
  });

  it("returns Activity primary action plan metadata", async () => {
    const { activityDestinationContent, activityPrimaryActionPlan } = await loadUi();

    expect(activityPrimaryActionPlan(2)).toEqual({
      route: "activity",
      reviewTarget: "attention-review",
      label: "Review attention item",
      disabled: false,
    });
    expect(activityDestinationContent()).toContain('data-activity-primary-action');
    expect(activityDestinationContent()).toContain('data-route="activity"');
    expect(activityDestinationContent()).toContain('data-review-target="attention-review"');
  });
});
