import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HubLicenseState } from "./core";

const VALID_CODE = "OSL-0338-V4LD-C0DE-0001";
const INVALID_CODE = "OSL-0338-BAD0-C0DE-0001";

const mocks = vi.hoisted(() => ({
  validateHubActivationCode: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("./core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./core")>();
  return {
    ...actual,
    validateHubActivationCode: mocks.validateHubActivationCode,
  };
});

interface FakeElement {
  id: string;
  value: string;
  disabled: boolean;
  textContent: string;
  className: string;
  role: string;
  dataset: Record<string, string>;
  classList: { add(className: string): void };
  handlers: Map<string, ((event: unknown) => unknown)[]>;
  addEventListener(type: string, handler: (event: unknown) => unknown): void;
  remove(): void;
}

function fakeElement(id = ""): FakeElement {
  return {
    id,
    value: "",
    disabled: false,
    textContent: "",
    className: "",
    role: "",
    dataset: {},
    classList: { add: vi.fn() },
    handlers: new Map(),
    addEventListener(type, handler) {
      const list = this.handlers.get(type) ?? [];
      this.handlers.set(type, [...list, handler]);
    },
    remove: vi.fn(),
  };
}

interface ProCodeHarness {
  form: FakeElement;
  input: FakeElement;
  submit: FakeElement;
  skip: FakeElement;
  back: FakeElement;
  appended: FakeElement[];
  savedCodes: string[];
  submitCode(code: string): Promise<void>;
  skipProCode(): void;
  backFromProCode(): void;
}

function handlersFor(element: FakeElement, type: string): ((event: unknown) => unknown)[] {
  const handlers = element.handlers.get(type) ?? [];
  expect(handlers.length, `${element.id} ${type} handlers`).toBe(1);
  return handlers;
}

function license(access: HubLicenseState["access"]): HubLicenseState {
  return {
    access,
    status: access === "pro" ? "ACTIVE" : "UNREDEEMED",
    currentPeriodEnd: access === "pro" ? 1_807_052_800 : null,
    lastValidatedAt: 1_786_080_000,
  };
}

async function loadUi(harness: ProCodeHarness) {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => {
      if (selector === "#activation-form") return harness.form;
      if (selector === "#activation-code") return harness.input;
      if (selector === '#activation-form button[type="submit"]') return harness.submit;
      if (selector === "#skip-pro-setup") return harness.skip;
      if (selector === "#onboarding-back") return harness.back;
      if (selector === ".toast") return null;
      return null;
    }),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => fakeElement()),
    body: { append: (node: FakeElement) => { harness.appended.push(node); } },
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
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
  return import("./main");
}

function buildHarness(): ProCodeHarness {
  const form = fakeElement("activation-form");
  const input = fakeElement("activation-code");
  const submit = fakeElement("activation-submit");
  const skip = fakeElement("skip-pro-setup");
  const back = fakeElement("onboarding-back");
  return {
    form,
    input,
    submit,
    skip,
    back,
    appended: [],
    savedCodes: [],
    async submitCode(code: string): Promise<void> {
      input.value = code;
      await handlersFor(form, "submit")[0]({ preventDefault: () => undefined });
    },
    skipProCode(): void {
      handlersFor(skip, "click")[0]({});
    },
    backFromProCode(): void {
      handlersFor(back, "click")[0]({});
    },
  };
}

async function setupProCodeHarness() {
  const harness = buildHarness();
  mocks.validateHubActivationCode.mockClear();
  mocks.validateHubActivationCode.mockImplementation(async (code: string) => {
    if (code === VALID_CODE) {
      harness.savedCodes.push(code);
      return license("pro");
    }
    throw new Error("invalid activation code");
  });
  const ui = await loadUi(harness);
  ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "pro", licenseAccess: "free" });
  ui.__oslHubUiTest.bindOnboarding();
  return { harness, ui };
}

describe("TASK0338 Pro code controls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllGlobals();
  });

  it("calls Continue, Skip, and Back from Pro code with valid, invalid, and absent codes", async () => {
    const valid = await setupProCodeHarness();
    expect(valid.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    await valid.harness.submitCode(VALID_CODE);
    expect(valid.harness.savedCodes).toEqual([VALID_CODE]);
    expect(valid.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("forward-secrecy");
    console.log(`TASK0338 Continue valid saved_code=${valid.harness.savedCodes[0]} route=${valid.ui.__oslHubUiTest.snapshot().onboardingRoute}`);

    const skipped = await setupProCodeHarness();
    skipped.harness.skipProCode();
    expect(skipped.harness.savedCodes).toEqual([]);
    expect(mocks.validateHubActivationCode).not.toHaveBeenCalled();
    expect(skipped.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("forward-secrecy");
    console.log(`TASK0338 Skip saved_code_count=${skipped.harness.savedCodes.length} route=${skipped.ui.__oslHubUiTest.snapshot().onboardingRoute}`);

    const backed = await setupProCodeHarness();
    backed.harness.backFromProCode();
    expect(backed.harness.savedCodes).toEqual([]);
    expect(mocks.validateHubActivationCode).not.toHaveBeenCalled();
    expect(backed.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("burnpass");
    console.log(`TASK0338 Back saved_code_count=${backed.harness.savedCodes.length} route=${backed.ui.__oslHubUiTest.snapshot().onboardingRoute}`);

    const invalid = await setupProCodeHarness();
    await invalid.harness.submitCode(INVALID_CODE);
    expect(invalid.harness.savedCodes).toEqual([]);
    expect(mocks.validateHubActivationCode).toHaveBeenCalledWith(INVALID_CODE);
    expect(invalid.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    console.log(`TASK0338 Continue invalid attempted_code=${INVALID_CODE} saved_code_count=${invalid.harness.savedCodes.length} route=${invalid.ui.__oslHubUiTest.snapshot().onboardingRoute}`);

    const absent = await setupProCodeHarness();
    await absent.harness.submitCode("");
    expect(absent.harness.savedCodes).toEqual([]);
    expect(mocks.validateHubActivationCode).not.toHaveBeenCalled();
    expect(absent.ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    console.log(`TASK0338 Continue absent saved_code_count=${absent.harness.savedCodes.length} route=${absent.ui.__oslHubUiTest.snapshot().onboardingRoute}`);
  }, 30_000);
});
