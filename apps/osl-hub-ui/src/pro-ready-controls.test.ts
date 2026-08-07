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
  disabled = false;
  innerHTML = "";
  textContent = "";
  value = "";
  private readonly listeners = new Map<string, Listener[]>();

  constructor(options: { dataset?: Record<string, string>; value?: string } = {}) {
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

async function loadUi(selectors: Record<string, FakeElement[]> = {}) {
  vi.resetModules();
  mocks.invoke.mockReset();
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

describe("TASK0339 Pro-ready controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps Pro-ready Continue inert without a Pro result and sends Back to Pro code", async () => {
    const continueReady = new FakeElement();
    const back = new FakeElement();
    const { __oslHubUiTest } = await loadUi({
      "#continue-pro-ready": [continueReady],
      "#onboarding-back": [back],
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "pro", licenseAccess: "pro" });

    expect(__oslHubUiTest.renderOnboardingRoute("pro")).toContain("OSL Pro is ready");
    __oslHubUiTest.bindOnboarding();

    continueReady.dispatch("click");
    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");

    back.dispatch("click");
    const proCode = __oslHubUiTest.renderOnboardingRoute("pro");
    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    expect(proCode).toContain("Enter Pro code");
    expect(proCode).toContain('id="activation-form"');
    console.info("TASK0339 no-prior-result Continue route=pro Back screen=Enter Pro code");
  }, MODULE_RELOAD_BUDGET_MS);

  it("allows Pro-ready Continue only after validation returns active Pro", async () => {
    const form = new FakeElement();
    const input = new FakeElement({ value: "OSL-ABCD-EFGH-IJKL-MNOP" });
    const submit = new FakeElement();
    const continueReady = new FakeElement();
    const { __oslHubUiTest } = await loadUi({
      "#activation-form": [form],
      "#activation-code": [input],
      '#activation-form button[type="submit"]': [submit],
      "#continue-pro-ready": [continueReady],
    });
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "validate_hub_activation_code") {
        return { access: "pro", status: "ACTIVE", currentPeriodEnd: null, lastValidatedAt: null };
      }
      throw new Error(`unstubbed command ${command}`);
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "pro", licenseAccess: "free" });
    __oslHubUiTest.bindOnboarding();

    form.dispatch("submit");
    await settle();
    expect(mocks.invoke).toHaveBeenCalledWith("validate_hub_activation_code", { activationCode: "OSL-ABCD-EFGH-IJKL-MNOP" });
    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    expect(__oslHubUiTest.renderOnboardingRoute("pro")).toContain("OSL Pro is ready");

    continueReady.dispatch("click");

    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("forward-secrecy");
    console.info("TASK0339 prior-Pro-result Continue route=forward-secrecy result=ACTIVE");
  }, MODULE_RELOAD_BUDGET_MS);
});
