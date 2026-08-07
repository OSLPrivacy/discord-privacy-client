import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

const localStore = new Map<string, string>();
let ui: typeof import("./main");

function renderedText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("Scrub threat-model page copy", () => {
  it("states the four deletion limits and user responsibility", () => {
    ui.__oslHubUiTest.reset({ route: "settings" });
    const text = renderedText(ui.__oslHubUiTest.renderSettingsSection("scrub"));
    const required = [
      "Scrub cannot undo copies",
      "screenshots",
      "service records",
      "guarantee service permission",
      "You are responsible. Check the original app and delete each message yourself.",
    ];

    for (const phrase of required) {
      expect(text).toContain(phrase);
      console.info(`FOUND: ${phrase}`);
    }
  });
});
