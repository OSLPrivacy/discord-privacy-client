import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";

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

type Handler = (event: Record<string, unknown>) => unknown;

class FakeElement {
  disabled = false;
  innerHTML = "";
  textContent = "";
  value = "";
  checked = false;
  dataset: Record<string, string> = {};
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  private readonly handlers = new Map<string, Handler[]>();

  constructor(dataset: Record<string, string> = {}) {
    this.dataset = dataset;
  }

  addEventListener(type: string, handler: Handler): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }

  setAttribute(): void {}
  removeAttribute(): void {}
  getAttribute(): string | null { return null; }
  hasAttribute(name: string): boolean { return name.startsWith("data-"); }
  focus(): void {}
  querySelector(): FakeElement | null { return null; }
  querySelectorAll(): FakeElement[] { return []; }

  async click(): Promise<void> {
    const handler = (this.handlers.get("click") ?? [])[0];
    expect(handler, "shipping completion control was not bound").toBeTypeOf("function");
    await handler?.({ currentTarget: this, preventDefault: () => undefined });
    for (let turn = 0; turn < 16; turn += 1) await Promise.resolve();
  }
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => [...values.keys()][index] ?? null,
    removeItem: (key: string) => { values.delete(key); },
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
}

const service = {
  id: "discord",
  displayName: "Discord",
  sidebarGlyph: "D",
  sidebarOrder: 0,
  category: "consumer",
  launchState: "available",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [] as import("./services").LinkedAccount[],
} as const;

let selectors: Record<string, FakeElement[]> = {};

async function loadUi() {
  const app = new FakeElement();
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? app : selectors[selector]?.[0] ?? null,
    querySelectorAll: (selector: string) => selectors[selector] ?? [],
    getElementById: (id: string) => selectors[`#${id}`]?.[0] ?? null,
    createElement: () => new FakeElement(),
    body: { append: vi.fn() },
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
    activeElement: null,
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
  mocks.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true) });
  return import("./main");
}

function saveCount(): number {
  return mocks.invoke.mock.calls.filter((call) => call[0] === "save_onboarding_preferences").length;
}

describe("TASK0334A shipping setup completion runtime crawl", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    selectors = {};
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "save_onboarding_preferences") return args?.preferences;
      if (command === "list_linked_services") return [];
      if (command === "list_native_apps") return [];
      throw new Error(`unneeded command ${command}`);
    });
  });

  it("independent source inventory and real listeners agree on all six reachable routes", async () => {
    const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const staticInventory = new Set<string>();
    const patterns: Array<[string, RegExp]> = [
      ["app-choice", /id="continue-app-choice"[\s\S]*?#continue-app-choice[\s\S]*?completeOnboarding\(\)/u],
      ["app-choice-refusal", /id="continue-without-apps"[\s\S]*?#continue-without-apps[\s\S]*?continueWithoutNativeApps\(\)/u],
      ["onboarding-service-home", /\[data-route\][\s\S]*?requestedRoute === "home"[\s\S]*?advanceOnboardingConnection/u],
      ["onboarding-service-continue", /id="onboarding-service-continue"[\s\S]*?#onboarding-service-continue[\s\S]*?continueOnboardingFromService/u],
      ["service-guide-skip", /id="service-guide-skip"[\s\S]*?#service-guide-skip[\s\S]*?advanceOnboardingConnection/u],
      ["skip-connect-app", /id="skip-connect-app"[\s\S]*?#skip-connect-app[\s\S]*?completeOnboarding\(\)/u],
    ];
    for (const [name, pattern] of patterns) {
      expect(source, `starved static route ${name}`).toMatch(pattern);
      staticInventory.add(name);
    }
    expect(source).not.toContain("service-guide-finish");

    const ui = await loadUi();
    const runtime = new Set<string>();
    const crawl = async (
      name: string,
      selector: string,
      binding: "onboarding" | "workspace",
      patch: Record<string, unknown>,
      dataset: Record<string, string> = {},
    ) => {
      const control = new FakeElement(dataset);
      selectors = { [selector]: [control], ...(selector === "[data-route]" ? { "[data-route]": [control] } : {}) };
      ui.__oslHubUiTest.reset({
        route: binding === "onboarding" ? "onboarding" : "service",
        onboardingRoute: "apps",
        coreReady: true,
        services: [service],
        ...patch,
      });
      const before = saveCount();
      if (binding === "onboarding") ui.__oslHubUiTest.bindOnboarding();
      else ui.__oslHubUiTest.bindWorkspace();
      await control.click();
      expect(saveCount(), `${name} must reach the shared native commit`).toBe(before + 1);
      runtime.add(name);
    };

    await crawl("app-choice", "#continue-app-choice", "onboarding", {});
    await crawl("app-choice-refusal", "#continue-without-apps", "onboarding", {});
    await crawl("skip-connect-app", "#skip-connect-app", "onboarding", { onboardingConnectAppId: "discord" });
    await crawl("onboarding-service-home", "[data-route]", "workspace", {
      onboardingServiceSetup: true,
      activeHomeAppId: "discord",
    }, { route: "home" });
    await crawl("onboarding-service-continue", "#onboarding-service-continue", "workspace", {
      onboardingServiceSetup: true,
      activeHomeAppId: "discord",
    });
    await crawl("service-guide-skip", "#service-guide-skip", "workspace", {
      onboardingServiceSetup: true,
      activeHomeAppId: "discord",
    });

    expect([...runtime].sort()).toEqual([...staticInventory].sort());
    console.info(
      `TASK0334A_RECONCILE static=${staticInventory.size} runtime=${runtime.size} missing=0 extra=0 names=${[...runtime].sort().join(",")}`,
    );
  }, 30_000);
});
