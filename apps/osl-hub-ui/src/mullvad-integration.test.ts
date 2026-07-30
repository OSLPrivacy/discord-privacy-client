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

describe("Mullvad integration", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Render Mullvad card without VPN content privacy claims", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "connections", mullvadAvailability: "installed" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-connection-card="mullvad"');
    expect(html).toContain('data-privacy-scope="networkOnly"');
    expect(html).toContain('data-connection-state="installed"');
    expect(html).toContain("Network privacy signal only.");
    expect(html).toContain("Platforms and recipients can still see ordinary content you send there.");
    expect(html).not.toMatch(/OSL-built VPN|message content private from|recipient cannot see|platform cannot see/i);
  });

  it("Renders Mullvad helper without content privacy claims", async () => {
    const { mullvadConnectionCardMarkup } = await loadUi();

    const markup = mullvadConnectionCardMarkup({
      availability: "installed",
      integrationState: "availableToOpen",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    const copy = visibleText(markup);

    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(markup).toContain('data-privacy-scope="networkOnly"');
    expect(copy).toMatch(/Network privacy only/iu);
    expect(copy).toMatch(/separate network tool/iu);
    expect(copy).toMatch(/does not read its account state, connection state, or app content/iu);
    expect(copy).not.toMatch(/message content private|private messages|end-to-end encrypted|anonymous browsing|hides your content|OSL protects Mullvad content/iu);
  });
});
