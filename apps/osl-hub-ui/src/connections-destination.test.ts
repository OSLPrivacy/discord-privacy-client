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
});
