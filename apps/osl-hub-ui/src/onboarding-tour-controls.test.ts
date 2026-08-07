import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type TourResult = ReturnType<typeof import("./main").__oslHubUiTest.quickTourSnapshot>;

function memoryStorage(seed: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(seed));
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

function installGlobals(): void {
  const root = {
    innerHTML: "",
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
  };
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : null,
    querySelectorAll: () => [],
    createElement: () => root,
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
}

async function loadUi() {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "save_onboarding_preferences") return args?.preferences;
    if (command === "list_linked_services") return [];
    if (command === "list_native_apps") return [];
    throw new Error(`unstubbed command ${command}`);
  });
  installGlobals();
  return import("./main");
}

function line(label: string, result: TourResult): string {
  return `TASK_0348_${label} accepted=${result.accepted} control="${result.control}" screen=${result.screen} route=${result.route} onboardingRoute=${result.onboardingRoute} card=${result.cardNumber ?? "none"} completed=${result.completedCardCount} reason=${result.reason ?? "none"}`;
}

describe("TASK 0348 quick-tour controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("records Back, Next, Choose apps, and invalid-card outcomes from tour cards", async () => {
    const ui = await loadUi();
    const { __oslHubUiTest } = ui;

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "tutorial", onboardingTourStep: 0 });
    const initial = __oslHubUiTest.quickTourSnapshot();
    expect(initial).toMatchObject({ screen: "tour-card", cardNumber: 1, completedCardCount: 0 });
    console.log(line("CARD_1_START", initial));

    const backFirst = __oslHubUiTest.backQuickTour();
    expect(backFirst).toMatchObject({ accepted: true, screen: "setup", onboardingRoute: "welcome", cardNumber: null, completedCardCount: 0 });
    console.log(line("BACK_CARD_1", backFirst));

    __oslHubUiTest.setQuickTourCardNumber(1);
    const chooseFirst = __oslHubUiTest.chooseAppsFromQuickTour();
    expect(chooseFirst).toMatchObject({ accepted: false, reason: "quick-tour-incomplete", screen: "tour-card", cardNumber: 1, completedCardCount: 0 });
    console.log(line("CHOOSE_APPS_CARD_1_REFUSED", chooseFirst));

    __oslHubUiTest.setQuickTourCardNumber(1);
    const nextFirst = __oslHubUiTest.nextQuickTour();
    expect(nextFirst).toMatchObject({ accepted: true, screen: "tour-card", cardNumber: 2, completedCardCount: 1 });
    console.log(line("NEXT_CARD_1", nextFirst));

    for (const [from, to, completed] of [[2, 3, 2], [3, 4, 3], [4, 5, 4]] as const) {
      __oslHubUiTest.setQuickTourCardNumber(from);
      const middleNext = __oslHubUiTest.nextQuickTour();
      expect(middleNext).toMatchObject({ accepted: true, screen: "tour-card", cardNumber: to, completedCardCount: completed });
      console.log(line(`NEXT_CARD_${from}`, middleNext));
    }

    __oslHubUiTest.setQuickTourCardNumber(3);
    const backMiddle = __oslHubUiTest.backQuickTour();
    expect(backMiddle).toMatchObject({ accepted: true, screen: "tour-card", cardNumber: 2, completedCardCount: 1 });
    console.log(line("BACK_MIDDLE_CARD_3", backMiddle));

    __oslHubUiTest.setQuickTourCardNumber(5);
    const backLast = __oslHubUiTest.backQuickTour();
    expect(backLast).toMatchObject({ accepted: true, screen: "tour-card", cardNumber: 4, completedCardCount: 3 });
    console.log(line("BACK_LAST_CARD_5", backLast));

    __oslHubUiTest.setQuickTourCardNumber(3);
    const beforeInvalid = __oslHubUiTest.quickTourSnapshot();
    const invalid = __oslHubUiTest.setQuickTourCardNumber(42);
    expect(invalid).toMatchObject({ accepted: false, reason: "invalid-card-number", screen: "tour-card", cardNumber: 3, completedCardCount: 2 });
    expect(invalid.cardNumber).toBe(beforeInvalid.cardNumber);
    expect(invalid.completedCardCount).toBe(beforeInvalid.completedCardCount);
    console.log(line("INVALID_CARD_42_REFUSED", invalid));

    __oslHubUiTest.setQuickTourCardNumber(3);
    const chooseMiddle = __oslHubUiTest.chooseAppsFromQuickTour();
    expect(chooseMiddle).toMatchObject({ accepted: false, reason: "quick-tour-incomplete", screen: "tour-card", cardNumber: 3, completedCardCount: 2 });
    console.log(line("CHOOSE_APPS_MIDDLE_CARD_3_REFUSED", chooseMiddle));

    __oslHubUiTest.setQuickTourCardNumber(5);
    const nextLast = __oslHubUiTest.nextQuickTour();
    expect(nextLast).toMatchObject({ accepted: true, screen: "app-selection", cardNumber: null, completedCardCount: 5 });
    expect(__oslHubUiTest.renderOnboardingRoute("tutorial")).toContain('id="continue-app-choice"');
    console.log(line("NEXT_LAST_CARD_5", nextLast));

    __oslHubUiTest.setQuickTourCardNumber(5);
    const chooseLast = __oslHubUiTest.chooseAppsFromQuickTour();
    expect(chooseLast).toMatchObject({ accepted: true, screen: "app-selection", cardNumber: null, completedCardCount: 5 });
    expect(__oslHubUiTest.renderOnboardingRoute("tutorial")).toContain('id="continue-app-choice"');
    console.log(line("CHOOSE_APPS_LAST_CARD_5", chooseLast));
  }, 30_000);
});
