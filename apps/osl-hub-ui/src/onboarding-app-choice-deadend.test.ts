/**
 * D-190 -- onboarding dead-ends on "Choose apps" when a native app is selected.
 *
 * These tests drive the REAL `#continue-app-choice` listener bound by the real
 * `bindOnboarding()`, over the real `loadNativeApps` -> `parseNativeApps` ->
 * `isCompleteNativeCatalog` path. Nothing about the decision is stubbed: the only
 * stand-ins are the DOM and the Tauri transport, and the transport is fed the
 * catalog `apps/osl-hub/src/native_apps.rs` actually serialises.
 *
 * The existing coverage of this screen (`onboarding-layout.test.ts:319-330`,
 * `onboarding-app-choice.test.ts:27`) asserts on the TEXT of `main.ts`. It was
 * green throughout the regression, because the text never changed -- only the
 * size of `supportedNativeAppIds` did.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";


// D-251: every `it()` below deliberately re-loads `./main` with its own
// selectors / storage / stubs, so the import CANNOT be hoisted into a single
// `beforeAll` without destroying what the tests check. `src/main.ts` is ~10k
// lines and one load costs ~2.5 s cold, which left almost nothing of vitest's
// default 5,000 ms budget for the behaviour under test: on a busy machine these
// tests died with `Test timed out in 5000ms` before reaching an assertion.
// The budget below covers MODULE LOADING, not the behaviour -- no assertion
// depends on it, and every assertion is unchanged.
const MODULE_RELOAD_BUDGET_MS = 30_000;

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Listener = (event: { currentTarget: FakeElement; preventDefault: () => void }) => void;

class FakeElement {
  readonly dataset: Record<string, string>;
  disabled = false;
  innerHTML = "";
  private readonly listeners = new Map<string, Listener[]>();

  constructor(options: { dataset?: Record<string, string> } = {}) {
    this.dataset = options.dataset ?? {};
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  /** Fire only the FIRST listener: a re-render rebinds, and a real click is one click. */
  dispatch(type: string): void {
    const listener = (this.listeners.get(type) ?? [])[0];
    listener?.({ currentTarget: this, preventDefault: vi.fn() });
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  querySelector(): FakeElement | null {
    return null;
  }
}

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

/**
 * What `list_native_apps` actually returns on the shipping build.
 *
 * One row per entry of `NATIVE_APPS` (`apps/osl-hub/src/native_apps.rs:434-547`):
 * discord, telegram, signal, whatsapp, outlook. Discord is `installed` because
 * the reporter had Discord installed and running when the dead-end was hit.
 */
const shippingWindowsCatalog = [
  { id: "discord", displayName: "Discord", availability: "installed", supportStatus: "beta", protectedMode: "assistOnly", isolatedProfileAvailable: true, supportsOverlay: false },
  { id: "telegram", displayName: "Telegram", availability: "installable", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: false },
  { id: "signal", displayName: "Signal", availability: "installable", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: false },
  { id: "whatsapp", displayName: "WhatsApp", availability: "installable", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: false, supportsOverlay: false },
  { id: "outlook", displayName: "Outlook", availability: "unavailable", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: false, supportsOverlay: false },
];

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

async function loadUi(selectors: Record<string, FakeElement[]>, storage: Storage) {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "list_native_apps") return shippingWindowsCatalog;
    if (command === "save_onboarding_preferences") return args?.preferences;
    if (command === "list_linked_services") return [];
    throw new Error(`unstubbed command ${command}`);
  });
  vi.stubGlobal("localStorage", storage);
  installDom(selectors);
  // `__TAURI_INTERNALS__` is what `isTauriRuntime()` reads (preferences.ts:31-33).
  // Without it `loadNativeApps` returns the browser preview list and never
  // reaches the command this defect is about.
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

function commands(): string[] {
  return mocks.invoke.mock.calls.map((call) => String(call[0]));
}

/** Let the async click handler run to completion. */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 12; turn += 1) await Promise.resolve();
}

describe("D-190 -- Choose apps must never swallow Continue", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("advances when Discord is already selected and the Windows catalog answers", async () => {
    // The state the reporter was in: Discord carried into "Choose apps" from
    // storage, so the tile arrives selected without a click on this screen.
    const storage = memoryStorage({ "osl-selected-apps-v1": JSON.stringify(["discord"]) });
    const continueButton = new FakeElement();
    const ui = await loadUi({ "#continue-app-choice": [continueButton] }, storage);
    await ui.loadUiPreferences();
    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "tutorial" });
    ui.__oslHubUiTest.bindOnboarding();

    continueButton.dispatch("click");
    await settle();

    // The gate did run -- this is the probe the dead-end hangs on.
    expect(commands()).toContain("list_native_apps");
    // ...and Continue must then proceed. Before the fix it returns false here
    // and the click is a no-op, so this never happens.
    expect(commands()).toContain("save_onboarding_preferences");
  }, MODULE_RELOAD_BUDGET_MS);

  it("says why it cannot continue, and offers a way out, when the catalog probe fails", async () => {
    const storage = memoryStorage({ "osl-selected-apps-v1": JSON.stringify(["discord"]) });
    const continueButton = new FakeElement();
    const ui = await loadUi({ "#continue-app-choice": [continueButton] }, storage);
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "list_native_apps") throw new Error("probe failed");
      if (command === "list_linked_services") return [];
      throw new Error(`unstubbed command ${command}`);
    });
    await ui.loadUiPreferences();
    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "tutorial" });
    ui.__oslHubUiTest.bindOnboarding();

    continueButton.dispatch("click");
    await settle();

    // Continue correctly refused. What it must NOT do is refuse invisibly.
    expect(commands()).not.toContain("save_onboarding_preferences");

    const markup = ui.__oslHubUiTest.renderChooseAppsForTest();
    // A refusal that survives on the panel -- not a toast that is gone in 2.5s.
    expect(markup).toContain('id="app-choice-refusal"');
    expect(markup).toContain('role="alert"');
    expect(markup).toMatch(/Windows apps/u);
    // ...and the escape the reporter could only find by de-selecting a tile they
    // never selected is now a labelled control.
    expect(markup).toContain('id="continue-without-apps"');
  }, MODULE_RELOAD_BUDGET_MS);

  it("the escape hatch completes onboarding and drops the native selection", async () => {
    const storage = memoryStorage({ "osl-selected-apps-v1": JSON.stringify(["discord"]) });
    const continueButton = new FakeElement();
    const escapeButton = new FakeElement();
    const ui = await loadUi({
      "#continue-app-choice": [continueButton],
      "#continue-without-apps": [escapeButton],
    }, storage);
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "list_native_apps") throw new Error("probe failed");
      if (command === "save_onboarding_preferences") return args?.preferences;
      if (command === "list_linked_services") return [];
      throw new Error(`unstubbed command ${command}`);
    });
    await ui.loadUiPreferences();
    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "tutorial" });
    ui.__oslHubUiTest.bindOnboarding();

    continueButton.dispatch("click");
    await settle();
    expect(commands()).not.toContain("save_onboarding_preferences");
    // The control has to be on the panel, not merely bound to something.
    expect(ui.__oslHubUiTest.renderChooseAppsForTest()).toContain('id="continue-without-apps"');

    escapeButton.dispatch("click");
    await settle();

    expect(commands()).toContain("save_onboarding_preferences");
    expect(JSON.parse(storage.getItem("osl-selected-apps-v1") ?? "[]")).not.toContain("discord");
  }, MODULE_RELOAD_BUDGET_MS);
});
