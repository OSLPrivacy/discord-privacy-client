import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

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
  focusCount = 0;

  constructor(readonly tagName: string, readonly id = "") {}

  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }

  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  removeAttribute(name: string): void { this.attributes.delete(name); }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  focus(): void { this.focusCount += 1; }

  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) {
      await handler({ preventDefault: () => undefined, currentTarget: this });
    }
  }
}

function restoreHarness() {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "identity-import-form");
  const phrase = new FakeElement("TEXTAREA", "identity-recovery-phrase");
  const password = new FakeElement("INPUT", "import-password");
  password.type = "password";
  const confirm = new FakeElement("INPUT", "import-password-confirm");
  confirm.type = "password";
  const passwordEye = new FakeElement("BUTTON");
  passwordEye.dataset.passwordToggle = "import-password";
  passwordEye.textContent = "Show password";
  passwordEye.setAttribute("aria-label", "Show password");
  const confirmEye = new FakeElement("BUTTON");
  confirmEye.dataset.passwordToggle = "import-password-confirm";
  confirmEye.textContent = "Show password";
  confirmEye.setAttribute("aria-label", "Show password");
  const restore = new FakeElement("BUTTON", "identity-import-submit");
  restore.disabled = true;
  restore.textContent = "Restore";
  const error = new FakeElement("P", "import-error");
  const back = new FakeElement("BUTTON");
  back.dataset.onboarding = "welcome";
  back.textContent = "Back";
  const nodes = new Map<string, FakeElement>([
    ["#app", app],
    ["#identity-import-form", form],
    ["#identity-recovery-phrase", phrase],
    ["#import-password", password],
    ["#import-password-confirm", confirm],
    ["#identity-import-submit", restore],
    ["#import-error", error],
  ]);

  return {
    app,
    form,
    phrase,
    password,
    confirm,
    passwordEye,
    confirmEye,
    restore,
    error,
    back,
    nodes,
    all(selector: string): FakeElement[] {
      if (selector === "[data-password-toggle]") return [passwordEye, confirmEye];
      if (selector === "[data-onboarding]") return [back];
      return [];
    },
  };
}

function readiness(bootstrapStatus: "setupRequired" | "passwordRequired") {
  return {
    originalCoreLinked: bootstrapStatus === "passwordRequired",
    identityLoaded: bootstrapStatus === "passwordRequired",
    keyserverInitialised: false,
    cloudRegistrationState: "notAttempted",
    groupSenderKeysEnabled: false,
    groupSenderKeysReachable: false,
    remoteServiceHasNativeAccess: false,
    bootstrapAttempted: true,
    passwordGateRequired: bootstrapStatus === "passwordRequired",
    unlocked: false,
    activeOslUserId: bootstrapStatus === "passwordRequired" ? "osl_task0329" : null,
    bootstrapStatus,
    storageMethod: bootstrapStatus === "passwordRequired" ? "os-keyring" : null,
  };
}

async function loadRestore(harness: ReturnType<typeof restoreHarness>) {
  vi.resetModules();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => { storage.set(key, value); },
    removeItem: (key: string) => { storage.delete(key); },
    clear: () => { storage.clear(); },
  });
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => harness.nodes.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => harness.all(selector)),
    getElementById: vi.fn((id: string) => harness.nodes.get(`#${id}`) ?? null),
    createElement: vi.fn((tag: string) => new FakeElement(String(tag).toUpperCase())),
    body: { append: vi.fn() },
    documentElement: new FakeElement("HTML"),
    addEventListener: vi.fn(),
    visibilityState: "visible",
    activeElement: null,
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  mocks.getCurrentWindow.mockReturnValue({
    isFocused: vi.fn(async () => true),
    close: vi.fn(async () => undefined),
  });

  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({
    route: "onboarding",
    onboardingRoute: "import",
    coreReady: false,
    bootstrapStatus: "notAttempted",
  });
  __oslHubUiTest.bindOnboarding();
  return __oslHubUiTest;
}

async function enterRestoreData(
  harness: ReturnType<typeof restoreHarness>,
  phrase: string,
  password: string,
): Promise<void> {
  harness.phrase.value = phrase;
  harness.password.value = password;
  harness.confirm.value = password;
  await harness.phrase.dispatch("input");
  await harness.password.dispatch("input");
  await harness.confirm.dispatch("input");
}

const savedPhrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
const changedPhrase = "abandon zoo able about above absent absorb abstract absurd abuse access accident";
const longPassword = "correct horse battery staple";

describe("TASK 0329 Restore buttons and bad phrase", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  it("calls both Show password controls twice and Back once from zero restored accounts", async () => {
    const harness = restoreHarness();
    const ui = await loadRestore(harness);
    const restoredAccounts: string[] = [];
    harness.password.value = longPassword;
    harness.confirm.value = longPassword;

    expect(restoredAccounts).toHaveLength(0);
    await harness.passwordEye.dispatch("click");
    const firstShown = { type: harness.password.type, value: harness.password.value };
    await harness.passwordEye.dispatch("click");
    const firstHidden = { type: harness.password.type, value: harness.password.value };
    await harness.confirmEye.dispatch("click");
    const secondShown = { type: harness.confirm.type, value: harness.confirm.value };
    await harness.confirmEye.dispatch("click");
    const secondHidden = { type: harness.confirm.type, value: harness.confirm.value };

    console.log(`TASK0329_SHOW_NEW accounts=${restoredAccounts.length} shown_type=${firstShown.type} shown_value=${firstShown.value} hidden_type=${firstHidden.type} hidden_value=${firstHidden.value}`);
    console.log(`TASK0329_SHOW_CONFIRM accounts=${restoredAccounts.length} shown_type=${secondShown.type} shown_value=${secondShown.value} hidden_type=${secondHidden.type} hidden_value=${secondHidden.value}`);
    expect(firstShown).toEqual({ type: "text", value: longPassword });
    expect(firstHidden).toEqual({ type: "password", value: longPassword });
    expect(secondShown).toEqual({ type: "text", value: longPassword });
    expect(secondHidden).toEqual({ type: "password", value: longPassword });

    await harness.back.dispatch("click");
    console.log(`TASK0329_BACK control=${harness.back.textContent} route=${ui.snapshot().onboardingRoute}`);
    expect(ui.snapshot().onboardingRoute).toBe("welcome");
  }, 30_000);

  it("records one restored account for the saved phrase and opens Unlock", async () => {
    const harness = restoreHarness();
    const restoredAccounts: string[] = [];
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "import_hub_osl_identity_phrase") {
        expect(args).toEqual({ recoveryPhrase: savedPhrase });
        restoredAccounts.push("osl_task0329");
        return {
          userId: "osl_task0329",
          identityRecoveryPhrase: null,
          storageMethod: "os-keyring",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password") {
        expect(args).toEqual({ password: longPassword });
        throw new Error("fixture interruption after the restored account was saved");
      }
      if (command === "get_core_readiness") return readiness("passwordRequired");
      throw new Error(`unexpected command ${command}`);
    });
    const ui = await loadRestore(harness);
    await enterRestoreData(harness, savedPhrase, longPassword);
    expect(harness.restore.disabled).toBe(false);
    await harness.form.dispatch("submit");

    console.log(`TASK0329_VALID control=${harness.restore.textContent} phrase=${savedPhrase} password_len=${longPassword.length} matching=${harness.password.value === harness.confirm.value} restored_accounts=${restoredAccounts.length} route=${ui.snapshot().onboardingRoute}`);
    expect(restoredAccounts).toEqual(["osl_task0329"]);
    expect(ui.snapshot().onboardingRoute).toBe("unlock");
  }, 30_000);

  it("refuses one changed recovery word and leaves Restore unchanged with zero accounts", async () => {
    const harness = restoreHarness();
    const restoredAccounts: string[] = [];
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "import_hub_osl_identity_phrase") {
        expect(args).toEqual({ recoveryPhrase: changedPhrase });
        throw new Error("recovery phrase was refused");
      }
      if (command === "get_core_readiness") return readiness("setupRequired");
      throw new Error(`unexpected command ${command}`);
    });
    const ui = await loadRestore(harness);
    await enterRestoreData(harness, changedPhrase, longPassword);
    expect(harness.restore.disabled).toBe(false);
    await harness.form.dispatch("submit");

    console.log(`TASK0329_BAD_PHRASE changed_word=2 saved=ability entered=zoo control=${harness.restore.textContent} restored_accounts=${restoredAccounts.length} route=${ui.snapshot().onboardingRoute} restore_enabled=${!harness.restore.disabled}`);
    expect(restoredAccounts).toHaveLength(0);
    expect(harness.restore.textContent).toBe("Restore");
    expect(harness.restore.disabled).toBe(false);
    expect(ui.snapshot().onboardingRoute).toBe("import");
  }, 30_000);
});
