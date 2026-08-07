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

interface TopBarControl {
  id: string;
  current: boolean;
  ariaCurrent: boolean;
  ariaLabel: string;
  label: string;
  classes: string;
}

/**
 * Every top-bar control in the bar, read out of the rendered markup.
 *
 * These specs run in vitest's node environment (there is no DOM here), so the
 * bar is parsed rather than queried. The slice is bounded to the command bar's
 * own <header> so nothing further down the shell can be mistaken for a control.
 */
function topBarControls(shell: string): TopBarControl[] {
  const barStart = shell.indexOf(`class="home-header home-command-bar"`);
  expect(barStart, "the shell should contain the Home command bar").toBeGreaterThanOrEqual(0);
  const bar = shell.slice(barStart, shell.indexOf("</header>", barStart));
  return bar.split("<button ").slice(1).flatMap((chunk) => {
    const id = /data-top-bar-control="([a-z]+)"/.exec(chunk)?.[1];
    if (!id) return [];
    const tag = chunk.slice(0, chunk.indexOf(">"));
    return [{
      id,
      current: /data-current-page="true"/.test(tag),
      ariaCurrent: /aria-current="page"/.test(tag),
      ariaLabel: /aria-label="([^"]*)"/.exec(tag)?.[1] ?? "",
      label: /<span class="home-top-bar-label">([^<]*)<\/span>/.exec(chunk)?.[1] ?? "",
      classes: /^class="([^"]*)"/.exec(tag)?.[1] ?? "",
    }];
  });
}

describe("TASK 0816 Home top bar", () => {
  it("names exactly five controls, in order, each with the word it is called", () => {
    expect([...ui.homeTopBarControlIds]).toEqual(["logo", "friends", "notifications", "settings", "profile"]);
    expect(ui.homeTopBarControlLabels).toEqual({
      logo: "Home",
      friends: "Friends",
      notifications: "Notifications",
      settings: "Settings",
      profile: "Profile",
    });
  });

  it("resolves the current control from the page the owner is actually on", () => {
    const page = (patch: Partial<Parameters<typeof ui.homeTopBarCurrentControl>[0]>) => ui.homeTopBarCurrentControl({
      route: "home",
      settingsSection: "account",
      friendsDialogOpen: false,
      profileSettingsFocus: false,
      ...patch,
    });

    expect(page({})).toBe("logo");
    expect(page({ friendsDialogOpen: true })).toBe("friends");
    expect(page({ route: "settings", settingsSection: "notifications" })).toBe("notifications");
    expect(page({ route: "settings", settingsSection: "account" })).toBe("settings");
    expect(page({ route: "settings", settingsSection: "account", profileSettingsFocus: true })).toBe("profile");
    // Settings pages the top bar does not own still belong to the Settings control.
    expect(page({ route: "settings", settingsSection: "apps" })).toBe("settings");
    // Sidebar destinations are owned by nobody in this bar, and say so.
    for (const route of ["inbox", "people", "privacy", "activity", "connections", "service"] as const) {
      expect(page({ route })).toBeNull();
    }
    // The Friends dialog opens over Home, and wins over the route beneath it.
    expect(page({ route: "settings", settingsSection: "notifications", friendsDialogOpen: true })).toBe("friends");
  });

  it("draws all five controls on every page the bar renders on", () => {
    const { __oslHubUiTest } = ui;
    for (const route of ["home", "settings", "inbox", "people", "privacy", "activity", "connections"] as const) {
      __oslHubUiTest.reset({ route, coreReady: true, storageMethod: "tpm-pcp" });
      const controls = topBarControls(__oslHubUiTest.renderRouteShell(route));
      expect(controls.map((control) => control.id), `route ${route}`).toEqual([
        "logo",
        "friends",
        "notifications",
        "settings",
        "profile",
      ]);
      expect(controls.map((control) => control.label), `route ${route}`).toEqual([
        "Home",
        "Friends",
        "Notifications",
        "Settings",
        "Profile",
      ]);
    }
  });

  it("marks exactly one control current, and marks it three ways that agree", () => {
    const { __oslHubUiTest } = ui;
    type ResetPatch = NonNullable<Parameters<typeof __oslHubUiTest.reset>[0]>;
    const cases: Array<{ name: string; patch: ResetPatch; route: "home" | "settings"; expected: string | null }> = [
      { name: "Home", patch: { route: "home" }, route: "home", expected: "logo" },
      { name: "Friends", patch: { route: "home", friendsDialogOpen: true }, route: "home", expected: "friends" },
      { name: "Notifications", patch: { route: "settings", settingsSection: "notifications" }, route: "settings", expected: "notifications" },
      { name: "Settings", patch: { route: "settings", settingsSection: "account" }, route: "settings", expected: "settings" },
      { name: "Profile", patch: { route: "settings", settingsSection: "account", profileSettingsFocus: true }, route: "settings", expected: "profile" },
      { name: "Inbox", patch: { route: "inbox" }, route: "home", expected: null },
    ];

    for (const scenario of cases) {
      __oslHubUiTest.reset({ coreReady: true, storageMethod: "tpm-pcp", ...scenario.patch });
      const shell = __oslHubUiTest.renderRouteShell(scenario.patch.route ?? scenario.route);
      const controls = topBarControls(shell);
      const current = controls.filter((control) => control.current);
      expect(current.length, `${scenario.name} should mark ${scenario.expected === null ? "no" : "one"} control`)
        .toBe(scenario.expected === null ? 0 : 1);
      if (scenario.expected !== null) {
        expect(current[0]?.id, scenario.name).toBe(scenario.expected);
        // The three markers are written from one boolean; they must agree.
        expect(current[0]?.ariaCurrent, `${scenario.name} aria-current`).toBe(true);
        expect(current[0]?.classes, `${scenario.name} class`).toContain("current");
      }
      for (const control of controls.filter((candidate) => !candidate.current)) {
        expect(control.ariaCurrent, `${scenario.name}/${control.id}`).toBe(false);
      }
    }
  });

  it("keeps the Settings device-protection status the bar took over", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "tpm-pcp" });
    expect(__oslHubUiTest.renderRouteShell("settings")).toContain('data-identity-protection="protected"');
    // Home is the owner's own chrome and does not repeat the device status.
    __oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
    const home = __oslHubUiTest.renderRouteShell("home");
    const bar = home.slice(home.indexOf("home-command-bar"), home.indexOf("</header>"));
    expect(bar).not.toContain("data-identity-protection");
  });

  it("styles the label so it cannot be shrunk, wrapped or ellipsised away", () => {
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(styles).toMatch(/\.home-top-bar-control\s*\{[^}]*width:\s*auto[^}]*white-space:\s*nowrap/s);
    expect(styles).toMatch(/\.home-top-bar-label\s*\{[^}]*white-space:\s*nowrap/s);
    expect(styles).not.toMatch(/\.home-top-bar-label\s*\{[^}]*text-overflow:\s*ellipsis/s);
    expect(styles).toMatch(/\.home-top-bar-control\.current\s*\{/);
  });
});
