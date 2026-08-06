import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Listener = (event: { currentTarget: FakeElement; preventDefault: () => void }) => void;

class FakeElement {
  readonly dataset: Record<string, string>;
  checked = false;
  disabled = false;
  value = "";
  innerHTML = "";
  private readonly listeners = new Map<string, Listener>();

  constructor(options: { checked?: boolean; value?: string; dataset?: Record<string, string> } = {}) {
    this.checked = options.checked ?? false;
    this.value = options.value ?? "";
    this.dataset = options.dataset ?? {};
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, listener);
  }

  dispatch(type: string): void {
    this.listeners.get(type)?.({ currentTarget: this, preventDefault: vi.fn() });
  }

  querySelector(): FakeElement | null {
    return null;
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  focus(): void {
    return undefined;
  }
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

function installDom(selectors: Record<string, FakeElement[]>): FakeElement {
  const root = new FakeElement();
  const querySelector = (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null;
  const querySelectorAll = (selector: string) => selectors[selector] ?? [];
  vi.stubGlobal("document", {
    querySelector,
    querySelectorAll,
    getElementById: vi.fn(() => null),
    createElement: () => new FakeElement(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  return root;
}

async function loadUi(selectors: Record<string, FakeElement[]>, savedPreferences: unknown[]) {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string, args?: { preferences?: unknown }) => {
    if (command === "save_onboarding_preferences") {
      savedPreferences.push(args?.preferences);
      return args?.preferences;
    }
    return null;
  });
  vi.stubGlobal("localStorage", memoryStorage());
  installDom(selectors);
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  const ui = await import("./main");
  return { ui };
}

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("TASK0340 forward-secrecy choice controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("records both choices, Continue, Back, and an out-of-range refusal", async () => {
    const protectPast = new FakeElement({ value: "protect-past" });
    const keepGroupDelivery = new FakeElement({ value: "keep-group-delivery" });
    const outsideChoice = new FakeElement({ value: "archive-forever" });
    const continueButton = new FakeElement();
    const backButton = new FakeElement();
    const savedPreferences: unknown[] = [];
    const { ui } = await loadUi({
      'input[name="forward-secrecy-mode"]': [protectPast, keepGroupDelivery, outsideChoice],
      "[data-forward-secrecy-continue]": [continueButton],
      "#onboarding-back": [backButton],
    }, savedPreferences);
    const { __oslHubUiTest } = ui;

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "forward-secrecy", forwardSecrecyChoice: null });
    const initial = __oslHubUiTest.renderOnboardingRoute("forward-secrecy");
    __oslHubUiTest.bindOnboarding();
    expect(__oslHubUiTest.snapshot().forwardSecrecyChoice).toBeNull();
    expect(initial).toContain("data-forward-secrecy-continue disabled");
    console.log(`TASK0340 initial_route=${__oslHubUiTest.snapshot().onboardingRoute} initial_choice=${__oslHubUiTest.snapshot().forwardSecrecyChoice}`);

    protectPast.checked = true;
    protectPast.dispatch("change");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "forward-secrecy", forwardSecrecyChoice: "protect-past" });
    console.log(`TASK0340 choice_1_saved=${__oslHubUiTest.snapshot().forwardSecrecyChoice}`);

    keepGroupDelivery.checked = true;
    keepGroupDelivery.dispatch("change");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "forward-secrecy", forwardSecrecyChoice: "keep-group-delivery" });
    console.log(`TASK0340 choice_2_saved=${__oslHubUiTest.snapshot().forwardSecrecyChoice}`);

    const beforeInvalid = __oslHubUiTest.snapshot();
    outsideChoice.checked = true;
    outsideChoice.dispatch("change");
    expect(__oslHubUiTest.snapshot()).toMatchObject({
      onboardingRoute: beforeInvalid.onboardingRoute,
      forwardSecrecyChoice: beforeInvalid.forwardSecrecyChoice,
    });
    expect(savedPreferences).toHaveLength(0);
    console.log(`TASK0340 invalid_value=${outsideChoice.value} refused=true unchanged_route=${__oslHubUiTest.snapshot().onboardingRoute} unchanged_choice=${__oslHubUiTest.snapshot().forwardSecrecyChoice}`);

    continueButton.dispatch("click");
    await flushPromises();
    expect(savedPreferences).toHaveLength(1);
    expect(savedPreferences[0]).toMatchObject({ forwardSecrecyMode: "keepGroupDelivery" });
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "privacy", forwardSecrecyMode: "keepGroupDelivery" });
    const protectionLevel = __oslHubUiTest.renderOnboardingRoute(__oslHubUiTest.snapshot().onboardingRoute);
    expect(protectionLevel).toContain("What should OSL check before you send?");
    console.log(`TASK0340 continue_route=${__oslHubUiTest.snapshot().onboardingRoute} protection_level_heading=What should OSL check before you send? saved_count=${savedPreferences.length} saved_forward_secrecy_mode=${(savedPreferences[0] as { forwardSecrecyMode: string }).forwardSecrecyMode}`);

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "forward-secrecy", forwardSecrecyChoice: null });
    __oslHubUiTest.bindOnboarding();
    backButton.dispatch("click");
    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    expect(__oslHubUiTest.renderOnboardingRoute(__oslHubUiTest.snapshot().onboardingRoute)).toContain("Enter Pro code");
    console.log(`TASK0340 back_route=${__oslHubUiTest.snapshot().onboardingRoute} back_heading=Enter Pro code`);
  }, 30_000);
});
