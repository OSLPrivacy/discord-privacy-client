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
  checked = false;
  textContent = "";
  innerHTML = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  elements = { namedItem: (_name: string): FakeElement | null => null };
  handlers = new Map<string, (event: { currentTarget: FakeElement; preventDefault: () => void }) => unknown>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };

  constructor(readonly tagName: string, readonly id = "") {}

  addEventListener(type: string, handler: (event: { currentTarget: FakeElement; preventDefault: () => void }) => unknown): void {
    // render() replaces these nodes in the real DOM. Replacing the handler here
    // models that lifecycle while retaining simple references for assertions.
    this.handlers.set(type, handler);
  }
  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  querySelector(): FakeElement | null { return null; }
  querySelectorAll(): FakeElement[] { return []; }
  focus(): void {}
  async dispatch(type: string): Promise<void> {
    await this.handlers.get(type)?.({ preventDefault: () => undefined, currentTarget: this });
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function harness() {
  const app = new FakeElement("DIV", "app");
  const phraseForm = new FakeElement("FORM");
  const phrase = new FakeElement("TEXTAREA", "account-recovery-phrase");
  phraseForm.elements = { namedItem: (name) => name === "recoveryPhrase" ? phrase : null };
  const passwordForm = new FakeElement("FORM");
  const password = new FakeElement("INPUT", "account-recovery-new-password"); password.type = "password";
  const confirm = new FakeElement("INPUT", "account-recovery-confirm-password"); confirm.type = "password";
  passwordForm.elements = { namedItem: (name) => name === "newPassword" ? password : name === "confirmPassword" ? confirm : null };
  const continueButton = new FakeElement("BUTTON", "account-recovery-continue"); continueButton.disabled = true;
  const passwordEye = new FakeElement("BUTTON"); passwordEye.dataset.passwordToggle = password.id;
  const confirmEye = new FakeElement("BUTTON"); confirmEye.dataset.passwordToggle = confirm.id;
  const back = new FakeElement("BUTTON"); back.dataset.accountRecoveryBack = "";
  const phraseBack = new FakeElement("BUTTON"); phraseBack.dataset.onboarding = "unlock";
  const nodes = new Map<string, FakeElement>([
    ["#app", app],
    ["[data-account-recovery-phrase]", phraseForm],
    ["#account-recovery-phrase", phrase],
    ["[data-account-recovery-password]", passwordForm],
    ["#account-recovery-new-password", password],
    ["#account-recovery-confirm-password", confirm],
    ["#account-recovery-continue", continueButton],
    ["[data-account-recovery-back]", back],
  ]);
  return {
    app, phraseForm, phrase, passwordForm, password, confirm, continueButton, passwordEye, confirmEye, back, phraseBack, nodes,
    all(selector: string): FakeElement[] {
      if (selector === "[data-password-toggle]") return [passwordEye, confirmEye];
      if (selector === "[data-onboarding]") return [phraseBack];
      return [];
    },
  };
}

function commandCalls(command: string): unknown[][] {
  return mocks.invoke.mock.calls.filter((call) => call[0] === command);
}

async function load(h: ReturnType<typeof harness>) {
  vi.resetModules();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  });
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => h.nodes.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => h.all(selector)),
    getElementById: vi.fn((id: string) => h.nodes.get(`#${id}`) ?? null),
    createElement: vi.fn((tag: string) => new FakeElement(tag.toUpperCase())),
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
  mocks.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), close: vi.fn(async () => undefined) });
  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
  const initialMarkup = __oslHubUiTest.renderOnboardingRoute("account-recovery");
  __oslHubUiTest.bindOnboarding();
  return { ui: __oslHubUiTest, initialMarkup };
}

const lockoutStatus = {
  passwordLockedUntil: null,
  passwordAttemptsUsed: 0,
  phraseLockedUntil: null,
  phraseAttemptsUsed: 0,
  now: 0,
};

describe("TASK 0303 password reset buttons", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  it("gates Continue on the exact checked phrase and matching new passwords", async () => {
    const h = harness();
    const correctPhrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
    const changedPhrase = "ability ability able about above absent absorb abstract absurd abuse access accident";
    const replacement = "new-password-0303";
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "check_hub_password_reset_phrase") {
        const approved = (args as { phrase?: string })?.phrase === correctPhrase;
        return {
          status: approved ? "approved" : "refused",
          recoveryToken: approved ? "task-0303-token" : null,
          lockoutStatus: {
            password_locked_until: lockoutStatus.passwordLockedUntil,
            password_attempts_used: lockoutStatus.passwordAttemptsUsed,
            phrase_locked_until: lockoutStatus.phraseLockedUntil,
            phrase_attempts_used: approved ? 0 : 1,
            now: lockoutStatus.now,
          },
        };
      }
      if (command === "reset_hub_main_password_after_recovery") {
        expect(args).toEqual({ recoveryPhrase: correctPhrase, newPassword: replacement });
        return {
          accessState: "ready", identityLoaded: true, mainPasswordSet: true, unlocked: true,
          serviceNeutralIdentitySupported: true, canCreateIdentity: false, canImportIdentityPhrase: true,
          passwordAttemptsUsed: 0, passwordLockoutSecondsRemaining: 0,
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { ui, initialMarkup } = await load(h);
    const checks = () => commandCalls("check_hub_password_reset_phrase").length;
    const resets = () => commandCalls("reset_hub_main_password_after_recovery").length;

    expect(initialMarkup).not.toContain("account-recovery-continue");
    h.password.value = replacement;
    h.confirm.value = replacement;
    h.continueButton.disabled = false;
    await h.passwordForm.dispatch("submit");
    console.log(`TASK0303_BEFORE_CHECK unavailable=${h.continueButton.disabled} checks=${checks()} resets=${resets()} direct_refused=${resets() === 0}`);
    expect(h.continueButton.disabled).toBe(true);
    expect(checks()).toBe(0);
    expect(resets()).toBe(0);

    h.phrase.value = changedPhrase;
    await h.phraseForm.dispatch("submit");
    console.log(`TASK0303_CHANGED_PHRASE step=${ui.accountRecoverySnapshot().step} checks=${checks()} resets=${resets()}`);
    expect(ui.accountRecoverySnapshot().step).toBe("phrase");
    expect(checks()).toBe(1);
    expect(resets()).toBe(0);

    h.phrase.value = correctPhrase;
    await h.phraseForm.dispatch("submit");
    const passwordMarkup = ui.renderOnboardingRoute("account-recovery");
    console.log(`TASK0303_AFTER_EXACT step=${ui.accountRecoverySnapshot().step} checks=${checks()} password_markup=${passwordMarkup.includes("account-recovery-continue")}`);
    const controls = {
      newPassword: (passwordMarkup.match(/id="account-recovery-new-password"/gu) ?? []).length,
      confirmPassword: (passwordMarkup.match(/id="account-recovery-confirm-password"/gu) ?? []).length,
      show: (passwordMarkup.match(/data-password-toggle=/gu) ?? []).length,
      continue: (passwordMarkup.match(/id="account-recovery-continue"/gu) ?? []).length,
      back: (passwordMarkup.match(/data-account-recovery-back/gu) ?? []).length,
    };
    console.log(`TASK0303_CONTROLS new_password=${controls.newPassword} confirm_password=${controls.confirmPassword} show=${controls.show} continue=${controls.continue} back=${controls.back}`);
    expect(controls).toEqual({ newPassword: 1, confirmPassword: 1, show: 2, continue: 1, back: 1 });
    expect(ui.accountRecoverySnapshot().step).toBe("password");
    expect(checks()).toBe(2);

    h.password.value = replacement;
    h.confirm.value = "mismatched-password";
    await h.password.dispatch("input");
    await h.confirm.dispatch("input");
    const mismatchUnavailable = h.continueButton.disabled;
    h.continueButton.disabled = false;
    await h.passwordForm.dispatch("submit");
    console.log(`TASK0303_MISMATCH unavailable=${mismatchUnavailable} forced_unavailable=${h.continueButton.disabled} resets=${resets()} direct_refused=${resets() === 0}`);
    expect(mismatchUnavailable).toBe(true);
    expect(h.continueButton.disabled).toBe(true);
    expect(resets()).toBe(0);

    h.confirm.value = replacement;
    await h.confirm.dispatch("input");
    const availableBeforeShow = !h.continueButton.disabled;
    await h.passwordEye.dispatch("click");
    await h.confirmEye.dispatch("click");
    console.log(`TASK0303_SHOW password=${h.password.type} confirm=${h.confirm.type} availability_unchanged=${!h.continueButton.disabled === availableBeforeShow}`);
    expect(h.password.type).toBe("text");
    expect(h.confirm.type).toBe("text");
    expect(h.continueButton.disabled).toBe(false);

    await h.back.dispatch("click");
    console.log(`TASK0303_BACK step=${ui.accountRecoverySnapshot().step} unavailable=${h.continueButton.disabled} resets=${resets()}`);
    expect(ui.accountRecoverySnapshot().step).toBe("phrase");
    expect(h.continueButton.disabled).toBe(true);
    expect(resets()).toBe(0);

    h.phrase.value = correctPhrase;
    await h.phraseForm.dispatch("submit");
    h.password.type = "password";
    h.confirm.type = "password";
    h.password.value = replacement;
    h.confirm.value = replacement;
    await h.password.dispatch("input");
    await h.confirm.dispatch("input");
    console.log(`TASK0303_READY exact_phrase_checks=${checks()} matching_passwords=${h.password.value === h.confirm.value} available=${!h.continueButton.disabled}`);
    expect(h.continueButton.disabled).toBe(false);

    await h.passwordForm.dispatch("submit");
    console.log(`TASK0303_SUCCESS resets=${resets()} step=${ui.accountRecoverySnapshot().step} new_password_cleared=${h.password.value === ""} confirm_password_cleared=${h.confirm.value === ""}`);
    expect(resets()).toBe(1);
    expect(ui.accountRecoverySnapshot().step).toBe("complete");
    expect(h.password.value).toBe("");
    expect(h.confirm.value).toBe("");
  }, 30_000);

  it("TASK 0328 records phrase Continue, password Continue, both Back controls, and one changed word", async () => {
    const h = harness();
    const savedRecoveryPhrases = ["abandon ability able about above absent absorb abstract absurd abuse access accident"];
    const changedRecoveryPhrase = "abandon ability able about above absent absorb abstract absurd abuse access action";
    const longPassword = "TASK-0328 matching long password";
    const resetPasswords: string[] = [];

    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "check_hub_password_reset_phrase") {
        const phrase = (args as { phrase?: string })?.phrase ?? "";
        const approved = savedRecoveryPhrases.includes(phrase);
        return {
          status: approved ? "approved" : "refused",
          recoveryToken: approved ? "task-0328-token" : null,
          lockoutStatus: {
            password_locked_until: null,
            password_attempts_used: 0,
            phrase_locked_until: null,
            phrase_attempts_used: approved ? 0 : 1,
            now: 0,
          },
        };
      }
      if (command === "reset_hub_main_password_after_recovery") {
        const reset = args as { recoveryPhrase?: string; newPassword?: string };
        expect(reset.recoveryPhrase).toBe(savedRecoveryPhrases[0]);
        expect(reset.newPassword).toBe(longPassword);
        resetPasswords.push(reset.newPassword ?? "");
        return {
          accessState: "ready", identityLoaded: true, mainPasswordSet: true, unlocked: true,
          serviceNeutralIdentitySupported: true, canCreateIdentity: false, canImportIdentityPhrase: true,
          passwordAttemptsUsed: 0, passwordLockoutSecondsRemaining: 0,
        };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { ui, initialMarkup } = await load(h);
    expect(savedRecoveryPhrases).toHaveLength(1);
    expect(resetPasswords).toHaveLength(0);
    expect(initialMarkup).toContain('id="account-recovery-phrase-continue"');
    expect(initialMarkup).toContain('<span>Continue</span>');
    expect(initialMarkup).toContain('data-onboarding="unlock"');
    console.log(`TASK0328_INITIAL saved_recovery_phrases=${savedRecoveryPhrases.length} reset_passwords=${resetPasswords.length} heading=${initialMarkup.includes(">Password reset<")} phrase_control=Continue`);

    h.phrase.value = changedRecoveryPhrase;
    await h.phraseForm.dispatch("submit");
    const refusedMarkup = ui.renderOnboardingRoute("account-recovery");
    console.log(`TASK0328_CHANGED_WORD result=refused step=${ui.accountRecoverySnapshot().step} reset_passwords=${resetPasswords.length} heading=${refusedMarkup.includes(">Password reset<")} phrase_unchanged=${ui.snapshot().onboardingRoute === "account-recovery"}`);
    expect(ui.accountRecoverySnapshot().step).toBe("phrase");
    expect(ui.snapshot().onboardingRoute).toBe("account-recovery");
    expect(refusedMarkup).toContain(">Password reset<");
    expect(resetPasswords).toHaveLength(0);

    await h.phraseBack.dispatch("click");
    const phraseBackMarkup = ui.renderOnboardingRoute(ui.snapshot().onboardingRoute);
    console.log(`TASK0328_PHRASE_BACK result_route=${ui.snapshot().onboardingRoute} unlock_control=${phraseBackMarkup.includes(">Unlock<")} reset_passwords=${resetPasswords.length}`);
    expect(ui.snapshot().onboardingRoute).toBe("unlock");
    expect(phraseBackMarkup).toContain(">Unlock<");
    expect(resetPasswords).toHaveLength(0);

    ui.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    ui.bindOnboarding();
    h.phrase.value = savedRecoveryPhrases[0];
    await h.phraseForm.dispatch("submit");
    const firstPasswordMarkup = ui.renderOnboardingRoute("account-recovery");
    console.log(`TASK0328_PHRASE_CONTINUE phrase=exact result_step=${ui.accountRecoverySnapshot().step} password_heading=${firstPasswordMarkup.includes(">Choose a new password<")} reset_passwords=${resetPasswords.length}`);
    expect(ui.accountRecoverySnapshot().step).toBe("password");
    expect(firstPasswordMarkup).toContain(">Choose a new password<");
    expect(firstPasswordMarkup).toContain('id="account-recovery-continue"');
    expect(firstPasswordMarkup).toContain('<span>Continue</span>');
    expect(resetPasswords).toHaveLength(0);

    await h.back.dispatch("click");
    const passwordBackMarkup = ui.renderOnboardingRoute("account-recovery");
    console.log(`TASK0328_PASSWORD_BACK result_step=${ui.accountRecoverySnapshot().step} heading=${passwordBackMarkup.includes(">Password reset<")} reset_passwords=${resetPasswords.length}`);
    expect(ui.accountRecoverySnapshot().step).toBe("phrase");
    expect(passwordBackMarkup).toContain(">Password reset<");
    expect(resetPasswords).toHaveLength(0);

    h.phrase.value = savedRecoveryPhrases[0];
    await h.phraseForm.dispatch("submit");
    h.password.value = longPassword;
    h.confirm.value = longPassword;
    await h.password.dispatch("input");
    await h.confirm.dispatch("input");
    const matchingLongPasswords = h.password.value === h.confirm.value && h.password.value.length >= 12;
    expect(h.continueButton.disabled).toBe(false);
    expect(matchingLongPasswords).toBe(true);
    await h.passwordForm.dispatch("submit");
    const unlockMarkup = ui.renderOnboardingRoute(ui.snapshot().onboardingRoute);
    console.log(`TASK0328_PASSWORD_CONTINUE matching_long=${matchingLongPasswords} reset_passwords=${resetPasswords.length} result_route=${ui.snapshot().onboardingRoute} unlock_control=${unlockMarkup.includes(">Unlock<")}`);
    expect(resetPasswords).toEqual([longPassword]);
    expect(ui.accountRecoverySnapshot().step).toBe("complete");
    expect(ui.snapshot().onboardingRoute).toBe("unlock");
    expect(unlockMarkup).toContain(">Unlock<");
  }, 30_000);
});
