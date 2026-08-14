import { beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: native.emitTo, listen: native.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: native.getCurrentWindow }));

class FakeElement {
  value = "";
  type = "";
  disabled = false;
  textContent = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Array<(event: unknown) => unknown>>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  private markup = "";
  onMarkup: ((markup: string) => void) | null = null;

  constructor(readonly tagName: string, readonly id = "") {}

  get innerHTML(): string { return this.markup; }
  set innerHTML(value: string) { this.markup = value; this.onMarkup?.(value); }
  querySelector(_selector: string): null { return null; }
  querySelectorAll(_selector: string): [] { return []; }
  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }
  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  removeAttribute(name: string): void { this.attributes.delete(name); }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  focus(): void {}
  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) await handler({ preventDefault: () => undefined, currentTarget: this });
  }
}

function harness() {
  let page: "keylost" | "import" = "keylost";
  const app = new FakeElement("DIV", "app");
  app.onMarkup = (markup) => { if (markup.includes('id="identity-import-form"')) page = "import"; };
  const restoreWithPhrase = new FakeElement("BUTTON");
  restoreWithPhrase.dataset.onboarding = "import";
  const form = new FakeElement("FORM", "identity-import-form");
  const phrase = new FakeElement("TEXTAREA", "identity-recovery-phrase");
  const password = new FakeElement("INPUT", "import-password");
  const confirm = new FakeElement("INPUT", "import-password-confirm");
  const submit = new FakeElement("BUTTON", "identity-import-submit"); submit.disabled = true;
  const error = new FakeElement("P", "import-error");
  const back = new FakeElement("BUTTON"); back.dataset.onboarding = "welcome";
  const nodes = new Map<string, FakeElement>([
    ["#app", app], ["#identity-import-form", form], ["#identity-recovery-phrase", phrase],
    ["#import-password", password], ["#import-password-confirm", confirm],
    ["#identity-import-submit", submit], ["#import-error", error],
  ]);
  return {
    app, restoreWithPhrase, form, phrase, password, confirm, submit, error,
    node(selector: string) { return selector === "#app" ? app : page === "import" ? nodes.get(selector) ?? null : null; },
    all(selector: string) { return selector === "[data-onboarding]" ? (page === "keylost" ? [restoreWithPhrase] : [back]) : []; },
  };
}

const phrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
const oneWordChanged = "abandon ability able about above absent absorb abstract absurd abuse access abandon";
const password = "correct horse battery staple";

function readiness(ready: boolean): Record<string, unknown> {
  return {
    originalCoreLinked: ready, identityLoaded: ready, keyserverInitialised: ready,
    cloudRegistrationState: ready ? "registered" : "notAttempted", groupSenderKeysEnabled: ready,
    groupSenderKeysReachable: ready, remoteServiceHasNativeAccess: ready, bootstrapAttempted: ready,
    passwordGateRequired: !ready, unlocked: ready, activeOslUserId: ready ? "osl-task0330" : null,
    bootstrapStatus: ready ? "ready" : "identityKeyLost", storageMethod: ready ? "os-keyring" : null,
  };
}

async function load(h: ReturnType<typeof harness>) {
  vi.resetModules();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key), clear: () => storage.clear() });
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => h.node(selector)), querySelectorAll: vi.fn((selector: string) => h.all(selector)),
    getElementById: vi.fn((id: string) => h.node(`#${id}`)), createElement: vi.fn((tag: string) => new FakeElement(tag.toUpperCase())),
    body: { append: vi.fn() }, documentElement: new FakeElement("HTML"), addEventListener: vi.fn(), visibilityState: "visible", activeElement: null,
  });
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => { callback(0); return 1; }));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  native.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), close: vi.fn(async () => undefined) });
  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "keylost", coreReady: false, bootstrapStatus: "identityKeyLost" });
  const keyLostMarkup = __oslHubUiTest.renderOnboardingRoute("keylost");
  __oslHubUiTest.bindOnboarding();
  return { ui: __oslHubUiTest, keyLostMarkup };
}

async function openRestore(h: ReturnType<typeof harness>, ui: Awaited<ReturnType<typeof load>>["ui"]): Promise<void> {
  expect(h.restoreWithPhrase.handlers.get("click") ?? []).toHaveLength(1);
  await h.restoreWithPhrase.dispatch("click");
  expect(ui.snapshot()).toMatchObject({ route: "onboarding", onboardingRoute: "import" });
  expect(ui.renderOnboardingRoute("import")).toContain("Restore your account");
  console.log(`TASK0330_RESTORE_WITH_RECOVERY_PHRASE route=${ui.snapshot().onboardingRoute} page=Restore`);
}

async function fillAndRestore(h: ReturnType<typeof harness>, recoveryPhrase: string): Promise<void> {
  h.phrase.value = recoveryPhrase; h.password.value = password; h.confirm.value = password;
  await h.phrase.dispatch("input"); await h.password.dispatch("input"); await h.confirm.dispatch("input");
  expect(h.submit.disabled).toBe(false);
  await h.form.dispatch("submit");
}

describe("TASK 0330 device-key-lost recovery restore controls", () => {
  beforeEach(() => { vi.unstubAllGlobals(); native.invoke.mockReset(); native.listen.mockReset(); native.emitTo.mockReset(); native.getCurrentWindow.mockReset(); });

  it("routes from the lost-key page and records only the exact available recovery-service restore", async () => {
    const recordedRestoreAttempts: string[] = [];
    let recoveryServiceAvailable = true;
    let currentFlowRecovered = false;
    native.invoke.mockImplementation(async (command: string, args?: { recoveryPhrase?: string }) => {
      if (command === "import_hub_osl_identity_phrase") {
        if (!recoveryServiceAvailable) throw new Error("Recovery service is unavailable.");
        if (args?.recoveryPhrase !== phrase) throw new Error("Recovery phrase was refused.");
        recordedRestoreAttempts.push(args.recoveryPhrase);
        currentFlowRecovered = true;
        return { userId: "osl-task0330", identityRecoveryPhrase: null, storageMethod: "os-keyring", passwordSetupRequired: true };
      }
      if (command === "setup_hub_main_password") return { passwordRecoveryPhrase: phrase, encryptedStateReloadComplete: true, encryptedStateReloadIssueCount: 0, readiness: { accessState: "ready", identityLoaded: true, mainPasswordSet: true, unlocked: true, serviceNeutralIdentitySupported: true, canCreateIdentity: false, canImportIdentityPhrase: true, passwordAttemptsUsed: 0, passwordLockoutSecondsRemaining: 0 } };
      if (command === "get_core_readiness") return readiness(currentFlowRecovered);
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") return { mainPasswordSet: true, stealthPasswordSet: false, burnPasswordSet: false, unlocked: true, stealthActionWired: false, burnActionWired: false };
      if (command === "set_hub_recovery_kit_unsaved" || command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });

    const success = harness();
    const successPage = await load(success);
    expect(successPage.keyLostMarkup).toContain("Restore with recovery phrase");
    expect(successPage.ui.snapshot()).toMatchObject({ route: "onboarding", onboardingRoute: "keylost" });
    expect(recordedRestoreAttempts).toHaveLength(0);
    console.log(`TASK0330_START page=device-key-lost restore_attempts=${recordedRestoreAttempts.length}`);
    await openRestore(success, successPage.ui);
    await fillAndRestore(success, phrase);
    console.log(`TASK0330_RESTORE available=true exact_phrase=true restore_attempts=${recordedRestoreAttempts.length} route=${successPage.ui.snapshot().onboardingRoute}`);
    expect(recordedRestoreAttempts).toEqual([phrase]);
    expect(successPage.ui.snapshot().onboardingRoute).toBe("recovery");

    recoveryServiceAvailable = false;
    currentFlowRecovered = false;
    const unavailable = harness();
    const unavailablePage = await load(unavailable);
    await openRestore(unavailable, unavailablePage.ui);
    await fillAndRestore(unavailable, phrase);
    console.log(`TASK0330_RESTORE recovery_service_refusal="${unavailable.error.textContent}" restore_attempts=${recordedRestoreAttempts.length} route=${unavailablePage.ui.snapshot().onboardingRoute}`);
    expect(unavailable.error.textContent).toBe("Recovery service is unavailable.");
    expect(recordedRestoreAttempts).toHaveLength(1);
    expect(unavailablePage.ui.snapshot().onboardingRoute).toBe("import");

    recoveryServiceAvailable = true;
    currentFlowRecovered = false;
    const changedWord = harness();
    const changedWordPage = await load(changedWord);
    await openRestore(changedWord, changedWordPage.ui);
    await fillAndRestore(changedWord, oneWordChanged);
    console.log(`TASK0330_RESTORE changed_one_word_refusal="${changedWord.error.textContent}" restore_attempts=${recordedRestoreAttempts.length} route=${changedWordPage.ui.snapshot().onboardingRoute} page=Restore`);
    expect(changedWord.error.textContent).toBe("Recovery phrase was refused.");
    expect(recordedRestoreAttempts).toHaveLength(1);
    expect(changedWordPage.ui.snapshot().onboardingRoute).toBe("import");
  }, 30_000);
});
