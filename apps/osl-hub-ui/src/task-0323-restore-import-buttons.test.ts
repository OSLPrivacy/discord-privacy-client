import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

class FakeElement {
  value = "";
  type = "";
  disabled = false;
  textContent = "";
  innerHTML = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Array<(event: unknown) => unknown>>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };

  constructor(readonly tagName: string, readonly id = "") {}

  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }
  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) await handler({ preventDefault: () => undefined, currentTarget: this });
  }
  focus(): void {}
}

function harness() {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "identity-import-form");
  const phrase = new FakeElement("TEXTAREA", "identity-recovery-phrase");
  const password = new FakeElement("INPUT", "import-password"); password.type = "password";
  const confirm = new FakeElement("INPUT", "import-password-confirm"); confirm.type = "password";
  const passwordEye = new FakeElement("BUTTON"); passwordEye.dataset.passwordToggle = "import-password";
  const confirmEye = new FakeElement("BUTTON"); confirmEye.dataset.passwordToggle = "import-password-confirm";
  const submit = new FakeElement("BUTTON", "identity-import-submit"); submit.disabled = true;
  const error = new FakeElement("P", "import-error");
  const back = new FakeElement("BUTTON"); back.dataset.onboarding = "welcome";
  const nodes = new Map<string, FakeElement>([
    ["#app", app], ["#identity-import-form", form], ["#identity-recovery-phrase", phrase],
    ["#import-password", password], ["#import-password-confirm", confirm],
    ["#identity-import-submit", submit], ["#import-error", error],
  ]);
  return { app, form, phrase, password, confirm, passwordEye, confirmEye, submit, error, back, nodes,
    all(selector: string) { return selector === "[data-password-toggle]" ? [passwordEye, confirmEye] : selector === "[data-onboarding]" ? [back] : []; },
    submitForm: () => form.dispatch("submit"),
  };
}

function calls(command: string): unknown[][] { return mocks.invoke.mock.calls.filter((call) => call[0] === command); }

async function load(h: ReturnType<typeof harness>) {
  vi.resetModules();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key), clear: () => storage.clear() });
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => h.nodes.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => h.all(selector)),
    getElementById: vi.fn((id: string) => h.nodes.get(`#${id}`) ?? null),
    createElement: vi.fn((tag: string) => new FakeElement(tag.toUpperCase())),
    body: { append: vi.fn() }, documentElement: new FakeElement("HTML"), addEventListener: vi.fn(), visibilityState: "visible", activeElement: null,
  });
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  mocks.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), close: vi.fn(async () => undefined) });
  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "import", coreReady: false, bootstrapStatus: "notAttempted" });
  const markup = __oslHubUiTest.renderOnboardingRoute("import");
  __oslHubUiTest.bindOnboarding();
  return { ui: __oslHubUiTest, markup };
}

describe("TASK 0323 restore import buttons", () => {
  beforeEach(() => { vi.unstubAllGlobals(); mocks.invoke.mockReset(); mocks.listen.mockReset(); mocks.emitTo.mockReset(); mocks.getCurrentWindow.mockReset(); });

  it("keeps Restore unavailable until the exact phrase and matching password are valid", async () => {
    const h = harness();
    const { ui, markup } = await load(h);
    const validPhrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
    const password = "correct horse battery staple";
    const imports = () => calls("import_hub_osl_identity_phrase").length;
    const setups = () => calls("setup_hub_main_password").length;

    const controlCounts = {
      phrase: (markup.match(/id="identity-recovery-phrase"/gu) ?? []).length,
      passwords: (markup.match(/id="import-password(?:-confirm)?"/gu) ?? []).length,
      show: (markup.match(/data-password-toggle=/gu) ?? []).length,
      restore: (markup.match(/id="identity-import-submit"/gu) ?? []).length,
      back: (markup.match(/data-onboarding="welcome"/gu) ?? []).length,
    };
    console.log(`TASK0323_CONTROLS phrase=${controlCounts.phrase} passwords=${controlCounts.passwords} show=${controlCounts.show} restore=${controlCounts.restore} back=${controlCounts.back}`);
    expect(controlCounts).toEqual({ phrase: 1, passwords: 2, show: 2, restore: 1, back: 1 });
    expect(markup).toContain("Restore your account");

    console.log(`TASK0323_INITIAL unavailable=${h.submit.disabled}`);
    expect(h.submit.disabled).toBe(true);

    h.phrase.value = "not a twelve word recovery phrase at all";
    h.password.value = password; h.confirm.value = password;
    await h.phrase.dispatch("input"); await h.password.dispatch("input"); await h.confirm.dispatch("input");
    const invalidUnavailable = h.submit.disabled;
    h.submit.disabled = false;
    await h.submitForm();
    console.log(`TASK0323_INVALID_PHRASE unavailable=${invalidUnavailable} forced_disabled=${h.submit.disabled} imports=${imports()} setups=${setups()}`);
    expect(invalidUnavailable).toBe(true);
    expect(h.submit.disabled).toBe(true);
    expect(imports()).toBe(0); expect(setups()).toBe(0);

    h.phrase.value = validPhrase; h.confirm.value = "different valid password";
    await h.phrase.dispatch("input"); await h.confirm.dispatch("input");
    const mismatchUnavailable = h.submit.disabled;
    h.submit.disabled = false;
    await h.submitForm();
    console.log(`TASK0323_MISMATCH unavailable=${mismatchUnavailable} forced_disabled=${h.submit.disabled} imports=${imports()} setups=${setups()}`);
    expect(mismatchUnavailable).toBe(true);
    expect(h.submit.disabled).toBe(true);
    expect(imports()).toBe(0); expect(setups()).toBe(0);

    h.password.value = password; h.confirm.value = password;
    await h.password.dispatch("input"); await h.confirm.dispatch("input");
    const availableBeforeToggles = !h.submit.disabled;
    await h.passwordEye.dispatch("click");
    expect(h.password.type).toBe("text"); expect(h.confirm.type).toBe("password"); expect(h.password.value).toBe(password);
    await h.confirmEye.dispatch("click");
    expect(h.confirm.type).toBe("text"); expect(h.password.type).toBe("text"); expect(h.confirm.value).toBe(password);
    await h.passwordEye.dispatch("click"); await h.confirmEye.dispatch("click");
    console.log(`TASK0323_VISIBILITY password=${h.password.type} confirm=${h.confirm.type} availability_unchanged=${!h.submit.disabled === availableBeforeToggles}`);
    expect(h.password.type).toBe("password"); expect(h.confirm.type).toBe("password"); expect(h.submit.disabled).toBe(false);

    await h.back.dispatch("click");
    console.log(`TASK0323_BACK route=${ui.snapshot().onboardingRoute} availability_unchanged=${!h.submit.disabled === availableBeforeToggles}`);
    expect(ui.snapshot().onboardingRoute).toBe("welcome"); expect(h.submit.disabled).toBe(false);

    ui.reset({ route: "onboarding", onboardingRoute: "import", coreReady: false, bootstrapStatus: "notAttempted" });
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "import_hub_osl_identity_phrase") { expect(args).toEqual({ recoveryPhrase: validPhrase }); return { userId: "osl_task0323", identityRecoveryPhrase: null, storageMethod: "os-keyring", passwordSetupRequired: true }; }
      if (command === "setup_hub_main_password") { expect(args).toEqual({ password }); return { passwordRecoveryPhrase: validPhrase, encryptedStateReloadComplete: true, encryptedStateReloadIssueCount: 0, readiness: { accessState: "ready", identityLoaded: true, mainPasswordSet: true, unlocked: true, serviceNeutralIdentitySupported: true, canCreateIdentity: false, canImportIdentityPhrase: true, passwordAttemptsUsed: 0, passwordLockoutSecondsRemaining: 0 } }; }
      if (command === "get_core_readiness") return { originalCoreLinked: true, identityLoaded: true, keyserverInitialised: true, cloudRegistrationState: "registered", groupSenderKeysEnabled: true, groupSenderKeysReachable: true, remoteServiceHasNativeAccess: true, bootstrapAttempted: true, passwordGateRequired: false, unlocked: true, activeOslUserId: "osl_task0323", bootstrapStatus: "ready", storageMethod: "os-keyring" };
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") return { mainPasswordSet: true, stealthPasswordSet: false, burnPasswordSet: false, unlocked: true, stealthActionWired: false, burnActionWired: false };
      if (command === "set_hub_recovery_kit_unsaved" || command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });
    await h.submitForm();
    console.log(`TASK0323_SUCCESS imports=${imports()} setups=${setups()} route=${ui.snapshot().onboardingRoute}`);
    expect(imports()).toBe(1); expect(setups()).toBe(1); expect(ui.snapshot().onboardingRoute).toBe("recovery");
  }, 30_000);
});
