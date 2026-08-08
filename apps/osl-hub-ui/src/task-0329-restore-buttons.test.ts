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
  focusCount = 0;
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };

  constructor(readonly tagName: string, readonly id = "") {}

  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  focus(): void {
    this.focusCount += 1;
  }

  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) {
      await handler({ preventDefault: () => undefined, currentTarget: this });
    }
  }
}

interface RestoreHarness {
  app: FakeElement;
  form: FakeElement;
  phrase: FakeElement;
  password: FakeElement;
  confirm: FakeElement;
  passwordShow: FakeElement;
  confirmShow: FakeElement;
  restore: FakeElement;
  error: FakeElement;
  back: FakeElement;
  nodes: Map<string, FakeElement>;
  all(selector: string): FakeElement[];
}

function buildRestoreHarness(): RestoreHarness {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "identity-import-form");
  const phrase = new FakeElement("TEXTAREA", "identity-recovery-phrase");
  const password = new FakeElement("INPUT", "import-password");
  password.type = "password";
  const confirm = new FakeElement("INPUT", "import-password-confirm");
  confirm.type = "password";
  const passwordShow = new FakeElement("BUTTON");
  passwordShow.dataset.passwordToggle = "import-password";
  passwordShow.setAttribute("aria-label", "Show password");
  const confirmShow = new FakeElement("BUTTON");
  confirmShow.dataset.passwordToggle = "import-password-confirm";
  confirmShow.setAttribute("aria-label", "Show password");
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
    passwordShow,
    confirmShow,
    restore,
    error,
    back,
    nodes,
    all(selector: string): FakeElement[] {
      if (selector === "[data-password-toggle]") return [passwordShow, confirmShow];
      if (selector === "[data-onboarding]") return [back];
      return [];
    },
  };
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => [...values.keys()][index] ?? null,
    removeItem: (key: string) => { values.delete(key); },
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
}

async function loadRestoreUi(harness: RestoreHarness) {
  vi.resetModules();
  vi.stubGlobal("localStorage", memoryStorage());
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

function readiness(
  bootstrapStatus: "notAttempted" | "setupRequired" | "passwordRequired",
  identityLoaded: boolean,
): Record<string, unknown> {
  return {
    originalCoreLinked: identityLoaded,
    identityLoaded,
    keyserverInitialised: identityLoaded,
    cloudRegistrationState: "notAttempted",
    groupSenderKeysEnabled: false,
    groupSenderKeysReachable: false,
    remoteServiceHasNativeAccess: false,
    bootstrapAttempted: true,
    passwordGateRequired: bootstrapStatus === "passwordRequired",
    unlocked: false,
    activeOslUserId: identityLoaded ? "osl_task0329" : null,
    bootstrapStatus,
    storageMethod: identityLoaded ? "os-keyring" : null,
  };
}

async function enterValidRestoreData(harness: RestoreHarness, phrase: string, password: string): Promise<void> {
  harness.phrase.value = phrase;
  harness.password.value = password;
  harness.confirm.value = password;
  await harness.phrase.dispatch("input");
  await harness.password.dispatch("input");
  await harness.confirm.dispatch("input");
  expect(harness.restore.disabled).toBe(false);
}

describe("TASK 0329 Restore buttons and bad phrase", () => {
  const savedPhrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
  const changedPhrase = "abandon ability able about above absent absorb abstract absurd abuse access acoustic";
  const longPassword = "correct horse battery staple";

  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  it("records both Show password controls and Back from zero restored accounts", async () => {
    const harness = buildRestoreHarness();
    const ui = await loadRestoreUi(harness);
    const restoredAccounts: string[] = [];

    harness.password.value = longPassword;
    await harness.passwordShow.dispatch("click");
    console.log(`TASK0329_SHOW_NEW first_press_type=${harness.password.type} entered_password=${harness.password.value} restored_accounts=${restoredAccounts.length}`);
    expect(harness.password.type).toBe("text");
    expect(harness.password.value).toBe(longPassword);
    await harness.passwordShow.dispatch("click");
    console.log(`TASK0329_SHOW_NEW next_press_type=${harness.password.type} entered_password_preserved=${harness.password.value} restored_accounts=${restoredAccounts.length}`);
    expect(harness.password.type).toBe("password");
    expect(harness.password.value).toBe(longPassword);

    harness.confirm.value = longPassword;
    await harness.confirmShow.dispatch("click");
    console.log(`TASK0329_SHOW_CONFIRM first_press_type=${harness.confirm.type} entered_password=${harness.confirm.value} restored_accounts=${restoredAccounts.length}`);
    expect(harness.confirm.type).toBe("text");
    expect(harness.confirm.value).toBe(longPassword);
    await harness.confirmShow.dispatch("click");
    console.log(`TASK0329_SHOW_CONFIRM next_press_type=${harness.confirm.type} entered_password_preserved=${harness.confirm.value} restored_accounts=${restoredAccounts.length}`);
    expect(harness.confirm.type).toBe("password");
    expect(harness.confirm.value).toBe(longPassword);

    await harness.back.dispatch("click");
    console.log(`TASK0329_BACK control=${harness.back.textContent} opens=${ui.snapshot().onboardingRoute} restored_accounts=${restoredAccounts.length}`);
    expect(ui.snapshot().onboardingRoute).toBe("welcome");
    expect(restoredAccounts).toHaveLength(0);
  }, 30_000);

  it("records the successful Restore destination for the saved phrase", async () => {
    const harness = buildRestoreHarness();
    const ui = await loadRestoreUi(harness);
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
        return {
          passwordRecoveryPhrase: savedPhrase,
          encryptedStateReloadComplete: true,
          encryptedStateReloadIssueCount: 0,
          readiness: {
            accessState: "ready",
            identityLoaded: true,
            mainPasswordSet: true,
            unlocked: true,
            serviceNeutralIdentitySupported: true,
            canCreateIdentity: false,
            canImportIdentityPhrase: true,
            passwordAttemptsUsed: 0,
            passwordLockoutSecondsRemaining: 0,
          },
        };
      }
      if (command === "get_core_readiness") {
        return {
          ...readiness("passwordRequired", true),
          cloudRegistrationState: "registered",
          passwordGateRequired: false,
          unlocked: true,
          bootstrapStatus: "ready",
        };
      }
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") {
        return {
          mainPasswordSet: true,
          stealthPasswordSet: false,
          burnPasswordSet: false,
          unlocked: true,
          stealthActionWired: false,
          burnActionWired: false,
        };
      }
      if (command === "set_hub_recovery_kit_unsaved" || command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });

    await enterValidRestoreData(harness, savedPhrase, longPassword);
    await harness.form.dispatch("submit");

    console.log(`TASK0329_RESTORE_VALID control=${harness.restore.textContent} phrase=saved passwords=matching-long restored_accounts=${restoredAccounts.length} opens=${ui.snapshot().onboardingRoute} required_unlock=${ui.snapshot().onboardingRoute === "unlock"}`);
    expect(restoredAccounts).toEqual(["osl_task0329"]);
    expect(ui.snapshot().onboardingRoute).toBe("recovery");
    expect(ui.snapshot().onboardingRoute).not.toBe("unlock");
  }, 30_000);

  it("refuses one changed recovery word and leaves Restore unchanged", async () => {
    const harness = buildRestoreHarness();
    const ui = await loadRestoreUi(harness);
    const restoredAccounts: string[] = [];

    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "import_hub_osl_identity_phrase") {
        expect(args).toEqual({ recoveryPhrase: changedPhrase });
        throw new Error("recovery phrase does not match the saved account");
      }
      if (command === "get_core_readiness") return readiness("setupRequired", false);
      throw new Error(`unexpected command ${command}`);
    });

    await enterValidRestoreData(harness, changedPhrase, longPassword);
    const restoreLabelBefore = harness.restore.textContent;
    await harness.form.dispatch("submit");

    console.log(`TASK0329_RESTORE_BAD changed_words=1 refused=${harness.error.textContent.length > 0} restored_accounts=${restoredAccounts.length} route=${ui.snapshot().onboardingRoute} restore_before=${restoreLabelBefore} restore_after=${harness.restore.textContent}`);
    expect(harness.error.textContent).toBe("recovery phrase does not match the saved account");
    expect(restoredAccounts).toHaveLength(0);
    expect(ui.snapshot().onboardingRoute).toBe("import");
    expect(harness.restore.textContent).toBe(restoreLabelBefore);
    expect(harness.restore.textContent).toBe("Restore");
  }, 30_000);
});
