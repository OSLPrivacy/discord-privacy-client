import { beforeEach, describe, expect, it, vi } from "vitest";
import { RECOVERY_REVEAL_WRONG_PASSWORD_ERROR } from "./recovery-reveal";

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

class FakeElement {
  tagName: string;
  id: string;
  value = "";
  type = "";
  disabled = false;
  checked = false;
  textContent = "";
  innerHTML = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Array<(event: Record<string, unknown>) => unknown>>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };

  constructor(tagName: string, id = "") {
    this.tagName = tagName;
    this.id = id;
  }

  addEventListener(type: string, handler: (event: Record<string, unknown>) => unknown): void {
    const handlers = this.handlers.get(type) ?? [];
    handlers.push(handler);
    this.handlers.set(type, handlers);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  focus(): void {}

  querySelector(): FakeElement | null {
    return null;
  }

  querySelectorAll(selector: string): FakeElement[] {
    return selector === 'input[type="password"]' ? [] : [];
  }

  async dispatch(type: string, event: Record<string, unknown> = {}): Promise<void> {
    // Rendering binds the replacement DOM while a submit is outstanding. Use
    // the listeners that belonged to this press, just as a browser event does.
    for (const handler of [...(this.handlers.get(type) ?? [])]) {
      await handler({ preventDefault: () => undefined, currentTarget: this, ...event });
    }
  }
}

interface RecoveryHarness {
  app: FakeElement;
  form: FakeElement;
  password: FakeElement;
  passwordEye: FakeElement;
  error: FakeElement;
  nodes: Map<string, FakeElement>;
}

function recoveryHarness(): RecoveryHarness {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "recovery-reveal-form");
  const password = new FakeElement("INPUT", "recovery-reveal-password");
  password.type = "password";
  const passwordEye = new FakeElement("BUTTON");
  passwordEye.dataset.passwordToggle = "recovery-reveal-password";
  passwordEye.setAttribute("aria-label", "Show password");
  const error = new FakeElement("P", "recovery-reveal-error");
  const nodes = new Map<string, FakeElement>([
    ["#app", app],
    ["#recovery-reveal-form", form],
    ["#recovery-reveal-password", password],
    ["#recovery-reveal-error", error],
  ]);
  return { app, form, password, passwordEye, error, nodes };
}

async function loadRecoveryUi(harness: RecoveryHarness) {
  vi.resetModules();
  const store = new Map<string, string>();
  const documentElement = new FakeElement("HTML");
  const body = new FakeElement("BODY");
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("HTMLElement", FakeElement);
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => harness.nodes.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => selector === "[data-password-toggle]" ? [harness.passwordEye] : []),
    getElementById: vi.fn((id: string) => harness.nodes.get(`#${id}`) ?? null),
    createElement: vi.fn((tag: string) => new FakeElement(String(tag).toUpperCase())),
    body,
    documentElement,
    activeElement: null,
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    confirm: vi.fn(() => false),
    location: { reload: vi.fn() },
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  mocks.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), close: vi.fn(async () => undefined) });

  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "recovery", coreReady: true });
  expect(await __oslHubUiTest.loadRecoveryKitReminder()).toBe(true);
  return __oslHubUiTest;
}

function commandCalls(command: string): unknown[][] {
  return mocks.invoke.mock.calls.filter((call) => call[0] === command);
}

function heading(markup: string): string {
  return markup.match(/<h1[^>]*>([^<]+)<\/h1>/)?.[1] ?? "";
}

describe("TASK 0333 Finish saving recovery kit controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.emitTo.mockReset();
    mocks.getCurrentWindow.mockReset();
  });

  it("shows and re-hides the saved current password, then shows the exact saved kit once", async () => {
    const currentPassword = "correct horse battery staple";
    const savedKit = "abandon ability able about above absent absorb abstract absurd abuse access accident";
    let shownKits = 0;
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "get_hub_recovery_kit_unsaved") return true;
      if (command === "set_hub_screenshot_protection") return true;
      if (command === "view_hub_recovery_phrase") {
        expect(args).toEqual({ current: currentPassword });
        shownKits += 1;
        return savedKit;
      }
      throw new Error(`unexpected command ${command}`);
    });

    const harness = recoveryHarness();
    const ui = await loadRecoveryUi(harness);
    const finishMarkup = ui.renderOnboardingRoute("recovery");
    expect(heading(finishMarkup)).toBe("Finish saving your recovery kit");
    expect(finishMarkup).toContain("Show my recovery kit");
    expect(ui.recoveryKitSnapshot()).toMatchObject({ mode: "reveal-required", kitUnsaved: true, hasSecrets: false });
    console.log(`TASK0333_START heading=${heading(finishMarkup)} shown_kits=${shownKits} control=Show_my_recovery_kit`);

    ui.bindOnboarding();
    harness.password.value = currentPassword;
    await harness.passwordEye.dispatch("click");
    console.log(`TASK0333_SHOW_PASSWORD first_press_type=${harness.password.type} value=${harness.password.value} aria_label=${harness.passwordEye.getAttribute("aria-label")}`);
    expect(harness.password.type).toBe("text");
    expect(harness.password.value).toBe(currentPassword);
    expect(harness.passwordEye.getAttribute("aria-label")).toBe("Hide password");

    await harness.passwordEye.dispatch("click");
    console.log(`TASK0333_SHOW_PASSWORD second_press_type=${harness.password.type} value=${harness.password.value} aria_label=${harness.passwordEye.getAttribute("aria-label")}`);
    expect(harness.password.type).toBe("password");
    expect(harness.password.value).toBe(currentPassword);
    expect(harness.passwordEye.getAttribute("aria-label")).toBe("Show password");

    await harness.form.dispatch("submit");
    await vi.waitFor(() => expect(ui.recoveryKitSnapshot().mode).toBe("kit"));
    const shownMarkup = ui.renderOnboardingRoute("recovery");
    console.log(`TASK0333_SHOW_RECOVERY_KIT password=${currentPassword} shown_kits=${shownKits} exact_saved_kit=${shownMarkup.includes(savedKit)} saved_kit=${savedKit}`);
    expect(shownKits).toBe(1);
    expect(commandCalls("view_hub_recovery_phrase")).toHaveLength(1);
    expect(ui.recoveryKitSnapshot()).toMatchObject({ mode: "kit", kitUnsaved: true, hasSecrets: true, revealError: null });
    expect(shownMarkup).toContain(`<code>${savedKit}</code>`);
  }, 30_000);

  it("refuses one wrong current password without showing a kit or leaving Finish saving", async () => {
    const wrongPassword = "incorrect horse battery staple";
    let shownKits = 0;
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "get_hub_recovery_kit_unsaved") return true;
      if (command === "set_hub_screenshot_protection") return true;
      if (command === "view_hub_recovery_phrase") return null;
      throw new Error(`unexpected command ${command}`);
    });

    const harness = recoveryHarness();
    const ui = await loadRecoveryUi(harness);
    const before = ui.renderOnboardingRoute("recovery");
    const beforeHeading = heading(before);
    expect(beforeHeading).toBe("Finish saving your recovery kit");
    expect(shownKits).toBe(0);

    ui.bindOnboarding();
    harness.password.value = wrongPassword;
    await harness.form.dispatch("submit");
    await vi.waitFor(() => expect(ui.recoveryKitSnapshot().revealError).toBe(RECOVERY_REVEAL_WRONG_PASSWORD_ERROR));
    const after = ui.renderOnboardingRoute("recovery");
    const afterHeading = heading(after);
    console.log(`TASK0333_WRONG_PASSWORD refused=true password=${wrongPassword} shown_kits=${shownKits} view_calls=${commandCalls("view_hub_recovery_phrase").length} before_heading=${beforeHeading} after_heading=${afterHeading} mode=${ui.recoveryKitSnapshot().mode}`);

    expect(commandCalls("view_hub_recovery_phrase")).toHaveLength(1);
    expect(shownKits).toBe(0);
    expect(afterHeading).toBe(beforeHeading);
    expect(after).toContain(RECOVERY_REVEAL_WRONG_PASSWORD_ERROR);
    expect(ui.recoveryKitSnapshot()).toMatchObject({
      mode: "reveal-required",
      kitUnsaved: true,
      hasSecrets: false,
      revealBusy: false,
      revealError: RECOVERY_REVEAL_WRONG_PASSWORD_ERROR,
    });
  }, 30_000);
});
