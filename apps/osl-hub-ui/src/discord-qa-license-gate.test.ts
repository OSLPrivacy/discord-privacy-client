import { afterEach, describe, expect, it, vi } from "vitest";

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

  it("gates the Pro onboarding destination to sending in the QA shell", async () => {
    const { __oslHubUiTest } = await loadUi(true);

    const markup = __oslHubUiTest.renderOnboardingRoute("pro");

    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("sending");
    expect(markup).toContain("Choose how to send");
    expect(markup).not.toContain("Enter Pro code");
  });

  it("does not expose activation settings in the QA shell", async () => {
    const { __oslHubUiTest } = await loadUi(true);

    expect(__oslHubUiTest.renderSettingsSection("account")).not.toContain("Activate Pro");
  });

  it("retains the production Pro onboarding and activation UI", async () => {
    const { __oslHubUiTest } = await loadUi();

    expect(__oslHubUiTest.renderOnboardingRoute("pro")).toContain("Enter Pro code");
    expect(__oslHubUiTest.renderSettingsSection("account")).toContain("Activate Pro");
  });
});
