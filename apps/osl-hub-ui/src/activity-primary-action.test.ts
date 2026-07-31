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

    const plan = activityPrimaryActionPlan(2);
    const markup = activityDestinationContent();

    expect(plan).toEqual({
      route: "activity",
      reviewTarget: "attention-review",
      label: "Review attention item",
      disabled: false,
    });
    expect(markup).toContain('data-activity-primary-action');
    expect(markup).toContain('data-route="activity"');
    expect(markup).toContain('data-review-target="attention-review"');
    expect(markup).toContain('id="activity-attention-review"');
  });
});
