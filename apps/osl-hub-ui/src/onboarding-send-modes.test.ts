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

describe("onboarding send modes", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Implement ordinary send choices without Single Enter", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset();

    const html = __oslHubUiTest.renderOnboardingSendModes("single");

    expect(html).toContain('data-send-mode="manual"');
    expect(html).toContain('data-send-mode="clipboard"');
    expect(html).toContain('data-send-mode="double"');
    expect(html).not.toContain('data-send-mode="single"');
    expect(html).not.toContain("Single Enter");
    expect(html).toContain("No mode silently sends.");
    expect(html).toContain("If OSL cannot prove the destination, it copies the encrypted text and sends nothing.");
  });
});
