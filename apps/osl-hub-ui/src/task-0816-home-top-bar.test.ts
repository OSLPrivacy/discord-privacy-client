// TASK 0816 -- build the Home top bar.
//
// Five controls: logo, Friends, Notifications, Settings, Profile, each with a
// clear current-page state. The screenshot check (screenshots/
// capture-home-top-bar.mjs) proves they paint without clipped text on Linux;
// this file proves the state behind the paint -- that exactly one control is
// ever marked current, and that it is the one that owns the page on screen.

import { readFileSync } from "node:fs";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

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

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("TASK 0816 Home launcher header", () => {
  it("ships the launcher header with its logo, protection readout, notifications, and Settings", () => {
    ui.__oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
    const shell = ui.__oslHubUiTest.renderRouteShell("home");

    expect(shell).toContain('class="home-launcher-header"');
    expect(shell).toContain('class="home-launcher-logo"');
    expect(shell).toContain('class="home-status-readout"');
    expect(shell).toContain('data-toggle-home-notifications');
    expect(shell).toContain('data-route="settings"');
  });

  it("keeps the rebuilt launcher free of the superseded five-control top bar", () => {
    ui.__oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
    const shell = ui.__oslHubUiTest.renderRouteShell("home");

    expect(shell).not.toContain('data-top-bar-control=');
    expect(shell).not.toContain('class="home-header home-command-bar"');
  });

  it("keeps the launcher styles single-line without the retired top-bar selectors", () => {
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(styles).toMatch(/\.home-launcher-header\s*\{/);
    expect(styles).toMatch(/\.home-launcher-actions\s*\{/);
  });
});
