import { beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues, oslPrimaryDestinations, oslSettingsDestination } from "./state";

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

describe("fixed desktop IA sidebar", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("emits a stylesheet-owned navigation rail", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "home" });

    const sidebar = __oslHubUiTest.renderPrimarySidebar();
    expect(sidebar).toContain('class="primary-sidebar"');
    expect(sidebar).toContain('aria-label="OSL navigation"');
    expect(sidebar).not.toContain("<style");
    expect(sidebar).not.toContain('style="');
  });

  it("uses the fixed six primary destinations in model order", async () => {
    const { __oslHubUiTest } = await loadUi();
    const sidebar = __oslHubUiTest.renderPrimarySidebar();
    expect(sidebar).toContain('aria-label="Primary destinations"');
    const positions = oslPrimaryDestinationValues.map((destination) => sidebar.indexOf(`data-primary-destination="${destination}"`));
    expect(positions.every((position) => position >= 0)).toBe(true);
    expect(positions).toEqual([...positions].sort((left, right) => left - right));
    for (const destination of oslPrimaryDestinationValues) {
      expect(sidebar).toContain(`data-primary-destination="${destination}"`);
    }
    expect(oslPrimaryDestinations.map((destination) => destination.id)).toEqual(oslPrimaryDestinationValues);
  });

  it("keeps Settings fixed outside the primary destinations", async () => {
    const { __oslHubUiTest } = await loadUi();
    const sidebar = __oslHubUiTest.renderPrimarySidebar();
    expect(oslPrimaryDestinationValues).not.toContain(oslSettingsDestination);
    expect(sidebar).toContain('class="primary-sidebar-settings');
    expect(sidebar).toContain(`data-route="${oslSettingsDestination}"`);
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
  });

  it("does not expose implementation concepts as navigation copy", async () => {
    const { __oslHubUiTest } = await loadUi();
    const sidebar = __oslHubUiTest.renderPrimarySidebar();
    const visibleCopy = [
      ...oslPrimaryDestinations.flatMap((destination) => [destination.label, destination.userQuestion]),
      "Settings",
      "OSL",
    ].join("\n");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(sidebar).not.toMatch(/data-sidebar-move|data-sidebar-toggle|Move or hide apps/i);
  });
});
