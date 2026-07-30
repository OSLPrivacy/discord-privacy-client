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

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Public Post Guard", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders encrypted-audience carrier preview for public platforms in Privacy", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "privacy" });

    const html = __oslHubUiTest.renderWorkspaceContent("privacy");

    expect(html).toContain('data-public-platform-preview="encrypted-audience-carrier"');
    expect(html).toContain('data-public-post-kind="ordinary"');
    expect(html).toContain('data-public-post-kind="encrypted-audience-carrier"');
    expect(html).toContain("Search, quoting, archiving, audience, location, and media metadata still need review.");
    expect(html).toContain("the platform can still see the public carrier, timing, and engagement");
  });

  it("exposes public-post guard carrier parts without protected-public claims", async () => {
    const { publicPostGuardCarrierPreviewMarkup } = await loadUi();
    const markup = publicPostGuardCarrierPreviewMarkup("X");
    const copy = visibleText(markup);

    expect(markup).toContain('data-public-post-guard="encrypted-audience-carrier"');
    expect(markup).toContain('data-carrier-part="public"');
    expect(markup).toContain('data-carrier-part="protected-audience"');
    expect(copy).toMatch(/Encrypted-audience carrier preview/iu);
    expect(copy).toMatch(/public carrier text separately from the protected audience preview/iu);
    expect(copy).toMatch(/If audience proof is missing or changes, OSL refuses/iu);
    expect(copy).not.toMatch(/public .*end-to-end encrypted|ordinary external .*encrypted|available to everyone|global feed ready/iu);
  });
});
