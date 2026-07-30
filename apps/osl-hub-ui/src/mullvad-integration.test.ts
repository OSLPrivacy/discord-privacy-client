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

describe("Mullvad integration", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders Mullvad card without VPN content privacy claims", async () => {
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

  it("exposes Mullvad card markup as a network-only helper", async () => {
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
    expect(copy).toMatch(/Network privacy signal only/iu);
    expect(copy).toMatch(/separate network tool/iu);
    expect(copy).toMatch(/does not read its account state, connection state, or app content/iu);
    expect(copy).not.toMatch(/message content private|private messages|end-to-end encrypted|anonymous browsing|hides your content|OSL protects Mullvad content/iu);
  });
});
