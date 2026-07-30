import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Connections destination", () => {
  it("Implement Connections as the account and device destination", async () => {
    installGlobals();
    const { connectionsDestinationContent, fixedIaRoutePreview } = await import("./main");

    const route = fixedIaRoutePreview().find((target) => target.destination === "connections");
    const markup = connectionsDestinationContent();
    const copy = visibleText(markup);

    expect(route).toMatchObject({ route: "connections", settingsSection: null });
    expect(markup).toContain('class="content-viewport connections-destination"');
    expect(markup).toContain('class="settings-list connected-accounts"');
    expect(markup).toContain('class="settings-list connected-devices"');
    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(markup).toContain('data-android-surface="androidMobileWorkspace"');
    expect(copy).toMatch(/accounts, local app windows, network tools, and planned device surfaces/iu);
    expect(copy).toMatch(/Each profile stays separate/iu);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });
});
