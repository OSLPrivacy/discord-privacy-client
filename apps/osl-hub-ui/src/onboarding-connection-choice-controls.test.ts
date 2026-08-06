import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type FakeEvent = {
  currentTarget: FakeElement;
  preventDefault(): void;
  stopPropagation(): void;
};

type FakeListener = (event: FakeEvent) => void;

class FakeElement {
  readonly dataset: Record<string, string> = {};
  readonly classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  readonly listeners = new Map<string, FakeListener[]>();
  checked = false;
  disabled = false;
  innerHTML = "";
  textContent = "";
  value = "";

  constructor(options: { checked?: boolean; dataset?: Record<string, string>; value?: string } = {}) {
    this.checked = options.checked ?? false;
    this.value = options.value ?? "";
    Object.assign(this.dataset, options.dataset ?? {});
  }

  addEventListener(type: string, listener: FakeListener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  dispatch(type: string): void {
    const event = {
      currentTarget: this,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    };
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  querySelector(): null {
    return null;
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  prepend(): void {
    return undefined;
  }

  remove(): void {
    return undefined;
  }
}

const localStore = new Map<string, string>();
const root = new FakeElement();
let singleSelectors = new Map<string, FakeElement | null>([["#app", root]]);
let listSelectors = new Map<string, FakeElement[]>();
let ui: typeof import("./main");

function installConnectionChoiceDom(): {
  back: FakeElement;
  continueButton: FakeElement;
  direct: FakeElement;
  invalid: FakeElement;
  tor: FakeElement;
} {
  const back = new FakeElement();
  const continueButton = new FakeElement();
  const tor = new FakeElement({ value: "tor" });
  const direct = new FakeElement({ value: "direct" });
  const invalid = new FakeElement({ value: "relay" });
  singleSelectors = new Map<string, FakeElement | null>([
    ["#app", root],
    ["#onboarding-back", back],
    ['[data-tor-choice-continue]', continueButton],
  ]);
  listSelectors = new Map<string, FakeElement[]>([
    ['input[name="tor-route"]', [tor, direct, invalid]],
  ]);
  ui.__oslHubUiTest.bindOnboarding();
  return { back, continueButton, direct, invalid, tor };
}

function resetOnConnectionChoice(): void {
  localStore.clear();
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue(undefined);
  ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "tor" });
}

async function settleNativeSave(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", {
    activeElement: null,
    addEventListener: vi.fn(),
    body: new FakeElement(),
    createElement: vi.fn(() => new FakeElement()),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    querySelector: vi.fn((selector: string) => selector === "#app" ? root : singleSelectors.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => listSelectors.get(selector) ?? []),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  resetOnConnectionChoice();
});

describe("Task 0343 connection-choice controls", () => {
  it("records Tor, Direct, Continue, Back, and invalid connection-kind results", async () => {
    let controls = installConnectionChoiceDom();
    expect(mocks.invoke).toHaveBeenCalledTimes(0);
    controls.tor.checked = true;
    controls.tor.dispatch("change");
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "tor", torChoice: "tor" });
    expect(mocks.invoke).toHaveBeenCalledTimes(0);
    controls.continueButton.dispatch("click");
    await settleNativeSave();
    expect(mocks.invoke.mock.calls).toEqual([["set_tor_preference", { preference: "tor" }]]);
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "defaults", torChoice: "tor" });
    console.info("TASK0343_TOR_CONTROL selected=tor saved_before_continue=0");
    console.info("TASK0343_CONTINUE_AFTER_TOR opened=defaults saved_preference=tor save_count=1");

    resetOnConnectionChoice();
    controls = installConnectionChoiceDom();
    controls.direct.checked = true;
    controls.direct.dispatch("change");
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "tor", torChoice: "direct" });
    expect(mocks.invoke).toHaveBeenCalledTimes(0);
    controls.continueButton.dispatch("click");
    await settleNativeSave();
    expect(mocks.invoke.mock.calls).toEqual([["set_tor_preference", { preference: "direct" }]]);
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "defaults", torChoice: "direct" });
    console.info("TASK0343_DIRECT_CONTROL selected=direct saved_before_continue=0");
    console.info("TASK0343_CONTINUE_AFTER_DIRECT opened=defaults saved_preference=direct save_count=1");

    resetOnConnectionChoice();
    controls = installConnectionChoiceDom();
    controls.back.dispatch("click");
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "privacy", torChoice: "tor" });
    expect(mocks.invoke).toHaveBeenCalledTimes(0);
    console.info("TASK0343_BACK_CONTROL opened=privacy saved_count=0");

    resetOnConnectionChoice();
    controls = installConnectionChoiceDom();
    controls.direct.checked = true;
    controls.direct.dispatch("change");
    controls.invalid.checked = true;
    controls.invalid.dispatch("change");
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "tor", torChoice: "direct" });
    expect(mocks.invoke).toHaveBeenCalledTimes(0);
    console.info("TASK0343_INVALID_CONTROL input=relay refused=true choice_after=direct page_after=tor saved_count=0");
  });
});
