import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { RISK_ACKNOWLEDGEMENT, SEND_OPTIONS, onboardingSendingMarkup, sendModeIsDangerous } from "./onboarding-sending";
import type { SendMode } from "./state";

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
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
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

function sendModeButtons(markup: string): string[] {
  return [...markup.matchAll(/data-send-mode="([^"]+)"/gu)].map((match) => match[1]);
}

function screen(mode: SendMode, riskAccepted = false): string {
  return onboardingSendingMarkup({ mode, riskAccepted, captureEnabled: false, captureApplied: false });
}

describe("onboarding send modes", () => {
  // Protects: onboarding renders the mode the owner actually saved. Single Enter
  // was always in the send model and offered by the overlay, but this screen
  // rewrote a stored "single" back to "manual" before rendering -- showing a mode
  // the owner had never chosen. Single Enter is offered here now (2026-08-06
  // owner decision), so that downgrade must stay deleted.
  it("offers all four send modes and never downgrades a saved Single Enter", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset();

    const html = __oslHubUiTest.renderOnboardingSendModes("single");

    expect(sendModeButtons(html)).toEqual(["manual", "clipboard", "double", "single"]);
    expect(html).toContain("Single Enter");
    expect(html).toContain('data-send-mode="single" checked');
    expect(html).not.toContain('data-send-mode="manual" checked');
    // The honest limit is stated on the same screen as the choice.
    expect(html).toMatch(/cannot prove where it is sending[^<]*sends nothing/iu);
  });

  // Protects the rule the old "no Single Enter in onboarding" assertions stood in
  // for, in the stronger form that survives Single Enter being offered: an
  // experimental send mode is never reachable without the acknowledgement.
  // Both key-pressing modes raise #accept-send-risk and hold Continue disabled
  // until it is ticked; neither ordinary mode raises it or is held back.
  it("keeps every experimental mode behind the risk acknowledgement", () => {
    for (const mode of ["double", "single"] as const) {
      const pending = screen(mode);
      expect(pending).toContain('id="accept-send-risk"');
      expect(pending).toContain(RISK_ACKNOWLEDGEMENT);
      expect(pending).toMatch(/id="finish-onboarding"\s+disabled/u);

      const acknowledged = screen(mode, true);
      expect(acknowledged).toContain('id="accept-send-risk"');
      expect(acknowledged).toContain('id="accept-send-risk" type="checkbox" checked');
      expect(acknowledged).not.toMatch(/id="finish-onboarding"[^>]*disabled/u);
    }
    for (const mode of ["manual", "clipboard"] as const) {
      const ordinary = screen(mode);
      expect(ordinary).not.toContain('id="accept-send-risk"');
      expect(ordinary).not.toContain(RISK_ACKNOWLEDGEMENT);
      expect(ordinary).not.toMatch(/id="finish-onboarding"[^>]*disabled/u);
    }
  });

  // Protects: the two modes that press keys for you -- and only those two -- are
  // marked as risky where the owner picks them, so the tag cannot drift off the
  // experimental modes or onto the safe ones while the acknowledgement stays put.
  it("marks exactly the two key-pressing modes as risky in the chooser", () => {
    const { sendingSetupContent } = ui;
    const markup = sendingSetupContent();

    expect(sendModeButtons(markup)).toEqual(["manual", "clipboard", "double", "single"]);
    expect(SEND_OPTIONS.filter((option) => option.tag !== "").map((option) => option.mode)).toEqual(["double", "single"]);
    expect(SEND_OPTIONS.filter((option) => sendModeIsDangerous(option.mode)).every((option) => /danger|warn/u.test(option.tagKind))).toBe(true);
    for (const option of SEND_OPTIONS) {
      expect(markup).toContain(`<strong>${option.name}</strong>`);
      expect(sendModeIsDangerous(option.mode)).toBe(option.tag !== "");
    }
  });
});
