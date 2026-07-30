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

describe("Privacy primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Route Privacy primary action to protection review", async () => {
    const { __oslHubUiTest, privacyPrimaryAction } = await loadUi();
    __oslHubUiTest.reset({ route: "privacy" });

    expect(__oslHubUiTest.renderWorkspaceContent("privacy")).not.toContain("data-privacy-protection-review");

    privacyPrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "privacy",
      privacyProtectionReviewOpen: true,
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("data-privacy-protection-review");
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("Review or change protection");
  });

  it("Builds the Privacy primary action plan for protection review", async () => {
    const { privacyDestinationContent, privacyPrimaryActionPlan } = await loadUi();

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
