import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedService } from "./services";

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
// the first test drives the state it renders from through
// `__oslHubUiTest.reset(...)`, and the second test only reads pure/static
// content (`connectionsDestinationContent`, `fixedIaRoutePreview`) whose
// assertions do not depend on the specific service list left behind -- which
// is exactly the state a fresh import would have seen.
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
  generatedLabel: "Ready",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [
    { id: "acct-1", label: "Local profile", displayHandle: "local", state: "demoLinked", provider: null },
  ],
};

describe("Connections destination", () => {
  it("renders Connections as the account and device destination", () => {
    const { __oslHubUiTest } = ui;
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

  it("exposes Connections IA route and direct destination markup", () => {
    const { connectionsDestinationContent, fixedIaRoutePreview } = ui;

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
