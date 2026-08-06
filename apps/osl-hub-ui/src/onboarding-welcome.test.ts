import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const MODULE_RELOAD_BUDGET_MS = 30_000;

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => [...values.keys()][index] ?? null,
    removeItem: (key: string) => { values.delete(key); },
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
}

function installDom(): void {
  const root = {
    innerHTML: "",
    querySelector: () => null,
    querySelectorAll: () => [],
  };
  const createElement = () => ({
    className: "",
    role: "",
    textContent: "",
    classList: { add: vi.fn() },
    addEventListener: vi.fn(),
    remove: vi.fn(),
  });
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : null,
    querySelectorAll: () => [],
    createElement,
    body: { append: vi.fn() },
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

async function loadUi() {
  vi.resetModules();
  vi.stubGlobal("localStorage", memoryStorage());
  installDom();
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout: vi.fn(() => 1),
    clearTimeout: vi.fn(),
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("onboarding welcome content", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  const welcome = functionSource("welcomeOnboardingContent", "proSetupContent");
  const entry = functionSource("entryScreenContent", "welcomeOnboardingContent");

  it("renders the canonical entry skeleton for both branches and keeps removed introduction copy out", () => {
    // Real design export 2026-08-08: first run is Create Account.dc.html and a
    // returning device is Sign In Final.dc.html -- the SAME bare skeleton with
    // one button and the quiet recovery link. The interim three-button
    // "Welcome" chooser is gone with the invented spec that drew it.
    expect(welcome).toContain('entryScreenContent("Sign in", signinLockIcon(), "unlock")');
    expect(welcome).toContain('entryScreenContent("Create account", signinPlusIcon(), "create")');
    expect(welcome).not.toContain("welcome-choice-screen");
    expect(welcome).not.toContain(">Welcome</h1>");

    // Code only: the function's own comment lists the copy that was removed in
    // order to explain why, so the "it stays removed" check must not read it.
    const shown = `${welcome}${entry}`.replace(/^\s*\/\/.*$/gmu, "");
    expect(shown).not.toContain("Protect the accounts you already use");
    expect(shown).not.toContain("messaging, social and email accounts you already have");
    expect(shown).not.toContain("private place");
    expect(shown).not.toContain("OSL communication is built in");
    expect(shown).not.toMatch(/add another identity/iu);
  });

  it("keeps the shared entry screen available for direct create and unlock routes", () => {
    expect(entry).toContain('<h1 id="route-heading" class="sr-only" tabindex="-1">${label}</h1>');
    expect(entry).toContain('<button class="signin-unlock" data-onboarding="${route}" type="button">');
    expect(entry).toContain('<button class="signin-recovery" data-onboarding="import" type="button">Use recovery phrase</button>');
  });

  // Protects the first-run reading level. The copy lives in the shared entry
  // screen now, so the rule has to read that too or it checks nothing.
  it("does not expose implementation concepts in the welcome copy", () => {
    expect(`${welcome}${entry}`).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/i);
  });

  it("is the only welcome branch rendered by onboardingContent", () => {
    const content = functionSource("onboardingContent", "welcomeOnboardingContent");
    expect(content).toContain('if (onboardingRoute === "welcome") return welcomeOnboardingContent();');
  });

  it("calls the Welcome Create, Restore, and Unlock actions directly and refuses wrong routes", async () => {
    const { __oslHubUiTest } = await loadUi();

    const cases = [
      {
        label: "Create",
        action: "create",
        invalid: "create-wrong-route",
        page: "create",
        markers: ["Create a password", "Create account"],
      },
      {
        label: "Restore",
        action: "import",
        invalid: "restore-wrong-route",
        page: "import",
        markers: ["Restore your account", 'id="identity-import-form"', "Recovery phrase"],
      },
      {
        label: "Unlock",
        action: "unlock",
        invalid: "unlock-wrong-route",
        page: "unlock",
        markers: ["Sign in", "Unlock", 'data-password-mode="unlock"'],
      },
    ] as const;

    let validReached = 0;
    let invalidFailed = 0;
    for (const item of cases) {
      const valid = __oslHubUiTest.callWelcomeActionForTest(item.action);
      expect(valid.accepted, `${item.label} should accept its Welcome action`).toBe(true);
      expect(valid.route).toBe("onboarding");
      expect(valid.onboardingRoute).toBe(item.page);
      for (const marker of item.markers) expect(valid.markup).toContain(marker);
      validReached += 1;
      console.info(`TASK0325 valid action=${item.label} route=${valid.onboardingRoute} accepted=${valid.accepted} marker="${item.markers[0]}"`);

      const invalid = __oslHubUiTest.callWelcomeActionForTest(item.invalid);
      expect(invalid.accepted, `${item.label} invalid route should fail`).toBe(false);
      expect(invalid.route).toBe("onboarding");
      expect(invalid.onboardingRoute).toBe("welcome");
      expect(invalid.markup).toContain('data-onboarding="import"');
      expect(invalid.markup).not.toContain("Sending behavior");
      invalidFailed += 1;
      console.info(`TASK0325 invalid action=${item.label} raw=${item.invalid} accepted=${invalid.accepted} stayed=${invalid.onboardingRoute} fallback_sending=${invalid.markup.includes("Sending behavior")}`);
    }
    expect(validReached).toBe(3);
    expect(invalidFailed).toBe(3);
    console.info(`TASK0325 valid_reached_count=${validReached} invalid_failed_count=${invalidFailed}`);
  }, MODULE_RELOAD_BUDGET_MS);
});
