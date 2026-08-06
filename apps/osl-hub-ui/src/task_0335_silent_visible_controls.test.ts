import { beforeEach, describe, expect, it, vi } from "vitest";
import { onboardingSilentVisibleMarkup, silentVisibleDisplayMark } from "./onboarding-silent-visible";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Listener = () => void;

class FakeElement {
  readonly dataset: Record<string, string>;
  innerHTML = "";
  private readonly listeners = new Map<string, Listener[]>();

  constructor(dataset: Record<string, string> = {}) {
    this.dataset = dataset;
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  dispatch(type: string): void {
    for (const listener of this.listeners.get(type) ?? []) listener();
  }

  clearListeners(): void {
    this.listeners.clear();
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  querySelector(): FakeElement | null {
    return null;
  }
}

function installDom(selectors: Record<string, FakeElement[]>): void {
  const root = new FakeElement();
  const querySelector = (selector: string) => selector === "#app" ? root : selectors[selector]?.[0] ?? null;
  const querySelectorAll = (selector: string) => selectors[selector] ?? [];
  vi.stubGlobal("document", {
    querySelector,
    querySelectorAll,
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
  installDom(selectors);
  vi.stubGlobal("localStorage", {
    getItem: () => null,
    setItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
  });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout: vi.fn(() => 1), clearTimeout: vi.fn(), confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function displayMarkCount(markup: string): number {
  return markup.match(/data-silent-visible-display-mark/gu)?.length ?? 0;
}

function record(line: string): void {
  process.stdout.write(`${line}\n`);
}

describe("TASK 0335 silent-visible controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("calls SILENT, VISIBLE, Continue, Back, and refuses one unknown mode", async () => {
    const silent = new FakeElement({ silentVisibleMode: "SILENT" });
    const visible = new FakeElement({ silentVisibleMode: "VISIBLE" });
    const unknown = new FakeElement({ silentVisibleMode: "UNKNOWN" });
    const cont = new FakeElement();
    const back = new FakeElement();
    const { __oslHubUiTest } = await loadUi({
      "[data-silent-visible-mode]": [silent, visible, unknown],
      "#continue-silent-visible": [cont],
      "#onboarding-back": [back],
    });
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "silent-visible" });
    __oslHubUiTest.bindOnboarding();

    expect(__oslHubUiTest.snapshot()).toMatchObject({ route: "onboarding", onboardingRoute: "silent-visible", silentVisibleMode: null });
    record(`TASK0335_START route=${__oslHubUiTest.snapshot().onboardingRoute} saved=${__oslHubUiTest.snapshot().silentVisibleMode}`);

    silent.dispatch("click");
    const silentMarkup = __oslHubUiTest.renderSilentVisible();
    expect(__oslHubUiTest.snapshot().silentVisibleMode).toBe("SILENT");
    expect(silentVisibleDisplayMark("SILENT")).toBeNull();
    expect(displayMarkCount(silentMarkup)).toBe(0);
    record(`TASK0335_SILENT saved=${__oslHubUiTest.snapshot().silentVisibleMode} displayMark=${silentVisibleDisplayMark("SILENT") ?? "absent"} displayMarkCount=${displayMarkCount(silentMarkup)}`);

    visible.dispatch("click");
    const visibleMarkup = __oslHubUiTest.renderSilentVisible();
    expect(__oslHubUiTest.snapshot().silentVisibleMode).toBe("VISIBLE");
    expect(silentVisibleDisplayMark("VISIBLE")).toBe("VISIBLE");
    expect(displayMarkCount(visibleMarkup)).toBe(1);
    record(`TASK0335_VISIBLE saved=${__oslHubUiTest.snapshot().silentVisibleMode} displayMark=${silentVisibleDisplayMark("VISIBLE")} displayMarkCount=${displayMarkCount(visibleMarkup)}`);

    unknown.dispatch("click");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "silent-visible", silentVisibleMode: "VISIBLE" });
    record(`TASK0335_UNKNOWN saved=${__oslHubUiTest.snapshot().silentVisibleMode} route=${__oslHubUiTest.snapshot().onboardingRoute}`);

    cont.dispatch("click");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "passwords", silentVisibleMode: "VISIBLE" });
    record(`TASK0335_CONTINUE route=${__oslHubUiTest.snapshot().onboardingRoute} saved=${__oslHubUiTest.snapshot().silentVisibleMode}`);

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "silent-visible" });
    back.clearListeners();
    __oslHubUiTest.bindOnboarding();
    back.dispatch("click");
    expect(__oslHubUiTest.snapshot()).toMatchObject({ onboardingRoute: "cover", silentVisibleMode: null });
    record(`TASK0335_BACK route=${__oslHubUiTest.snapshot().onboardingRoute} saved=${__oslHubUiTest.snapshot().silentVisibleMode}`);

    expect(onboardingSilentVisibleMarkup(null)).toContain('id="continue-silent-visible" disabled');
  }, 30_000);
});
