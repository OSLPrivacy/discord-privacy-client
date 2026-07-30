import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedService } from "./services";

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
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

  it("renders Connections as the account and device destination", async () => {
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

  it("exposes Connections IA route and direct destination markup", async () => {
    const { connectionsDestinationContent, fixedIaRoutePreview } = await loadUi();

    const route = fixedIaRoutePreview().find((target) => target.destination === "connections");
    const markup = connectionsDestinationContent();
    const copy = visibleText(markup);

    expect(route).toMatchObject({ route: "connections", settingsSection: null });
    expect(markup).toContain('class="settings-list connections-accounts connected-accounts"');
    expect(markup).toContain('class="settings-list connections-devices connected-devices"');
    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(copy).toMatch(/Accounts, devices, network status, and future workspaces/iu);
    expect(copy).toMatch(/Each profile stays separate/iu);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });
});
