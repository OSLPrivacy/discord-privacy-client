import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  LINUX_ONBOARDING_REQUIRED_CONTROLS,
  LINUX_ONBOARDING_SCREEN_FIXTURES,
  LINUX_ONBOARDING_SCREEN_WINDOW,
  prepareFixedLinuxOnboardingScreenData,
  type LinuxOnboardingScreenRun,
} from "./linux-onboarding-screen-data";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./styles.css", () => ({}));
vi.mock("./onboarding-before-send.css", () => ({}));
vi.mock("./onboarding-controls.css", () => ({}));
vi.mock("./onboarding-cover.css", () => ({}));
vi.mock("./onboarding-delete.css", () => ({}));
vi.mock("./onboarding-forward-secrecy.css", () => ({}));
vi.mock("./onboarding-mullvad.css", () => ({}));
vi.mock("./onboarding-sending.css", () => ({}));
vi.mock("./onboarding-stealth.css", () => ({}));
vi.mock("./onboarding-tor.css", () => ({}));
vi.mock("./recovery-screen.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

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

function renderedScreenRun(run: string): LinuxOnboardingScreenRun {
  const { __oslHubUiTest } = ui;

  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "create" });
  const createAccount = __oslHubUiTest.renderOnboardingRoute("create");

  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "welcome", bootstrapStatus: "passwordRequired" });
  const signIn = __oslHubUiTest.renderOnboardingRoute("welcome");

  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "pro" });
  const continueStep = __oslHubUiTest.renderOnboardingRoute("pro");

  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "unlock" });
  const back = __oslHubUiTest.renderOnboardingRoute("unlock");

  return prepareFixedLinuxOnboardingScreenData(run, {
    "create-account": createAccount,
    "sign-in": signIn,
    continue: continueStep,
    back,
  });
}

describe("fixed Linux onboarding screen data", () => {
  it("emits two deterministic non-empty runs with the named controls and 1280 by 800 window", () => {
    const first = renderedScreenRun("linux-onboarding-screen-data-run-1");
    const second = renderedScreenRun("linux-onboarding-screen-data-run-2");

    for (const run of [first, second]) {
      expect(run.window).toEqual({ width: 1280, height: 800 });
      expect(run.fixtures).toEqual(LINUX_ONBOARDING_SCREEN_FIXTURES);
      expect(run.screens).toHaveLength(4);
      expect(run.screens.every((screen) => screen.textLength > 0)).toBe(true);
      for (const control of LINUX_ONBOARDING_REQUIRED_CONTROLS) {
        expect(run.controls[control], `${run.run} should include ${control}`).toBe(true);
      }
    }

    expect(first.fixtures.accounts.map((account) => account.ownerName)).toEqual(["Alma Reed", "Miles Chen"]);
    expect(first.fixtures.names).toEqual(["Alma Reed", "Miles Chen", "Nora Vale"]);
    expect(first.fixtures.phrases).toEqual([
      "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
      "atlas broom cedar dusk ember flint grove honest iris kettle lunar mint",
    ]);
    expect(second.fixtures).toEqual(first.fixtures);
    expect(second.window).toEqual(first.window);
    expect(second.screens.map((screen) => screen.controls)).toEqual(first.screens.map((screen) => screen.controls));

    console.info("TASK-0324-SCREEN-DATA", JSON.stringify([first, second]));
  });

  it("rejects missing required controls", () => {
    const missingContinue = prepareFixedLinuxOnboardingScreenData("linux-onboarding-screen-data-red", {
      "create-account": "<button>Create account</button>",
      "sign-in": "<button>Sign in</button>",
      continue: "<p>No primary control</p>",
      back: "<button>Back</button>",
    });

    expect(missingContinue.controls.Continue).toBe(false);
  });

  it("pins the fixed fixture constants", () => {
    expect(LINUX_ONBOARDING_SCREEN_WINDOW).toEqual({ width: 1280, height: 800 });
    expect(LINUX_ONBOARDING_SCREEN_FIXTURES.accounts.map((account) => account.id)).toEqual([
      "osl-linux-screen-alma",
      "osl-linux-screen-miles",
    ]);
    expect(LINUX_ONBOARDING_SCREEN_FIXTURES.phrases).toHaveLength(2);
  });
});
