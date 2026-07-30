import { describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues, oslSettingsDestination } from "./state";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
}

describe("IA Settings placement", () => {
  it("Pin Settings below the six IA destinations", async () => {
    installGlobals();
    const { fixedIaSidebarOrderPreview, primarySidebarMarkup } = await import("./main");

    const order = fixedIaSidebarOrderPreview();
    expect(order).toEqual([...oslPrimaryDestinationValues, oslSettingsDestination]);
    expect(order.indexOf(oslSettingsDestination)).toBe(6);
    expect(order.slice(0, 6)).not.toContain(oslSettingsDestination);

    const sidebar = primarySidebarMarkup();
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
    expect(sidebar).toContain(`data-route="${oslSettingsDestination}"`);
  });
});
