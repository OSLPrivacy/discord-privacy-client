import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

// L-ATTR. Ledger 1 reported `data-account-recovery-phrase`
// (account-recovery.ts:106), `data-account-recovery-password`
// (account-recovery.ts:103), `data-recovery-add-phrase-wrap` and
// `data-recovery-fresh-start` (recovery-migration.ts:31) as interactive markup
// with no event-bound selector. All four were real: "Forgot password?" on the
// unlock card rendered a form whose submit did nothing at all, and the legacy
// migration screen was never rendered by any route. These tests click each
// control and assert the observable effect, not the re-render.

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Listener = (event: { currentTarget: FakeForm; preventDefault: () => void }) => void;

class FakeForm {
  readonly dataset: Record<string, string> = {};
  readonly elements: { namedItem: (name: string) => { value: string } | null };
  private readonly listeners = new Map<string, Listener[]>();

  constructor(private readonly fields: Record<string, string> = {}) {
    this.elements = { namedItem: (name) => (name in this.fields ? { value: this.fields[name] } : null) };
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  bound(type: string): boolean {
    return (this.listeners.get(type) ?? []).length > 0;
  }

  async dispatch(type: string): Promise<void> {
    for (const listener of this.listeners.get(type) ?? []) listener({ currentTarget: this, preventDefault: vi.fn() });
    // The handlers are async; let the microtask queue drain before asserting.
    await new Promise((resolve) => setTimeout(resolve, 0));
  }

  querySelectorAll(): FakeForm[] {
    return [];
  }

  querySelector(): FakeForm | null {
    return null;
  }
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

function installDom(selectors: Record<string, FakeForm[]>): void {
  const root = new FakeForm();
  const querySelector = (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null;
  vi.stubGlobal("document", {
    querySelector,
    querySelectorAll: (selector: string) => selectors[selector] ?? [],
    createElement: () => new FakeForm(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

async function loadUi(selectors: Record<string, FakeForm[]> = {}, confirmed = false) {
  vi.resetModules();
  mocks.invoke.mockReset();
  vi.stubGlobal("localStorage", memoryStorage());
  installDom(selectors);
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout: vi.fn(() => 1), clearTimeout: vi.fn(), confirm: vi.fn(() => confirmed) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

const lockout = { passwordLockedUntil: null, passwordAttemptsUsed: 0, phraseLockedUntil: null, phraseAttemptsUsed: 1, now: 0 };

describe("L-ATTR account recovery bindings", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("submits the recovery phrase form and advances to the new-password step", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "  correct horse battery staple  " });
    const verifyPhrase = vi.fn(async () => ({ ok: true as const, recoveryToken: "token-1", lockoutStatus: lockout }));
    const setPassword = vi.fn(async () => undefined);
    const { __oslHubUiTest } = await loadUi({ "[data-account-recovery-phrase]": [phraseForm] });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies({ verifyPhrase, setPassword });
    __oslHubUiTest.bindOnboarding();

    expect(phraseForm.bound("submit")).toBe(true);
    await phraseForm.dispatch("submit");

    expect(verifyPhrase).toHaveBeenCalledWith("correct horse battery staple");
    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({ step: "password", error: null });
    const markup = __oslHubUiTest.renderOnboardingRoute("account-recovery");
    expect(markup).toContain("data-account-recovery-password");
    expect(markup).toContain("Choose a new password");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("sets the recovered password through the token the phrase step returned", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "correct horse battery staple" });
    const passwordForm = new FakeForm({ newPassword: "brand-new-pass", confirmPassword: "brand-new-pass" });
    const setPassword = vi.fn(async () => undefined);
    const { __oslHubUiTest } = await loadUi({
      "[data-account-recovery-phrase]": [phraseForm],
      "[data-account-recovery-password]": [passwordForm],
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies({
      verifyPhrase: async () => ({ ok: true as const, recoveryToken: "token-9", lockoutStatus: lockout }),
      setPassword,
    });
    __oslHubUiTest.bindOnboarding();

    await phraseForm.dispatch("submit");
    expect(passwordForm.bound("submit")).toBe(true);
    await passwordForm.dispatch("submit");

    expect(setPassword).toHaveBeenCalledWith("brand-new-pass", "token-9");
    expect(__oslHubUiTest.accountRecoverySnapshot().step).toBe("complete");
    expect(__oslHubUiTest.renderOnboardingRoute("account-recovery")).toContain("Your password was reset");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("refuses a mismatched confirmation without calling the back end", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "correct horse battery staple" });
    const passwordForm = new FakeForm({ newPassword: "brand-new-pass", confirmPassword: "different-pass" });
    const setPassword = vi.fn(async () => undefined);
    const { __oslHubUiTest } = await loadUi({
      "[data-account-recovery-phrase]": [phraseForm],
      "[data-account-recovery-password]": [passwordForm],
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies({
      verifyPhrase: async () => ({ ok: true as const, recoveryToken: "token-9", lockoutStatus: lockout }),
      setPassword,
    });
    __oslHubUiTest.bindOnboarding();

    await phraseForm.dispatch("submit");
    await passwordForm.dispatch("submit");

    expect(setPassword).not.toHaveBeenCalled();
    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({ step: "password", error: "The new passwords do not match." });
    expect(__oslHubUiTest.renderOnboardingRoute("account-recovery")).toContain("The new passwords do not match.");
  });

  it("shows the lockout count a rejected phrase returns instead of staying silent", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "wrong words" });
    const { __oslHubUiTest } = await loadUi({ "[data-account-recovery-phrase]": [phraseForm] });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies({
      verifyPhrase: async () => ({ ok: false as const, lockoutStatus: { ...lockout, phraseAttemptsUsed: 3, phraseLockedUntil: 60, now: 0 } }),
      setPassword: vi.fn(),
    });
    __oslHubUiTest.bindOnboarding();

    await phraseForm.dispatch("submit");

    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({ step: "phrase" });
    expect(__oslHubUiTest.renderOnboardingRoute("account-recovery")).toContain("3 recovery phrase attempts recorded. Try again in 60 seconds.");
  });

  it("renders the legacy migration screen for a legacy-marker refusal and repairs the marker", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "correct horse battery staple" });
    const wrapForm = new FakeForm({ currentPassword: "the-current-password" });
    const addPhraseWrap = vi.fn(async () => undefined);
    const freshStart = vi.fn(async () => undefined);
    const { __oslHubUiTest } = await loadUi({
      "[data-account-recovery-phrase]": [phraseForm],
      "[data-recovery-add-phrase-wrap]": [wrapForm],
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies(
      { verifyPhrase: async () => { throw new Error("Cannot complete recovery for this account"); }, setPassword: vi.fn() },
      { addPhraseWrap, freshStart },
    );
    __oslHubUiTest.bindOnboarding();

    await phraseForm.dispatch("submit");

    expect(__oslHubUiTest.accountRecoverySnapshot().migration).toBe("needs-current-password");
    const migration = __oslHubUiTest.renderOnboardingRoute("account-recovery");
    expect(migration).toContain("data-recovery-add-phrase-wrap");
    expect(migration).toContain("data-recovery-fresh-start");
    expect(migration).toContain("This account needs its current password");

    expect(wrapForm.bound("submit")).toBe(true);
    await wrapForm.dispatch("submit");
    expect(addPhraseWrap).toHaveBeenCalledWith("the-current-password");
    expect(freshStart).not.toHaveBeenCalled();
    // Repaired: the phrase alone works again, so the migration screen is gone.
    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({ migration: null, step: "phrase" });
  });

  it("takes the fresh-start path only after the destructive confirmation is accepted", async () => {
    const freshStartButton = new FakeForm();
    const freshStart = vi.fn(async () => undefined);
    const { __oslHubUiTest } = await loadUi({ "[data-recovery-fresh-start]": [freshStartButton] }, false);
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies(
      { verifyPhrase: vi.fn(), setPassword: vi.fn() },
      { addPhraseWrap: vi.fn(), freshStart },
    );
    __oslHubUiTest.bindOnboarding();

    expect(freshStartButton.bound("click")).toBe(true);
    await freshStartButton.dispatch("click");
    expect(freshStart).not.toHaveBeenCalled();

    const accepted = new FakeForm();
    const acceptedFreshStart = vi.fn(async () => undefined);
    const second = await loadUi({ "[data-recovery-fresh-start]": [accepted] }, true);
    second.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    second.__oslHubUiTest.setAccountRecoveryDependencies(
      { verifyPhrase: vi.fn(), setPassword: vi.fn() },
      { addPhraseWrap: vi.fn(), freshStart: acceptedFreshStart },
    );
    second.__oslHubUiTest.bindOnboarding();
    await accepted.dispatch("click");

    expect(acceptedFreshStart).toHaveBeenCalledTimes(1);
    expect(second.__oslHubUiTest.accountRecoverySnapshot().migration).toBe("fresh-start");
    expect(second.__oslHubUiTest.renderOnboardingRoute("account-recovery")).toContain("Start over");
  });

  it("starts every visit to recovery at the phrase step with no inherited token", async () => {
    const phraseForm = new FakeForm({ recoveryPhrase: "correct horse battery staple" });
    const { __oslHubUiTest } = await loadUi({ "[data-account-recovery-phrase]": [phraseForm] });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.setAccountRecoveryDependencies({
      verifyPhrase: async () => ({ ok: true as const, recoveryToken: "token-1", lockoutStatus: lockout }),
      setPassword: vi.fn(),
    });
    __oslHubUiTest.bindOnboarding();
    await phraseForm.dispatch("submit");
    expect(__oslHubUiTest.accountRecoverySnapshot().step).toBe("password");

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({ step: "phrase", migration: null });
    expect(mainSource).toContain('if (onboardingRoute === "account-recovery") resetAccountRecovery();');
  });

  it("keeps the shipping build free of a password-recovery IPC command", async () => {
    // The forms are bound, but nothing behind them exists yet: no Tauri command
    // turns a recovery phrase into a token. The shipping dependency therefore
    // fails closed and says so, rather than the button doing nothing at all.
    const phraseForm = new FakeForm({ recoveryPhrase: "correct horse battery staple" });
    const { __oslHubUiTest } = await loadUi({ "[data-account-recovery-phrase]": [phraseForm] });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "account-recovery" });
    __oslHubUiTest.bindOnboarding();

    await phraseForm.dispatch("submit");

    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(__oslHubUiTest.accountRecoverySnapshot()).toMatchObject({
      step: "phrase",
      error: "We could not verify that recovery phrase. No password was changed.",
    });
    expect(__oslHubUiTest.renderOnboardingRoute("account-recovery")).toContain("We could not verify that recovery phrase.");
  });
});

describe("L-ATTR dead markup and dead listeners", () => {
  it("no longer emits data-whitelist-scope-add, which no handler ever read", () => {
    // Both "+" buttons in the whitelist roster are unconditionally `disabled`
    // and their personId was never read by anything. Removing the attribute
    // removes the dead markup; the disabled affordance and its tooltip stay.
    expect(mainSource).not.toContain("data-whitelist-scope-add");
    expect(mainSource).toContain("data-whitelist-scope-remove=");
    expect(mainSource).toContain('inDomTooltipMarkup("Already approved")');
  });

  it("no longer registers listeners on attributes no markup writes", () => {
    expect(mainSource).not.toContain('"[data-saved-account-mode]"');
    expect(mainSource).not.toContain('"[data-service]"');
    // The live neighbours that share the prefix must survive.
    expect(mainSource).toContain('"[data-service-current-session]"');
    expect(mainSource).toContain('"[data-service-account]"');
    expect(mainSource).toContain('"[data-saved-native]"');
  });
});
