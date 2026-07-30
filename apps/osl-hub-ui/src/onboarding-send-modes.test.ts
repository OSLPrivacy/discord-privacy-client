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

function sendModeButtons(markup: string): string[] {
  return [...markup.matchAll(/data-send-mode="([^"]+)"/gu)].map((match) => match[1]);
}

describe("onboarding send modes", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("implements ordinary send choices without Single Enter in onboarding state", async () => {
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

  it("exposes the direct send mode content without Single Enter", async () => {
    const { sendingSetupContent } = await loadUi();
    const markup = sendingSetupContent();

    expect(sendModeButtons(markup)).toEqual(["manual", "clipboard", "double"]);
    expect(markup).toContain("Manual");
    expect(markup).toContain("Clipboard");
    expect(markup).toContain("Double Enter");
    expect(markup).not.toContain('data-send-mode="single"');
    expect(markup).not.toMatch(/Single Enter/iu);
    expect(markup).toMatch(/No mode silently sends/iu);
  });
});
