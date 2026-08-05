import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues, oslSettingsDestination } from "./state";

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

describe("IA settings placement", () => {
  it("pins Settings below the six rendered IA destinations", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "home" });

    const sidebar = __oslHubUiTest.renderPrimarySidebar();
    const positions = oslPrimaryDestinationValues.map((destination) => sidebar.indexOf(`data-primary-destination="${destination}"`));
    const settingsPosition = sidebar.indexOf('class="primary-sidebar-settings');

    expect(positions).toHaveLength(6);
    expect(positions.every((position) => position >= 0)).toBe(true);
    expect(new Set(positions).size).toBe(6);
    expect(settingsPosition).toBeGreaterThan(Math.max(...positions));
    expect(sidebar).toContain('data-route="settings"');
    expect(sidebar).not.toContain('data-primary-destination="settings"');
  });

  it("exposes Settings after the fixed IA order", () => {
    const { fixedIaSidebarOrderPreview, primarySidebarMarkup } = ui;

    const order = fixedIaSidebarOrderPreview();
    expect(order).toEqual([...oslPrimaryDestinationValues, oslSettingsDestination]);
    expect(order.indexOf(oslSettingsDestination)).toBe(6);
    expect(order.slice(0, 6)).not.toContain(oslSettingsDestination);

    const sidebar = primarySidebarMarkup();
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
    expect(sidebar).toContain(`data-route="${oslSettingsDestination}"`);
  });
});
