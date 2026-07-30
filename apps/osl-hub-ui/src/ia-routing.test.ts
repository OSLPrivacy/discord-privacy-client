import { beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues } from "./state";

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

describe("IA routing", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Map application routes to the fixed IA destinations", async () => {
    const { __oslHubUiTest } = await loadUi();
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
      const sidebar = __oslHubUiTest.renderPrimarySidebar();
      const content = __oslHubUiTest.renderWorkspaceContent(route);

      expect(sidebar).toContain(`data-primary-destination="${route}"`);
      expect(sidebar).toContain(`data-route="${route}"`);
      expect(content).toContain(heading);
    }
  });

  it("Maps fixed IA route previews to destination routes", async () => {
    const { fixedIaRoutePreview, primarySidebarMarkup } = await loadUi();

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

    const sidebar = primarySidebarMarkup();
    for (const target of routes) {
      expect(sidebar).toContain(`data-primary-destination="${target.destination}"`);
      expect(sidebar).toContain(`data-route="${target.route}"`);
    }
  });
});
