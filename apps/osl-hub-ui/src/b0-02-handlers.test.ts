import { readFileSync } from "node:fs";
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
const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
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

function installDom(selectors: Record<string, FakeElement[]>): void {
  const root = new FakeElement();
  const body = { append: vi.fn() };
  const querySelector = (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null;
  const querySelectorAll = (selector: string) => selectors[selector] ?? [];
  vi.stubGlobal("document", {
    querySelector,
    querySelectorAll,
    createElement: () => new FakeElement(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body,
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
}

async function loadUi(selectors: Record<string, FakeElement[]> = {}, storage: Storage = memoryStorage()) {
  vi.resetModules();
  mocks.invoke.mockReset();
  vi.stubGlobal("localStorage", storage);
  installDom(selectors);
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout: vi.fn(() => 1), clearTimeout: vi.fn(), confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("B0-02 handler bindings", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("selects and persists the Maximum protection preset without IPC", async () => {
    const storage = memoryStorage();
    const maximum = new FakeElement({ value: "maximum" });
    const inputs = [
      new FakeElement({ value: "basic" }),
      new FakeElement({ value: "balanced", checked: true }),
      maximum,
    ];
    const { __oslHubUiTest } = await loadUi({ 'input[name="protection-preset"]': inputs }, storage);
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "privacy", protectionPreset: "balanced" });
    __oslHubUiTest.bindOnboarding();

    maximum.checked = true;
    maximum.dispatch("change");

    expect(__oslHubUiTest.snapshot().protectionPreset).toBe("maximum");
    expect(storage.getItem("osl-protection-preset-v1")).toBe("maximum");
    const markup = __oslHubUiTest.renderOnboardingRoute("privacy");
    expect(markup).toContain('class="send-mode-option selected" data-protection-preset="maximum"');
    expect(markup).toMatch(/value="maximum" checked/u);
    expect(mocks.invoke).not.toHaveBeenCalled();

    const restarted = await loadUi({}, storage);
    restarted.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "privacy" });
    expect(restarted.__oslHubUiTest.snapshot().protectionPreset).toBe("maximum");
  }, MODULE_RELOAD_BUDGET_MS);

  it("moves Inbox tabs from decorative markup to filtered panel state", async () => {
    const filters = {
      all: new FakeElement({ dataset: { inboxFilter: "all" } }),
      osl: new FakeElement({ dataset: { inboxFilter: "osl" } }),
      connected: new FakeElement({ dataset: { inboxFilter: "connected" } }),
      requests: new FakeElement({ dataset: { inboxFilter: "requests" } }),
    };
    const { __oslHubUiTest } = await loadUi({ "[data-inbox-filter]": Object.values(filters) });
    __oslHubUiTest.reset({ route: "inbox", inboxFilter: "all" });
    __oslHubUiTest.bindWorkspace();

    filters.osl.dispatch("click");
    expect(__oslHubUiTest.snapshot().inboxFilter).toBe("osl");
    expect(__oslHubUiTest.renderWorkspaceContent("inbox")).toContain('data-inbox-filter="osl" aria-pressed="true"');

    filters.connected.dispatch("click");
    expect(__oslHubUiTest.snapshot().inboxFilter).toBe("connected");
    const connectedHtml = __oslHubUiTest.renderWorkspaceContent("inbox");
    expect(connectedHtml).toContain('data-inbox-filter="connected" aria-pressed="true"');
    expect(connectedHtml).toContain('id="inbox-connected-heading"');
    expect(connectedHtml).not.toContain('id="inbox-osl-heading"');
    expect(connectedHtml).not.toContain('id="inbox-requests-heading"');

    filters.requests.dispatch("click");
    expect(__oslHubUiTest.snapshot().inboxFilter).toBe("requests");
    expect(__oslHubUiTest.renderWorkspaceContent("inbox")).toContain('data-inbox-filter="requests" aria-pressed="true"');

    filters.all.dispatch("click");
    const allHtml = __oslHubUiTest.renderWorkspaceContent("inbox");
    expect(__oslHubUiTest.snapshot().inboxFilter).toBe("all");
    expect(allHtml).toContain('id="inbox-osl-heading"');
    expect(allHtml).toContain('id="inbox-connected-heading"');
    expect(allHtml).toContain('id="inbox-requests-heading"');
    expect(mocks.invoke).not.toHaveBeenCalled();
  }, MODULE_RELOAD_BUDGET_MS);

  it("persists the OSL Mail notifications checkbox locally", async () => {
    const storage = memoryStorage();
    const checkbox = new FakeElement({ checked: true });
    const { __oslHubUiTest } = await loadUi({ "#osl-mail-notifications": [checkbox] }, storage);
    __oslHubUiTest.reset({ route: "osl-mail", oslMailNotifications: true });
    __oslHubUiTest.bindWorkspace();

    checkbox.checked = false;
    checkbox.dispatch("change");

    expect(__oslHubUiTest.snapshot().oslMailNotifications).toBe(false);
    expect(storage.getItem("osl-mail-notifications-v1")).toBe("false");

    const restarted = await loadUi({}, storage);
    restarted.__oslHubUiTest.reset({ route: "osl-mail" });
    expect(restarted.__oslHubUiTest.snapshot().oslMailNotifications).toBe(false);
  }, MODULE_RELOAD_BUDGET_MS);

  it("binds Connections Mullvad install and Privacy Change preset controls", async () => {
    const changePreset = new FakeElement();
    const installMullvad = new FakeElement();
    const { __oslHubUiTest } = await loadUi({
      "[data-change-protection-preset]": [changePreset],
      "#install-mullvad-from-connections": [installMullvad],
    });
    __oslHubUiTest.reset({ route: "connections", mullvadAvailability: "installable" });
    __oslHubUiTest.bindWorkspace();

    installMullvad.dispatch("click");
    expect(__oslHubUiTest.snapshot().mullvadSetupNotice).toBe("Installing Mullvad…");

    __oslHubUiTest.reset({ route: "privacy" });
    __oslHubUiTest.bindWorkspace();
    changePreset.dispatch("click");

    expect(__oslHubUiTest.snapshot()).toMatchObject({ route: "onboarding", onboardingRoute: "privacy" });
    expect(mainSource).toContain('#install-mullvad-from-connections');
    expect(mainSource).toContain('runMullvadSetupAction("install", "connections")');
  }, MODULE_RELOAD_BUDGET_MS);
});
