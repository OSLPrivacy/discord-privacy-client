import { beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues, oslSettingsDestination } from "./state";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("IA settings placement", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("pins Settings below the six rendered IA destinations", async () => {
    const { __oslHubUiTest } = await loadUi();
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

  it("exposes Settings after the fixed IA order", async () => {
    const { fixedIaSidebarOrderPreview, primarySidebarMarkup } = await loadUi();

    const order = fixedIaSidebarOrderPreview();
    expect(order).toEqual([...oslPrimaryDestinationValues, oslSettingsDestination]);
    expect(order.indexOf(oslSettingsDestination)).toBe(6);
    expect(order.slice(0, 6)).not.toContain(oslSettingsDestination);

    const sidebar = primarySidebarMarkup();
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
    expect(sidebar).toContain(`data-route="${oslSettingsDestination}"`);
  });
});
