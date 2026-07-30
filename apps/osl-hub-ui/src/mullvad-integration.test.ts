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
});
