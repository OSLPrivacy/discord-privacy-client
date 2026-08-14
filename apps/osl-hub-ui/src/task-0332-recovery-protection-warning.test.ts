import { beforeEach, describe, expect, it, vi } from "vitest";
import { RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT } from "./recovery-kit";

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  setScreenshotProtection: vi.fn(async () => false),
  setRecoveryKitUnsaved: vi.fn(async (_unsaved: boolean) => true),
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: native.emitTo, listen: native.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: native.getCurrentWindow }));
vi.mock("./adapters", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./adapters")>();
  return {
    ...actual,
    captureProtectionEnforced: () => true,
    setScreenshotProtection: native.setScreenshotProtection,
    setHubRecoveryKitUnsaved: native.setRecoveryKitUnsaved,
  };
});

type Handler = (event: Record<string, unknown>) => unknown;

class FakeElement {
  value = "";
  type = "";
  disabled = false;
  checked = false;
  textContent = "";
  className = "";
  role = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Handler[]>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  private markup = "";
  onMarkup: ((markup: string) => void) | null = null;

  constructor(readonly tagName: string, readonly id = "") {}

  get innerHTML(): string { return this.markup; }
  set innerHTML(value: string) { this.markup = value; this.onMarkup?.(value); }
  addEventListener(type: string, handler: Handler): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }
  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
    if (name === "disabled") this.disabled = true;
  }
  removeAttribute(name: string): void {
    this.attributes.delete(name);
    if (name === "disabled") this.disabled = false;
  }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  querySelector(_selector: string): FakeElement | null { return null; }
  querySelectorAll(_selector: string): FakeElement[] { return []; }
  append(_child: FakeElement): void {}
  prepend(_child: FakeElement): void {}
  remove(): void {}
  focus(): void {}
  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) {
      await handler({ preventDefault: () => undefined, currentTarget: this, target: this });
    }
  }
}

type WarningControls = {
  retry: FakeElement;
  exactWords: FakeElement;
  showAnyway: FakeElement;
  remindLater: FakeElement;
};

function warningHarness() {
  const app = new FakeElement("DIV", "app");
  const body = new FakeElement("BODY");
  let page: "warning" | "phrases" | "pro" | "unknown" = "unknown";
  let controls: WarningControls | null = null;

  app.onMarkup = (markup) => {
    if (markup.includes('id="recovery-show-anyway-ack"')) {
      page = "warning";
      controls = {
        retry: new FakeElement("BUTTON", "retry-recovery-protection"),
        exactWords: new FakeElement("INPUT", "recovery-show-anyway-ack"),
        showAnyway: new FakeElement("BUTTON", "recovery-show-anyway"),
        remindLater: new FakeElement("BUTTON", "recovery-remind-later"),
      };
      const showAnywayMarkup = markup.match(/<button[^>]*id="recovery-show-anyway"[^>]*>/u)?.[0] ?? "";
      controls.showAnyway.disabled = /\sdisabled(?:\s|>)/u.test(showAnywayMarkup);
      if (/aria-disabled="true"/u.test(showAnywayMarkup)) {
        controls.showAnyway.setAttribute("aria-disabled", "true");
      }
    } else {
      controls = null;
      page = markup.includes('data-recovery-secret="identity"')
        ? "phrases"
        : markup.includes('id="activation-form"')
          ? "pro"
          : "unknown";
    }
  };

  const node = (selector: string): FakeElement | null => {
    if (selector === "#app") return app;
    if (!controls) return null;
    if (selector === "#retry-recovery-protection") return controls.retry;
    if (selector === "#recovery-show-anyway-ack") return controls.exactWords;
    if (selector === "#recovery-show-anyway") return controls.showAnyway;
    if (selector === "#recovery-remind-later") return controls.remindLater;
    return null;
  };

  return {
    app,
    body,
    node,
    all: (_selector: string): FakeElement[] => [],
    page: () => page,
    controls: (): WarningControls => {
      if (!controls) throw new Error(`warning controls unavailable on ${page}`);
      return controls;
    },
  };
}

function frameQueue() {
  let next = 1;
  const frames = new Map<number, FrameRequestCallback>();
  return {
    request(callback: FrameRequestCallback): number {
      const handle = next++;
      frames.set(handle, callback);
      return handle;
    },
    cancel(handle: number): void { frames.delete(handle); },
    flush(): void {
      while (frames.size > 0) {
        const pending = [...frames.entries()];
        frames.clear();
        for (const [, callback] of pending) callback(0);
      }
    },
  };
}

const SECRETS = {
  userId: "osl-task0332",
  identityPhrase: "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima",
  passwordPhrase: "mike november oscar papa quebec romeo sierra tango uniform victor whiskey xray",
};

async function loadWarning() {
  vi.resetModules();
  const harness = warningHarness();
  const frames = frameQueue();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => { storage.set(key, value); },
    removeItem: (key: string) => { storage.delete(key); },
    clear: () => { storage.clear(); },
  });
  vi.stubGlobal("HTMLElement", FakeElement);
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => harness.node(selector)),
    querySelectorAll: vi.fn((selector: string) => harness.all(selector)),
    getElementById: vi.fn((id: string) => harness.node(`#${id}`)),
    createElement: vi.fn((tag: string) => new FakeElement(tag.toUpperCase())),
    body: harness.body,
    documentElement: new FakeElement("HTML"),
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
  vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => frames.request(callback)));
  vi.stubGlobal("cancelAnimationFrame", vi.fn((handle: number) => frames.cancel(handle)));
  native.getCurrentWindow.mockReturnValue({
    isFocused: vi.fn(async () => true),
    isMaximized: vi.fn(async () => false),
    minimize: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    onResized: vi.fn(async () => () => undefined),
  });

  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({
    route: "onboarding",
    onboardingRoute: "recovery",
    coreReady: true,
    recoveryBundle: SECRETS,
  });
  harness.app.innerHTML = __oslHubUiTest.renderOnboardingRoute("recovery");
  __oslHubUiTest.bindOnboarding();
  expect(harness.page()).toBe("warning");
  return { harness, frames, ui: __oslHubUiTest };
}

function remindersRecorded(): number {
  return native.setRecoveryKitUnsaved.mock.calls.filter(([unsaved]) => unsaved === true).length;
}

describe("TASK 0332 recovery protection warning controls", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    native.invoke.mockReset();
    native.listen.mockReset();
    native.emitTo.mockReset();
    native.getCurrentWindow.mockReset();
    native.setScreenshotProtection.mockClear();
    native.setRecoveryKitUnsaved.mockClear();
  });

  it("records Retry, exact words, Show anyway, Remind me later, and one wrong word", async () => {
    const revealFlow = await loadWarning();
    let phraseReveals = 0;
    expect(revealFlow.harness.controls().showAnyway.disabled).toBe(true);
    console.log(`TASK0332_START page=${revealFlow.harness.page()} phrase_reveals=${phraseReveals} reminders=${remindersRecorded()}`);

    await revealFlow.harness.controls().retry.dispatch("click");
    revealFlow.frames.flush();
    console.log(`TASK0332_RETRY result_page=${revealFlow.harness.page()} protection_calls=${native.setScreenshotProtection.mock.calls.length}`);
    expect(revealFlow.harness.page()).toBe("warning");
    expect(native.setScreenshotProtection).toHaveBeenCalledTimes(1);

    const exact = revealFlow.harness.controls();
    exact.exactWords.value = RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT;
    await exact.exactWords.dispatch("input");
    console.log(`TASK0332_EXACT_WORDS required="${RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT}" show_anyway_enabled=${!exact.showAnyway.disabled}`);
    expect(exact.showAnyway.disabled).toBe(false);
    expect(exact.showAnyway.getAttribute("aria-disabled")).toBeNull();

    await exact.showAnyway.dispatch("click");
    revealFlow.frames.flush();
    if (revealFlow.harness.page() === "phrases"
      && revealFlow.harness.app.innerHTML.includes(SECRETS.identityPhrase)
      && revealFlow.harness.app.innerHTML.includes(SECRETS.passwordPhrase)) phraseReveals += 1;
    console.log(`TASK0332_SHOW_ANYWAY result_page=${revealFlow.harness.page()} phrase_reveals=${phraseReveals} reminders=${remindersRecorded()}`);
    expect(revealFlow.harness.page()).toBe("phrases");
    expect(phraseReveals).toBe(1);
    expect(remindersRecorded()).toBe(0);

    native.setRecoveryKitUnsaved.mockClear();
    const reminderFlow = await loadWarning();
    await reminderFlow.harness.controls().remindLater.dispatch("click");
    reminderFlow.frames.flush();
    await Promise.resolve();
    await Promise.resolve();
    console.log(`TASK0332_REMIND_ME_LATER result_page=${reminderFlow.harness.page()} route=${reminderFlow.ui.snapshot().onboardingRoute} reminders=${remindersRecorded()} phrase_reveals=0`);
    expect(reminderFlow.harness.page()).toBe("pro");
    expect(reminderFlow.ui.snapshot().onboardingRoute).toBe("pro");
    expect(remindersRecorded()).toBe(1);

    native.setRecoveryKitUnsaved.mockClear();
    const wrongFlow = await loadWarning();
    const wrong = wrongFlow.harness.controls();
    const unchangedPage = wrongFlow.harness.app.innerHTML;
    wrong.exactWords.value = "show later";
    await wrong.exactWords.dispatch("input");
    expect(wrong.showAnyway.disabled).toBe(true);
    // A disabled native button cannot normally dispatch click. Calling its
    // bound handler directly also proves the reducer refuses a forged event.
    await wrong.showAnyway.dispatch("click");
    wrongFlow.frames.flush();
    await Promise.resolve();
    console.log(`TASK0332_WRONG_WORD entered="show later" refused=${wrongFlow.harness.page() === "warning"} phrase_reveals=0 reminders=${remindersRecorded()} page_unchanged=${wrongFlow.harness.app.innerHTML === unchangedPage}`);
    expect(wrongFlow.harness.page()).toBe("warning");
    expect(wrongFlow.ui.snapshot().onboardingRoute).toBe("recovery");
    expect(wrongFlow.harness.app.innerHTML).toBe(unchangedPage);
    expect(remindersRecorded()).toBe(0);
  }, 30_000);
});
