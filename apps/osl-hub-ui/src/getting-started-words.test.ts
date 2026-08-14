import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { renderScrubRoute } from "./scrub-route";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

function pageText(markup: string): string {
  return markup
    .replace(/<script[\s\S]*?<\/script>/giu, " ")
    .replace(/<style[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]+>/gu, " ")
    .replace(/&nbsp;/gu, " ")
    .replace(/&amp;/gu, "&")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&quot;/gu, "\"")
    .replace(/&#39;/gu, "'")
    .replace(/\s+/gu, " ")
    .trim();
}

describe("getting-started page text", () => {
  it("covers setup, choosing Scrub accounts, separate consent, and Pro code entry", () => {
    ui.__oslHubUiTest.reset({ coreReady: true });

    const text = [
      ui.__oslHubUiTest.renderOnboardingRoute("pro"),
      ui.__oslHubUiTest.renderOnboardingRoute("browser"),
      ui.__oslHubUiTest.renderSetupAppsForTest(),
      renderScrubRoute({
        accounts: [{ id: "local-export", label: "Local message export", detail: "TXT, CSV, or JSON on this device" }],
        selectedAccountIds: [],
        selectedCategories: [],
        scan: { state: "not-started", findings: 0 },
      }),
    ].map(pageText).join(" ");

    expect(text).toContain("Nothing opens during setup.");
    expect(text).toContain("Choose one or more accounts and categories.");
    expect(text).toContain("Consent separately to each browser area OSL may inspect.");
    expect(text).toContain("Enter Pro code");
  });
});
