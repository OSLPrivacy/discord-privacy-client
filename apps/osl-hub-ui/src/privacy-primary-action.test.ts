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

describe("Privacy primary action", () => {
  it("Route Privacy primary action to protection review", async () => {
    installGlobals();
    const { privacyDestinationContent, privacyPrimaryActionPlan } = await import("./main");

    const plan = privacyPrimaryActionPlan();
    const markup = privacyDestinationContent();

    expect(plan).toEqual({ route: "privacy", reviewTarget: "protection-review", label: "Review protection" });
    expect(markup).toContain('data-privacy-primary-action');
    expect(markup).toContain('data-route="privacy"');
    expect(markup).toContain('data-review-target="protection-review"');
    expect(markup).toContain('id="privacy-protection-review"');
    expect(markup).toContain("Global policy");
  });
});
