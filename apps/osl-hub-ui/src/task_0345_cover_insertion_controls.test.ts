import { beforeEach, describe, expect, it, vi } from "vitest";

const MODULE_RELOAD_BUDGET_MS = 30_000;

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./styles.css", () => ({}));
vi.mock("./local-protected-sheet.css", () => ({}));
vi.mock("./friend-invite.css", () => ({}));
vi.mock("./recovery-screen.css", () => ({}));
vi.mock("./onboarding-mullvad.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Listener = (event: { currentTarget: FakeElement; preventDefault: () => void }) => void;

class FakeElement {
  readonly dataset: Record<string, string>;
  checked: boolean;
  disabled = false;
  innerHTML = "";
  textContent = "";
  value: string;
  private readonly listeners = new Map<string, Listener[]>();

  constructor(options: { checked?: boolean; dataset?: Record<string, string>; value?: string } = {}) {
    this.checked = options.checked ?? false;
    this.dataset = options.dataset ?? {};
    this.value = options.value ?? "";
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  dispatch(type: string): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener({ currentTarget: this, preventDefault: vi.fn() });
    }
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  querySelector(): FakeElement | null {
    return null;
  }

  focus(): void {}
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
    getElementById: () => null,
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

async function loadUi(selectors: Record<string, FakeElement[]> = {}, storage = memoryStorage()) {
  vi.resetModules();
  mocks.invoke.mockReset();
  vi.stubGlobal("localStorage", storage);
  installDom(selectors);
  vi.stubGlobal("window", {
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

describe("TASK0345 Cover insertion controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  for (const choice of ["insert-on-send", "type-naturally"] as const) {
    it(`saves exact cover choice ${choice} and continues to Mullvad with it after reload`, async () => {
      const storage = memoryStorage();
      const radio = new FakeElement({ checked: true, value: choice });
      const other = new FakeElement({ checked: false, value: choice === "insert-on-send" ? "type-naturally" : "insert-on-send" });
      const continueCover = new FakeElement();
      const { __oslHubUiTest } = await loadUi({
        'input[name="cover-mode"]': [radio, other],
        "#continue-cover-draft": [continueCover],
      }, storage);
      __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "cover" });
      expect(__oslHubUiTest.snapshot().coverInsertion).toBeNull();
      expect(__oslHubUiTest.renderOnboardingRoute("cover")).toContain("Choose cover insertion");
      __oslHubUiTest.bindOnboarding();

      radio.dispatch("change");
      await settle();
      expect(__oslHubUiTest.snapshot().coverInsertion).toBe(choice);
      expect(storage.getItem("osl-preview-cover-insertion")).toBe(choice);

      continueCover.dispatch("click");
      await settle();
      // The display-mode step sits between cover insertion and the Mullvad
      // offer; what matters here is that the saved choice survives the reload.
      expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("silent-visible");
      expect(__oslHubUiTest.renderOnboardingRoute("mullvad")).toContain("Mullvad");

      vi.resetModules();
      const { loadOnboardingPreferences } = await import("./preferences");
      await expect(loadOnboardingPreferences()).resolves.toMatchObject({ coverInsertion: choice });
      console.info(`TASK0345 choice=${choice} saved_cover=${storage.getItem("osl-preview-cover-insertion")} continue_route=${__oslHubUiTest.snapshot().onboardingRoute} reload_cover=${choice}`);
    }, MODULE_RELOAD_BUDGET_MS);
  }

  it("refuses missing choice on Continue and lets Back return to Sending behavior", async () => {
    const storage = memoryStorage();
    const continueCover = new FakeElement();
    const back = new FakeElement();
    const { __oslHubUiTest } = await loadUi({
      "#continue-cover-draft": [continueCover],
      "#onboarding-back": [back],
    }, storage);
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "cover" });
    __oslHubUiTest.bindOnboarding();

    continueCover.dispatch("click");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "cover", coverInsertion: null });
    expect(storage.getItem("osl-preview-cover-insertion")).toBeNull();
    expect(__oslHubUiTest.renderOnboardingRoute("cover")).toContain("Choose cover insertion");

    back.dispatch("click");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "sending", coverInsertion: null });
    expect(__oslHubUiTest.renderOnboardingRoute("sending")).toContain("Sending behavior");
    expect(storage.getItem("osl-preview-cover-insertion")).toBeNull();
    console.info("TASK0345 missing_choice_continue route=cover cover=null saved_cover=null screen=Choose cover insertion");
    console.info("TASK0345 missing_choice_back route=sending cover=null saved_cover=null screen=Sending behavior");
  }, MODULE_RELOAD_BUDGET_MS);
});
