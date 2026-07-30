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

describe("Activity primary action", () => {
  it("Route Activity primary action to attention review", async () => {
    installGlobals();
    const { activityDestinationContent, activityPrimaryActionPlan } = await import("./main");

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
