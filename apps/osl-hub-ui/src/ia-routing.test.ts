import { describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues } from "./state";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
}

describe("IA routing", () => {
  it("Map application routes to the fixed IA destinations", async () => {
    installGlobals();
    const { fixedIaRoutePreview, primarySidebarMarkup } = await import("./main");

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
