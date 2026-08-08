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

type Listener = (event: Event) => void;

class FakeElement {
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  readonly classList = { add: vi.fn(), remove: vi.fn(), contains: vi.fn(() => false) };
  value = "";
  checked = false;
  disabled = false;
  open = false;
  textContent = "";
  innerHTML = "";
  className = "";
  role = "";
  private readonly listeners = new Map<string, Listener[]>();

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  async dispatch(type: string): Promise<void> {
    if (type === "click" && this.attributes.has("data-native-details")) this.open = !this.open;
    const event = { currentTarget: this, preventDefault: vi.fn() } as unknown as Event;
    for (const listener of this.listeners.get(type) ?? []) listener(event);
    for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
    if (name === "disabled") this.disabled = true;
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
    if (name === "disabled") this.disabled = false;
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  querySelector(): FakeElement | null { return null; }
  querySelectorAll(): FakeElement[] { return []; }
  append(): void {}
  prepend(): void {}
  remove(): void {}
  focus(): void {}
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

function installDom(selectors: Record<string, FakeElement[]>): void {
  const root = new FakeElement();
  const querySelector = (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null;
  vi.stubGlobal("document", {
    querySelector,
    querySelectorAll: (selector: string) => selectors[selector] ?? [],
    createElement: () => new FakeElement(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

const IDENTITY_PHRASE = "legal winner thank year wave sausage worth useful legal winner thank yellow";
const PASSWORD_PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const ACCOUNT = "OSLUSER-TASK0331";

describe("TASK0331 recovery-kit controls and word gate", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
  });

  it("calls every named control and refuses one wrong retyped word without mutating the kit", async () => {
    const copiedKits: string[] = [];
    const details = new FakeElement();
    details.setAttribute("data-native-details", "");
    const copy = new FakeElement();
    const saved = new FakeElement();
    const recoveryContinue = new FakeElement();
    recoveryContinue.disabled = true;
    const selectors: Record<string, FakeElement[]> = {
      "#recovery-account-details": [details],
      "#copy-recovery-kit": [copy],
      "#recovery-saved": [saved],
      "#recovery-continue": [recoveryContinue],
    };
    installDom(selectors);
    vi.stubGlobal("localStorage", memoryStorage());
    vi.stubGlobal("navigator", {
      clipboard: { writeText: async (kit: string) => { copiedKits.push(kit); } },
    });
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {},
      addEventListener: vi.fn(),
      matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
      setTimeout: vi.fn(() => 1),
      clearTimeout: vi.fn(),
      confirm: vi.fn(() => false),
    });
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
    vi.stubGlobal("cancelAnimationFrame", vi.fn());

    mocks.invoke.mockImplementation(async (command: string, args: unknown) => {
      if (command !== "check_hub_recovery_word_retype") return null;
      const request = (args as { request: { answers: Array<{ position: number; word: string }> } }).request;
      const expected = new Map<number, string>([[1, "abandon"], [11, "abandon"], [12, "about"]]);
      const failedPositions = request.answers
        .filter((answer) => answer.word !== expected.get(answer.position))
        .map((answer) => answer.position);
      return {
        prompts: request.answers.map(({ position }) => ({ position })),
        checkedCount: request.answers.length,
        passed: failedPositions.length === 0,
        failedPositions,
      };
    });

    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      route: "onboarding",
      onboardingRoute: "recovery",
      recoveryBundle: { userId: ACCOUNT, identityPhrase: IDENTITY_PHRASE, passwordPhrase: PASSWORD_PHRASE },
      recoveryCaptureProven: true,
    });

    const recoveryMarkup = __oslHubUiTest.renderOnboardingRoute("recovery");
    expect(recoveryMarkup).toContain('<details class="recovery-account-details" id="recovery-account-details">');
    expect(recoveryMarkup).toContain(`<code>${ACCOUNT}</code>`);
    expect(copiedKits).toHaveLength(0);
    console.info(`TASK0331_START page=recovery copied_kits=${copiedKits.length}`);

    await details.dispatch("click");
    expect(details.open).toBe(true);
    console.info(`TASK0331_ACCOUNT_DETAILS control="Account details" open=${details.open} account=${ACCOUNT}`);

    __oslHubUiTest.bindOnboarding();
    await recoveryContinue.dispatch("click");
    expect(__oslHubUiTest.recoveryKitSnapshot().onboardingRoute).toBe("recovery");
    console.info(`TASK0331_CONTINUE_BEFORE_SAVED refused=true page=${__oslHubUiTest.recoveryKitSnapshot().onboardingRoute} copied_kits=${copiedKits.length}`);

    await copy.dispatch("click");
    expect(copiedKits).toHaveLength(1);
    expect(copiedKits[0]).toContain(IDENTITY_PHRASE);
    expect(copiedKits[0]).toContain(PASSWORD_PHRASE);
    console.info(`TASK0331_COPY control="Copy recovery kit" copied_kits=${copiedKits.length} identity_phrase=true password_phrase=true`);

    saved.checked = true;
    await saved.dispatch("change");
    expect(__oslHubUiTest.recoveryKitSnapshot().savedAcknowledged).toBe(true);
    expect(recoveryContinue.disabled).toBe(false);
    console.info(`TASK0331_SAVED_TICK control="I saved my recovery kit" saved=${__oslHubUiTest.recoveryKitSnapshot().savedAcknowledged} continue_disabled=${recoveryContinue.disabled}`);

    await recoveryContinue.dispatch("click");
    expect(__oslHubUiTest.recoveryKitSnapshot().onboardingRoute).toBe("recovery-check");
    expect(__oslHubUiTest.renderOnboardingRoute("recovery-check")).toContain("Check your recovery words");
    console.info(`TASK0331_CONTINUE_AFTER_SAVED opens=${__oslHubUiTest.recoveryKitSnapshot().onboardingRoute} requested_positions=1,11,12`);

    const word1 = new FakeElement();
    word1.dataset.recoveryWordPosition = "1";
    const word11 = new FakeElement();
    word11.dataset.recoveryWordPosition = "11";
    const word12 = new FakeElement();
    word12.dataset.recoveryWordPosition = "12";
    const wordContinue = new FakeElement();
    wordContinue.disabled = true;
    const status = new FakeElement();
    selectors["[data-recovery-word-position]"] = [word1, word11, word12];
    selectors["#recovery-word-check-continue"] = [wordContinue];
    selectors["#recovery-word-check-status"] = [status];
    __oslHubUiTest.bindOnboarding();

    word1.value = "abandon";
    await word1.dispatch("input");
    word11.value = "abandon";
    await word11.dispatch("input");
    word12.value = "above";
    await word12.dispatch("input");
    expect(wordContinue.disabled).toBe(true);
    expect(status.textContent).toContain("did not match");
    expect(copiedKits).toHaveLength(1);
    expect(__oslHubUiTest.recoveryKitSnapshot()).toMatchObject({
      onboardingRoute: "recovery-check",
      bundle: { userId: ACCOUNT, identityPhrase: IDENTITY_PHRASE, passwordPhrase: PASSWORD_PHRASE },
      savedAcknowledged: true,
    });
    console.info(`TASK0331_WORD_RETYPE_WRONG changed_words=1 refused=true page=${__oslHubUiTest.recoveryKitSnapshot().onboardingRoute} copied_kits=${copiedKits.length} recovery_kit=unchanged`);

    word12.value = "about";
    await word12.dispatch("input");
    expect(wordContinue.disabled).toBe(false);
    expect(status.textContent).toBe("All requested words match.");
    await wordContinue.dispatch("click");
    expect(__oslHubUiTest.recoveryKitSnapshot().onboardingRoute).toBe("pro");
    expect(copiedKits).toHaveLength(1);
    console.info(`TASK0331_WORD_RETYPE_EXACT words=abandon,abandon,about opens=${__oslHubUiTest.recoveryKitSnapshot().onboardingRoute} copied_kits=${copiedKits.length}`);
  }, 30_000);
});
