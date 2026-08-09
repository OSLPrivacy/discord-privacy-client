import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const MODULE_RELOAD_BUDGET_MS = 30_000;

type Listener = (event: { currentTarget: FakeElement; preventDefault: () => void }) => void;

class FakeElement {
  readonly dataset: Record<string, string>;
  checked = false;
  disabled = false;
  value = "";
  innerHTML = "";
  textContent = "";
  className = "";
  role = "";
  readonly classList = { add: vi.fn() };
  private readonly listeners = new Map<string, Listener[]>();

  constructor(options: { dataset?: Record<string, string>; checked?: boolean; value?: string } = {}) {
    this.dataset = options.dataset ?? {};
    this.checked = options.checked ?? false;
    this.value = options.value ?? "";
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  dispatch(type: string): void {
    for (const listener of this.listeners.get(type) ?? []) listener({ currentTarget: this, preventDefault: vi.fn() });
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  querySelector(): FakeElement | null {
    return null;
  }

  focus(): void {}

  remove(): void {}
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

function installDom(selectors: Record<string, FakeElement[]>): void {
  const root = new FakeElement();
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null,
    querySelectorAll: (selector: string) => selectors[selector] ?? [],
    createElement: () => new FakeElement(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

async function loadUi(selectors: Record<string, FakeElement[]>) {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "set_hub_screenshot_protection") return true;
    if (command === "save_onboarding_preferences") return args?.preferences;
    throw new Error(`unstubbed command ${command}`);
  });
  vi.stubGlobal("localStorage", memoryStorage());
  installDom(selectors);
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout: vi.fn(() => 1),
    clearTimeout: vi.fn(),
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

async function settle(): Promise<void> {
  for (let turn = 0; turn < 12; turn += 1) await Promise.resolve();
}

function savedPreferences(): Array<Record<string, unknown>> {
  return mocks.invoke.mock.calls
    .filter(([command]) => command === "save_onboarding_preferences")
    .map(([, args]) => (args as { preferences: Record<string, unknown> }).preferences);
}

describe("TASK 0344 privacy and sending controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("records capture, all three send choices, understanding, Continue, Back, and invalid send refusal", async () => {
    const capture = new FakeElement({ checked: true });
    const sendManual = new FakeElement({ dataset: { sendMode: "manual" } });
    const sendClipboard = new FakeElement({ dataset: { sendMode: "clipboard" } });
    const sendDouble = new FakeElement({ dataset: { sendMode: "double" } });
    const sendInvalid = new FakeElement({ dataset: { sendMode: "hidden-auto" } });
    const understanding = new FakeElement({ checked: false });
    const continueButton = new FakeElement();
    const backButton = new FakeElement();
    const ui = await loadUi({
      "#window-capture-enabled": [capture],
      "[data-send-mode]": [sendManual, sendClipboard, sendDouble, sendInvalid],
      "#accept-send-risk": [understanding],
      "#finish-onboarding": [continueButton],
      "#onboarding-back": [backButton],
    });
    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "sending" });
    expect(ui.__oslHubUiTest.renderOnboardingRoute("sending")).toContain("Sending behavior");
    expect(savedPreferences()).toHaveLength(0);
    const initialSaveCount = savedPreferences().length;
    ui.__oslHubUiTest.bindOnboarding();

    capture.checked = false;
    capture.dispatch("change");
    await settle();
    const captureSave = savedPreferences().at(-1);
    expect(captureSave).toMatchObject({ windowCaptureEnabled: false, sendMode: "manual" });

    sendManual.dispatch("click");
    await settle();
    const manualSave = savedPreferences().at(-1);
    expect(manualSave).toMatchObject({ sendMode: "manual", windowCaptureEnabled: false });

    sendClipboard.dispatch("click");
    await settle();
    const clipboardSave = savedPreferences().at(-1);
    expect(clipboardSave).toMatchObject({ sendMode: "clipboard", windowCaptureEnabled: false });

    sendDouble.dispatch("click");
    await settle();
    const doubleSave = savedPreferences().at(-1);
    expect(doubleSave).toMatchObject({
      sendMode: "double",
      windowCaptureEnabled: false,
      acknowledgeExperimentalSendRisk: false,
    });

    understanding.checked = true;
    understanding.dispatch("change");
    await settle();
    const understandingSave = savedPreferences().at(-1);
    expect(understandingSave).toMatchObject({
      sendMode: "double",
      windowCaptureEnabled: false,
      acknowledgeExperimentalSendRisk: true,
    });

    const beforeInvalid = ui.__oslHubUiTest.snapshot();
    const beforeInvalidSaves = savedPreferences();
    sendInvalid.dispatch("click");
    await settle();
    const invalidSaveCountDelta = savedPreferences().length - beforeInvalidSaves.length;
    expect(savedPreferences()).toHaveLength(beforeInvalidSaves.length);
    const afterInvalid = ui.__oslHubUiTest.snapshot();
    expect(ui.__oslHubUiTest.snapshot()).toMatchObject({
      route: beforeInvalid.route,
      onboardingRoute: beforeInvalid.onboardingRoute,
      setup: beforeInvalid.setup,
      windowCaptureEnabled: beforeInvalid.windowCaptureEnabled,
    });
    expect(ui.__oslHubUiTest.renderOnboardingRoute("sending")).toContain('data-send-mode="double" aria-pressed="true"');

    const beforeContinueSaveCount = savedPreferences().length;
    continueButton.dispatch("click");
    await settle();
    expect(savedPreferences()).toHaveLength(beforeContinueSaveCount + 1);
    const continueSave = savedPreferences().at(-1);
    expect(continueSave).toMatchObject({
      sendMode: "double",
      placementMode: "atomic",
      windowCaptureEnabled: false,
      acknowledgeExperimentalSendRisk: true,
    });
    expect(ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("cover");
    const coverMarkup = ui.__oslHubUiTest.renderOnboardingRoute("cover");
    expect(coverMarkup).toContain("Choose cover insertion");

    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "sending" });
    backButton.dispatch("click");
    // Back is ONE step: sending's predecessor in the spine is the defaults
    // review (the old double-step to "tor" skipped a screen).
    expect(ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("defaults");
    const defaultsMarkup = ui.__oslHubUiTest.renderOnboardingRoute("defaults");
    expect(defaultsMarkup).toContain("What should OSL delete?");

    console.info([
      `TASK0344_INITIAL_SAVED_CHOICES=${initialSaveCount}`,
      `TASK0344_CAPTURE_CHOICE_WINDOW_CAPTURE=${String(captureSave?.["windowCaptureEnabled"])}`,
      `TASK0344_CAPTURE_BACKEND_ENABLED=${String((mocks.invoke.mock.calls.find(([command]) => command === "set_hub_screenshot_protection")?.[1] as { enabled?: boolean } | undefined)?.enabled)}`,
      `TASK0344_SEND_CHOICE_1=${String(manualSave?.["sendMode"])}`,
      `TASK0344_SEND_CHOICE_2=${String(clipboardSave?.["sendMode"])}`,
      `TASK0344_SEND_CHOICE_3=${String(doubleSave?.["sendMode"])}`,
      `TASK0344_UNDERSTANDING_ACK=${String(understandingSave?.["acknowledgeExperimentalSendRisk"])}`,
      `TASK0344_INVALID_SEND_MODE=${sendInvalid.dataset.sendMode}`,
      `TASK0344_INVALID_SAVE_COUNT_DELTA=${invalidSaveCountDelta}`,
      `TASK0344_INVALID_ROUTE=${afterInvalid.onboardingRoute}`,
      `TASK0344_INVALID_SELECTED_SEND=${afterInvalid.setup.sendMode}`,
      `TASK0344_CONTINUE_SAVE_COUNT_DELTA=${savedPreferences().length - beforeContinueSaveCount}`,
      `TASK0344_CONTINUE_SEND=${String(continueSave?.["sendMode"])}`,
      `TASK0344_CONTINUE_CAPTURE=${String(continueSave?.["windowCaptureEnabled"])}`,
      `TASK0344_CONTINUE_ROUTE=cover`,
      `TASK0344_COVER_HEADING=Choose cover insertion`,
      `TASK0344_BACK_ROUTE=defaults`,
      `TASK0344_BACK_PAGE=What should OSL delete?`,
    ].join("\n"));
  }, MODULE_RELOAD_BUDGET_MS);
});
