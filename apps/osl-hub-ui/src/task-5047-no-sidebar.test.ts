import { readFileSync } from "node:fs";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { Route } from "./main";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const workspaceRoutes = [
  "home",
  "inbox",
  "people",
  "privacy",
  "activity",
  "connections",
  "service",
  "settings",
  "mullvad",
  "osl-chat",
  "osl-mail",
  "osl-servers",
  "signal-qa",
] as const satisfies readonly Exclude<Route, "onboarding">[];

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  mocks.invoke.mockResolvedValue(undefined);
  mocks.listen.mockResolvedValue(() => undefined);
  mocks.getCurrentWindow.mockReturnValue({ onFocusChanged: vi.fn().mockResolvedValue(() => undefined) });
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

afterAll(() => vi.unstubAllGlobals());

describe("TASK 5047 route-wide sidebar removal", () => {
  it("renders all 13 workspace routes with one shared header and zero rail items", () => {
    let sharedHeaders = 0;
    let railItems = 0;
    for (const route of workspaceRoutes) {
      ui.__oslHubUiTest.reset({ route, coreReady: true, storageMethod: "tpm-pcp", services: [], servicesChecked: true });
      const shell = ui.__oslHubUiTest.renderRouteShell(route);
      const routeHeaders = shell.match(/data-shared-launcher-header/gu) ?? [];
      const routeRailItems = shell.match(/data-primary-destination/gu) ?? [];
      sharedHeaders += routeHeaders.length;
      railItems += routeRailItems.length;
      expect(routeHeaders, `${route} shared header`).toHaveLength(1);
      expect(routeRailItems, `${route} rail items`).toHaveLength(0);
      expect(shell, `${route} retired rail markup`).not.toMatch(/primary-sidebar|with-primary-sidebar|aria-label="Primary destinations"/u);
      expect(shell, `${route} full-width shell`).toContain('<div class="hub-layout"><section class="hub-workspace">');
    }
    console.log(`TASK5047 routes=${workspaceRoutes.length} shared_headers=${sharedHeaders} rail_items=${railItems}`);
    expect(workspaceRoutes).toHaveLength(13);
    expect(sharedHeaders).toBe(13);
    expect(railItems).toBe(0);
  });

  it("sets the ordinary shared launcher header and native reserves to exactly 58px", () => {
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    const nativeHost = readFileSync(new URL("../../osl-hub/src/native_window_host.rs", import.meta.url), "utf8");
    const serviceHost = readFileSync(new URL("../../osl-hub/src/service_host.rs", import.meta.url), "utf8");
    expect(styles).toMatch(/:root\s*\{[^}]*--chrome-row-height:\s*58px;/su);
    expect(styles).toMatch(/\.desktop-top-row\.shared-launcher-header-row\s*\{[^}]*height:\s*var\(--chrome-row-height\)/su);
    expect(nativeHost).toContain("const TRUSTED_VERTICAL_RESERVE: i32 = 58;");
    expect(serviceHost).toContain("const TRUSTED_BAR_HEIGHT: u32 = 58;");
    console.log("TASK5047 header_height=58 native_window_reserve=58 service_host_reserve=58");
  });
});
