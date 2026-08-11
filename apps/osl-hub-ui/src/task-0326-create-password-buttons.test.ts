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
  tagName: string;
  id: string;
  value = "";
  type = "";
  disabled = false;
  checked = false;
  textContent = "";
  innerHTML = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Array<(event: unknown) => unknown>>();
  focusCount = 0;
  classList = {
    add: vi.fn(),
    remove: vi.fn(),
    toggle: vi.fn(),
  };
  elements = {
    namedItem: (_name: string): FakeElement | null => null,
  };

  constructor(tagName: string, id = "") {
    this.tagName = tagName;
    this.id = id;
  }

  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    const list = this.handlers.get(type) ?? [];
    list.push(handler);
    this.handlers.set(type, list);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  focus(): void {
    this.focusCount += 1;
  }

  async dispatch(type: string, event: Record<string, unknown> = {}): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) {
      await handler({ preventDefault: () => undefined, currentTarget: this, ...event });
    }
  }
}

interface CreatePasswordHarness {
  app: FakeElement;
  form: FakeElement;
  password: FakeElement;
  confirm: FakeElement;
  noRecoverySecret: FakeElement;
  passwordEye: FakeElement;
  confirmEye: FakeElement;
  submitButton: FakeElement;
  backButton: FakeElement;
  error: FakeElement;
  nodes: Map<string, FakeElement>;
  all(selector: string): FakeElement[];
  submit(): Promise<void>;
  clickBack(): Promise<void>;
}

function buildCreatePasswordHarness(): CreatePasswordHarness {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "identity-password-form");
  form.dataset.passwordMode = "setup";
  const password = new FakeElement("INPUT", "identity-password");
  password.type = "password";
  const confirm = new FakeElement("INPUT", "identity-password-confirm");
  confirm.type = "password";
  const noRecoverySecret = new FakeElement("INPUT", "identity-no-recovery-secret");
  noRecoverySecret.type = "checkbox";
  const passwordEye = new FakeElement("BUTTON");
  passwordEye.dataset.passwordToggle = "identity-password";
  passwordEye.textContent = "Show password";
  passwordEye.setAttribute("aria-label", "Show password");
  const confirmEye = new FakeElement("BUTTON");
  confirmEye.dataset.passwordToggle = "identity-password-confirm";
  confirmEye.textContent = "Show password";
  confirmEye.setAttribute("aria-label", "Show password");
  const submitButton = new FakeElement("BUTTON", "identity-password-submit");
  submitButton.disabled = true;
  submitButton.textContent = "Create account";
  const backButton = new FakeElement("BUTTON");
  backButton.dataset.onboarding = "welcome";
  backButton.textContent = "Back";
  const error = new FakeElement("P", "password-error");
  const nodes = new Map<string, FakeElement>([
    ["#app", app],
    ["#identity-password-form", form],
    ["#identity-password", password],
    ["#identity-password-confirm", confirm],
    ["#identity-no-recovery-secret", noRecoverySecret],
    ["#identity-password-submit", submitButton],
    ["#password-error", error],
  ]);
  return {
    app,
    form,
    password,
    confirm,
    noRecoverySecret,
    passwordEye,
    confirmEye,
    submitButton,
    backButton,
    error,
    nodes,
    all(selector: string): FakeElement[] {
      if (selector === "[data-password-toggle]") return [passwordEye, confirmEye];
      if (selector === "[data-onboarding]") return [backButton];
      return [];
    },
    async submit(): Promise<void> {
      expect(form.handlers.get("submit") ?? [], "create-password form was not bound").toHaveLength(1);
      await form.dispatch("submit");
    },
    async clickBack(): Promise<void> {
      expect(backButton.handlers.get("click") ?? [], "Back command was not bound").toHaveLength(1);
      await backButton.dispatch("click");
    },
  };
}

function readiness(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    originalCoreLinked: false,
    identityLoaded: false,
    keyserverInitialised: false,
    cloudRegistrationState: "notAttempted",
    groupSenderKeysEnabled: false,
    groupSenderKeysReachable: false,
    remoteServiceHasNativeAccess: false,
    bootstrapAttempted: true,
    passwordGateRequired: true,
    unlocked: false,
    activeOslUserId: null,
    bootstrapStatus: "passwordRequired",
    storageMethod: null,
    ...overrides,
  };
}

function passwordSetupResult(): Record<string, unknown> {
  return {
    passwordRecoveryPhrase: "abandon ability able about above absent absorb abstract absurd abuse access accident",
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

function passwordRoleStatus(): Record<string, unknown> {
  return {
    mainPasswordSet: true,
    stealthPasswordSet: false,
    burnPasswordSet: false,
    unlocked: true,
    stealthActionWired: false,
    burnActionWired: false,
  };
}

async function loadUi(harness: CreatePasswordHarness) {
  vi.resetModules();
  const store = new Map<string, string>();
  const documentElement = new FakeElement("HTML");
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
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
    documentElement,
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
  mocks.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), close: vi.fn(async () => undefined) });
  return import("./main");
}

function commandCalls(command: string): unknown[][] {
  return mocks.invoke.mock.calls.filter((call) => call[0] === command);
}

async function bindCreatePasswordHarness(harness: CreatePasswordHarness) {
  const { __oslHubUiTest } = await loadUi(harness);
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "create", coreReady: false, bootstrapStatus: "notAttempted" });
  __oslHubUiTest.bindOnboarding();
  return __oslHubUiTest;
}

describe("TASK 0326 create-password buttons and bad match", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  it("records Show password, Confirm visibility, Create account, Back, short, and mismatch results", async () => {
    const harness = buildCreatePasswordHarness();
    const ui = await bindCreatePasswordHarness(harness);
    const longPassword = "correct horse battery staple";
    const shortPassword = "short";
    const mismatchConfirm = "correct horse battery wrong";

    console.log(`TASK0326_START accounts_recorded=${commandCalls("create_hub_osl_identity").length} route=${ui.snapshot().onboardingRoute}`);

    harness.password.value = longPassword;
    await harness.passwordEye.dispatch("click");
    console.log(`TASK0326_SHOW_PASSWORD after_first_press_type=${harness.password.type} value=${harness.password.value}`);
    await harness.passwordEye.dispatch("click");
    console.log(`TASK0326_SHOW_PASSWORD after_second_press_type=${harness.password.type} value_still=${harness.password.value}`);
    expect(harness.password.type).toBe("password");

    harness.confirm.value = longPassword;
    await harness.confirmEye.dispatch("click");
    console.log(`TASK0326_CONFIRM_SHOW after_first_press_type=${harness.confirm.type} value=${harness.confirm.value}`);
    await harness.confirmEye.dispatch("click");
    console.log(`TASK0326_CONFIRM_SHOW after_second_press_type=${harness.confirm.type} value_still=${harness.confirm.value}`);
    expect(harness.confirm.type).toBe("password");

    harness.password.value = shortPassword;
    harness.confirm.value = shortPassword;
    await harness.password.dispatch("input");
    await harness.confirm.dispatch("input");
    await harness.submit();
    console.log(`TASK0326_SHORT_REFUSED password_len=${shortPassword.length} submit_disabled=${harness.submitButton.disabled} accounts_recorded=${commandCalls("create_hub_osl_identity").length} route=${ui.snapshot().onboardingRoute}`);
    expect(commandCalls("create_hub_osl_identity")).toHaveLength(0);
    expect(ui.snapshot().onboardingRoute).toBe("create");

    harness.password.value = longPassword;
    harness.confirm.value = mismatchConfirm;
    await harness.password.dispatch("input");
    await harness.confirm.dispatch("input");
    await harness.submit();
    console.log(`TASK0326_MISMATCH_REFUSED password=${harness.password.value} confirm=${harness.confirm.value} error=${harness.error.textContent} submit_disabled=${harness.submitButton.disabled} accounts_recorded=${commandCalls("create_hub_osl_identity").length} route=${ui.snapshot().onboardingRoute}`);
    expect(harness.error.textContent).toBe("Both passwords must match.");
    expect(commandCalls("create_hub_osl_identity")).toHaveLength(0);
    expect(ui.snapshot().onboardingRoute).toBe("create");

    await harness.clickBack();
    console.log(`TASK0326_BACK route=${ui.snapshot().onboardingRoute}`);
    expect(ui.snapshot().onboardingRoute).toBe("welcome");

    ui.reset({ route: "onboarding", onboardingRoute: "create", coreReady: false, bootstrapStatus: "notAttempted" });
    harness.password.value = longPassword;
    harness.confirm.value = longPassword;
    await harness.password.dispatch("input");
    await harness.confirm.dispatch("input");
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "create_hub_osl_identity") {
        return {
          userId: "osl_task0326",
          identityRecoveryPhrase: "abandon ability able about above absent absorb abstract absurd abuse access accident",
          storageMethod: "os-keyring",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password") {
        expect(args).toEqual({ password: longPassword });
        return passwordSetupResult();
      }
      if (command === "get_core_readiness") {
        return readiness({
          originalCoreLinked: true,
          identityLoaded: true,
          keyserverInitialised: true,
          cloudRegistrationState: "registered",
          groupSenderKeysEnabled: true,
          remoteServiceHasNativeAccess: true,
          passwordGateRequired: false,
          unlocked: true,
          activeOslUserId: "osl_task0326",
          bootstrapStatus: "ready",
          storageMethod: "os-keyring",
        });
      }
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") return passwordRoleStatus();
      if (command === "set_hub_recovery_kit_unsaved") return true;
      if (command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });
    await harness.submit();
    console.log(`TASK0326_MATCHING_LONG password=${longPassword} confirm=${longPassword} accounts_recorded=${commandCalls("create_hub_osl_identity").length} route=${ui.snapshot().onboardingRoute}`);

    expect(commandCalls("create_hub_osl_identity")).toHaveLength(1);
    expect(ui.snapshot().onboardingRoute).toBe("recovery");
  }, 30_000);

  it("uses only the explicit no-recovery commands when that choice is selected", async () => {
    const harness = buildCreatePasswordHarness();
    const ui = await bindCreatePasswordHarness(harness);
    const password = "correct horse battery staple";

    harness.password.value = password;
    harness.confirm.value = password;
    harness.noRecoverySecret.checked = true;
    await harness.password.dispatch("input");
    await harness.confirm.dispatch("input");

    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "create_hub_osl_identity_without_recovery") {
        return {
          userId: "osl_task0334a",
          identityRecoveryPhrase: null,
          storageMethod: "os-keyring",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password_without_recovery") {
        expect(args).toEqual({ password });
        return {
          encryptedStateReloadComplete: true,
          encryptedStateReloadIssueCount: 0,
          readiness: passwordSetupResult().readiness,
        };
      }
      if (command === "get_core_readiness") {
        return readiness({
          originalCoreLinked: true,
          identityLoaded: true,
          keyserverInitialised: true,
          cloudRegistrationState: "registered",
          groupSenderKeysEnabled: true,
          remoteServiceHasNativeAccess: true,
          passwordGateRequired: false,
          unlocked: true,
          activeOslUserId: "osl_task0334a",
          bootstrapStatus: "ready",
          storageMethod: "os-keyring",
        });
      }
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") return passwordRoleStatus();
      if (command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });

    await harness.submit();

    const createWithoutRecovery = commandCalls("create_hub_osl_identity_without_recovery").length;
    const setupWithoutRecovery = commandCalls("setup_hub_main_password_without_recovery").length;
    const normalCreate = commandCalls("create_hub_osl_identity").length;
    const normalPassword = commandCalls("setup_hub_main_password").length;
    const recoveryStoreWrites = commandCalls("set_hub_recovery_kit_unsaved").length;
    console.log(
      `TASK0334A_NO_SECRET_UI create_without_recovery=${createWithoutRecovery} setup_without_recovery=${setupWithoutRecovery} normal_create=${normalCreate} normal_password=${normalPassword} recovery_store_writes=${recoveryStoreWrites} route=${ui.snapshot().onboardingRoute}`,
    );

    expect(createWithoutRecovery).toBe(1);
    expect(setupWithoutRecovery).toBe(1);
    expect(normalCreate).toBe(0);
    expect(normalPassword).toBe(0);
    expect(recoveryStoreWrites).toBe(0);
    expect(ui.snapshot().onboardingRoute).toBe("recovery");
  }, 30_000);
});
