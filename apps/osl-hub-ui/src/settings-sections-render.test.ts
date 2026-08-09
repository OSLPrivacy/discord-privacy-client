import { beforeAll, afterAll, beforeEach, describe, expect, it, vi } from "vitest";

// Every Settings section must RENDER. This screen once shipped a crash on
// every visit because `settingsContent()` in main.ts passed a hand-copied
// items list to `settingsHomeMenuMarkup`, the copy drifted (it lost
// "privacy"), and the menu's own guard threw on every render -- so the error
// boundary swallowed the whole Settings screen. Nothing caught it, because no
// test rendered the screen. This file renders every declared section through
// the real `settingsContent()` path, so a menu/type/content drift turns a
// test red instead of blanking the product.

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
  emitTo: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  window: {
    isFullscreen: vi.fn(() => Promise.resolve(false)),
    setFullscreen: vi.fn(() => Promise.resolve()),
    onResized: vi.fn(() => Promise.resolve(() => undefined)),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  providerLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  serviceLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
}));

import { settingsHomeChoices } from "./settings-home";

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
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
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("every declared Settings section renders", () => {
  // One test per declared choice, driven by the canonical list in
  // settings-home.ts, so adding a tenth choice there automatically adds a
  // tenth rendering requirement here -- this file can never go stale by hand.
  for (const choice of settingsHomeChoices) {
    it(`renders the "${choice.id}" section without throwing`, () => {
      ui.__oslHubUiTest.reset({});

      const markup = ui.__oslHubUiTest.renderSettingsSection(choice.id);

      // The whole home menu is present: every declared choice has its button.
      for (const declared of settingsHomeChoices) {
        expect(markup).toContain(`data-settings-home-choice="${declared.id}"`);
      }
      // The chosen section is marked current in the menu.
      expect(markup).toContain(`data-settings-home-choice="${choice.id}"`);
      expect(markup).toContain('aria-current="true"');
      // The detail pane exists and is not empty.
      const detailAt = markup.indexOf('<section class="settings-detail">');
      expect(detailAt).toBeGreaterThan(-1);
      const detail = markup.slice(detailAt + '<section class="settings-detail">'.length);
      expect(detail.replace(/<\/section>.*$/s, "").trim().length).toBeGreaterThan(0);
      // A crashed screen must never pass as a rendered one.
      expect(markup).not.toContain("ui-recovery");
      expect(markup).not.toContain("OSL paused this view");
    });
  }
});

describe("the crash view is machine-detectable", () => {
  it("carries a marker attribute a capture harness can fail on", () => {
    // The automated screenshot harness once graded the crash screen as a
    // conformant rendered screen (right background, right button colour). The
    // recovery view must carry a marker no healthy screen ever renders, so a
    // screenshot pipeline reading the DOM can hard-fail on a crashed view.
    const recovery = ui.__oslHubUiTest.renderRecoveryMarkupForTest();
    expect(recovery).toContain('data-osl-render-recovery="true"');
    expect(recovery).toContain("OSL paused this view");
  });
});
