import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues } from "./state";

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

describe("IA routing", () => {
  it("maps application routes to destination content without a rendered rail", () => {
    const { __oslHubUiTest } = ui;
    const expectations = [
      ["home", "Home"],
      ["inbox", "Conversations"],
      ["people", "People"],
      ["privacy", "Privacy"],
      ["activity", "Activity"],
      ["connections", "Connections"],
    ] as const;

    expect(oslPrimaryDestinationValues).toEqual(expectations.map(([route]) => route));

    for (const [route, heading] of expectations) {
      __oslHubUiTest.reset({ route });
      const content = __oslHubUiTest.renderWorkspaceContent(route);
      const shell = __oslHubUiTest.renderRouteShell(route);

      expect(content).toContain(heading);
      expect(shell).toContain("data-shared-launcher-header");
      expect(shell).not.toMatch(/primary-sidebar|data-primary-destination|with-primary-sidebar/u);
    }
  });

  it("exposes fixed IA route preview helpers", () => {
    const { fixedIaRoutePreview } = ui;

    const routes = fixedIaRoutePreview();
    expect(routes.map((target) => target.destination)).toEqual(oslPrimaryDestinationValues);
    expect(routes.map((target) => target.route)).toEqual([
      "home",
      "inbox",
      "people",
      "privacy",
      "activity",
      "connections",
    ]);
    expect(routes.every((target) => target.settingsSection === null)).toBe(true);

  });
});
