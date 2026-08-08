import { afterEach, describe, expect, it, vi } from "vitest";


// D-251: every `it()` below deliberately re-loads `./main` with its own
// selectors / storage / stubs, so the import CANNOT be hoisted into a single
// `beforeAll` without destroying what the tests check. `src/main.ts` is ~10k
// lines and one load costs ~2.5 s cold, which left almost nothing of vitest's
// default 5,000 ms budget for the behaviour under test: on a busy machine these
// tests died with `Test timed out in 5000ms` before reaching an assertion.
// The budget below covers MODULE LOADING, not the behaviour -- no assertion
// depends on it, and every assertion is unchanged.
const MODULE_RELOAD_BUDGET_MS = 30_000;

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi(qaShell = false) {
  vi.resetModules();
  vi.stubEnv("VITE_OSL_DISCORD_QA_SHELL", qaShell ? "1" : "0");
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("Discord QA license gate", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  // Protects: the QA shell has no Pro code screen -- asking for that route lands
  // on the sending screen instead, and the code entry is not rendered anyway.
  it("gates the Pro onboarding destination to sending in the QA shell", async () => {
    const { __oslHubUiTest } = await loadUi(true);

    const markup = __oslHubUiTest.renderOnboardingRoute("pro");

    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("sending");
    // Identify the screen by what only it renders -- the send-mode chooser and
    // its Continue -- so a copy edit to the heading cannot silently pass this.
    expect(markup).toContain('data-send-mode="manual"');
    expect(markup).toContain('data-send-mode="single"');
    expect(markup).toContain('id="finish-onboarding"');
    expect(markup).not.toContain("Enter Pro code");
  }, MODULE_RELOAD_BUDGET_MS);

  it("does not expose activation settings in the QA shell", async () => {
    const { __oslHubUiTest } = await loadUi(true);

    expect(__oslHubUiTest.renderSettingsSection("account")).not.toContain("Activate Pro");
  }, MODULE_RELOAD_BUDGET_MS);

  it("retains the production Pro onboarding and activation UI", async () => {
    const { __oslHubUiTest } = await loadUi();

    const proPage = __oslHubUiTest.renderOnboardingRoute("pro");
    const settingsPage = __oslHubUiTest.renderSettingsSection("account");
    expect(proPage).toContain("Enter Pro code");
    expect(settingsPage).toContain("Activate Pro");
    for (const surface of [proPage, settingsPage]) {
      expect(surface).toMatch(/making one needs\s+Pro/u);
      expect(surface).toMatch(/opening one is\s+free/u);
    }
  }, MODULE_RELOAD_BUDGET_MS);
});
