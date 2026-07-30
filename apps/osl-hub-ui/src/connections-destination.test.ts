import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedService } from "./services";

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

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

const discordService: LinkedService = {
  id: "discord",
  displayName: "Discord",
  sidebarGlyph: "DC",
  sidebarOrder: 0,
  category: "consumer",
  launchState: "available",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [
    { id: "acct-1", label: "Local profile", displayHandle: "local", state: "demoLinked", provider: null },
  ],
};

describe("Connections destination", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Implement Connections as the account and device destination", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "connections", services: [discordService], mullvadAvailability: "unavailable" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('class="content-viewport connections-destination"');
    expect(html).toContain("<h1 id=\"route-heading\" tabindex=\"-1\">Connections</h1>");
    expect(html).toContain('data-connection-app="discord"');
    expect(html).toContain("1 local profile");
    expect(html).toContain('data-connection-card="mullvad"');
    expect(html).toContain('data-android-surface="androidCompanion"');
    expect(html).toContain('data-android-surface="androidMobileWorkspace"');
  });

  it("Implements Connections as a fixed IA account and device destination preview", async () => {
    const { connectionsDestinationContent, fixedIaRoutePreview } = await loadUi();

    const route = fixedIaRoutePreview().find((target) => target.destination === "connections");
    const markup = connectionsDestinationContent();
    const copy = visibleText(markup);

    expect(route).toMatchObject({ route: "connections", settingsSection: null });
    expect(markup).toContain('class="content-viewport connections-destination"');
    expect(markup).toContain('settings-list connected-accounts connections-accounts');
    expect(markup).toContain('settings-list connected-devices connections-devices');
    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(markup).toContain('data-android-surface="androidMobileWorkspace"');
    expect(copy).toMatch(/accounts, local app windows, network tools, and planned device surfaces/iu);
    expect(copy).toMatch(/Each profile stays separate/iu);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });
});
