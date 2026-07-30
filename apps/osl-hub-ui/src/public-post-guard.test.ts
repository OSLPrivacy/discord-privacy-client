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

describe("Public Post Guard", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Render encrypted-audience carrier preview for public platforms", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "privacy" });

    const html = __oslHubUiTest.renderWorkspaceContent("privacy");

    expect(html).toContain('data-public-platform-preview="encrypted-audience-carrier"');
    expect(html).toContain('data-public-post-kind="ordinary"');
    expect(html).toContain('data-public-post-kind="encrypted-audience-carrier"');
    expect(html).toContain("Search, quoting, archiving, audience, location, and media metadata still need review.");
    expect(html).toContain("the platform can still see the public carrier, timing, and engagement");
  });
});
