import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
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

beforeEach(() => {
  localStore.clear();
});

function sendModeButtons(markup: string): string[] {
  return [...markup.matchAll(/data-send-mode="([^"]+)"/gu)].map((match) => match[1]);
}

describe("onboarding send modes", () => {
  it("implements ordinary send choices without Single Enter in onboarding state", () => {
    const { __oslHubUiTest } = ui;
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

  it("exposes the direct send mode content without Single Enter", () => {
    const { sendingSetupContent } = ui;
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
