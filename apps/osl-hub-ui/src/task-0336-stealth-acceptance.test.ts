import { beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: () => "", providerLogo: () => "", serviceLogo: () => "" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: native.emitTo, listen: native.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: native.getCurrentWindow }));

type Handler = (event: { preventDefault(): void; currentTarget: FakeElement; target: FakeElement }) => unknown;

class FakeElement {
  declare innerHTML: string;
  value = "";
  type = "";
  disabled = false;
  textContent = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Handler[]>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  elements = { namedItem: (name: string) => this.fields.get(name) ?? null };
  fields = new Map<string, FakeElement>();
  constructor(readonly tagName: string, readonly id = "") {}
  addEventListener(type: string, handler: Handler): void { this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]); }
  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  removeAttribute(name: string): void { this.attributes.delete(name); }
  getAttribute(name: string): string | null { return this.attributes.get(name) ?? null; }
  querySelector(selector: string): FakeElement | null {
    if (selector === "[data-onboarding-role-error]") return this.fields.get("error") ?? null;
    if (selector === "#workspace-render-surface") return this.fields.get("workspace-render-surface") ?? null;
    return null;
  }
  querySelectorAll(_selector: string): FakeElement[] { return []; }
  focus(): void {}
  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) await handler({ preventDefault: () => undefined, currentTarget: this, target: this });
  }
}

type Controls = {
  current?: FakeElement; alternate?: FakeElement; confirm?: FakeElement; show?: FakeElement;
  save?: FakeElement; continue?: FakeElement; back?: FakeElement; unlock?: FakeElement; unlockForm?: FakeElement;
};

function harness() {
  const app = new FakeElement("DIV", "app");
  const workspaceSurface = new FakeElement("DIV", "workspace-render-surface");
  let page = "unknown";
  let renderedMarkup = "";
  let controls: Controls = {};
  const render = (markup: string): void => {
    renderedMarkup = markup;
    controls = {};
    if (markup.includes('data-onboarding-password-role="stealth"')) {
      page = "stealth";
      const form = new FakeElement("FORM", "setup-stealth-form");
      form.dataset.onboardingPasswordRole = "stealth";
      form.dataset.onboardingPasswordNext = "burnpass";
      const current = new FakeElement("INPUT", "setup-stealth-current");
      const alternate = new FakeElement("INPUT", "setup-stealth-alternate");
      const confirm = new FakeElement("INPUT", "setup-stealth-confirm");
      current.type = alternate.type = confirm.type = "password";
      form.fields.set("current", current); form.fields.set("alternate", alternate); form.fields.set("confirm", confirm); form.fields.set("error", new FakeElement("P"));
      controls = { current, alternate, confirm, save: new FakeElement("BUTTON"), show: new FakeElement("BUTTON", "show-stealth-password") };
      controls.save!.disabled = /data-onboarding-role-submit disabled/u.test(markup);
      controls.show!.dataset.passwordToggle = "setup-stealth-alternate";
      controls.back = markup.includes('id="onboarding-back"') ? new FakeElement("BUTTON", "onboarding-back") : undefined;
      controls.save!.dataset.onboardingRoleSubmit = "";
      form.fields.set("form", form);
      controls.unlockForm = form;
    } else if (markup.includes('data-password-role-next="burnpass"')) {
      page = "stealth-ready";
      controls.continue = new FakeElement("BUTTON"); controls.continue.dataset.passwordRoleNext = "burnpass";
      controls.back = markup.includes('id="onboarding-back"') ? new FakeElement("BUTTON", "onboarding-back") : undefined;
    } else if (markup.includes('data-onboarding-password-role="burn"')) page = "burnpass";
    else if (markup.includes('class="cv-onboarding"')) page = "visibility";
    else if (markup.includes('id="identity-password-form"')) {
      page = "unlock";
      const form = new FakeElement("FORM", "identity-password-form"); form.dataset.passwordMode = "unlock";
      const password = new FakeElement("INPUT", "identity-password"); password.type = "password";
      const submit = new FakeElement("BUTTON", "identity-password-submit"); submit.disabled = true;
      form.fields.set("password", password);
      controls = { unlockForm: form, unlock: submit, current: password };
      (controls as Controls & { error: FakeElement }).error = new FakeElement("P", "password-error");
    } else if (markup.includes("decoy-workspace")) page = "decoy";
    else if (markup.includes('class="hub-layout with-primary-sidebar"')) page = "real-workspace";
    else page = "unknown";
  };
  app.fields.set("workspace-render-surface", workspaceSurface);
  Object.defineProperty(app, "innerHTML", { set: render, get: () => renderedMarkup });
  Object.defineProperty(workspaceSurface, "innerHTML", { set: render, get: () => renderedMarkup });
  const node = (selector: string): FakeElement | null => {
    if (selector === "#app") return app;
    if (selector === "[data-onboarding-password-role]") return controls.unlockForm?.id === "setup-stealth-form" ? controls.unlockForm : null;
    if (selector === "[data-onboarding-role-submit]") return controls.save ?? null;
    if (selector === "#onboarding-back") return controls.back ?? null;
    if (selector === "#identity-password-form") return controls.unlockForm?.id === "identity-password-form" ? controls.unlockForm : null;
    if (selector === "#identity-password") return controls.current?.id === "identity-password" ? controls.current : null;
    if (selector === "#identity-password-submit") return controls.unlock ?? null;
    if (selector === "#password-error") return (controls as Controls & { error?: FakeElement }).error ?? null;
    if (selector === "#setup-stealth-alternate") return controls.alternate ?? null;
    return null;
  };
  const all = (selector: string): FakeElement[] => {
    if (selector === "[data-password-toggle]") return controls.show ? [controls.show] : [];
    if (selector === "button[data-password-role-next]") return controls.continue ? [controls.continue] : [];
    return [];
  };
  return { app, page: () => page, markup: () => renderedMarkup, controls: () => controls, node, all };
}

const ready = {
  originalCoreLinked: true,
  identityLoaded: true,
  keyserverInitialised: true,
  groupSenderKeysEnabled: true,
  remoteServiceHasNativeAccess: true,
  bootstrapAttempted: true,
  passwordGateRequired: false,
  unlocked: true,
  activeOslUserId: "task-0336",
  bootstrapStatus: "ready",
  cloudRegistrationState: "registered",
  storageMethod: "os-keyring",
};
const roleStatus = { mainPasswordSet: true, stealthPasswordSet: true, burnPasswordSet: false, unlocked: true, stealthActionWired: true, burnActionWired: true };
const gate = (outcome: "unlocked" | "decoy" | "wrong") => ({
  outcome,
  lockoutSecondsRemaining: 0,
  attemptsUsed: outcome === "wrong" ? 1 : 0,
  readiness: outcome === "unlocked" ? ready : null,
  burn: null,
});

async function load(route: "passwords" | "unlock", onboardingComplete = false) {
  vi.resetModules();
  const view = harness();
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("HTMLElement", FakeElement); vi.stubGlobal("HTMLInputElement", FakeElement); vi.stubGlobal("HTMLTextAreaElement", FakeElement); vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined, clear: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(view.node), querySelectorAll: vi.fn(view.all), getElementById: vi.fn((id: string) => view.node(`#${id}`)), createElement: vi.fn((tag: string) => new FakeElement(tag)), body: new FakeElement("BODY"), documentElement: new FakeElement("HTML"), addEventListener: vi.fn(), visibilityState: "visible", activeElement: null });
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout: (fn: () => void) => { fn(); return 1; }, clearTimeout: vi.fn(), confirm: vi.fn(() => false) });
  vi.stubGlobal("setTimeout", (fn: () => void) => { fn(); return 1; });
  vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => { frames.push(callback); return frames.length; })); vi.stubGlobal("cancelAnimationFrame", vi.fn());
  native.getCurrentWindow.mockReturnValue({ isFocused: vi.fn(async () => true), isMaximized: vi.fn(async () => false), minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(), onResized: vi.fn(async () => () => undefined) });
  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: route, onboardingComplete, coreReady: false });
  view.app.innerHTML = route === "passwords" ? __oslHubUiTest.renderOnboardingCaptureShell("passwords") : __oslHubUiTest.renderOnboardingRoute("unlock");
  __oslHubUiTest.bindOnboarding();
  return { view, ui: __oslHubUiTest, flush: () => { while (frames.length) frames.shift()!(0); } };
}

describe("TASK 0336 stealth controls and unlock paths", () => {
  beforeEach(() => { vi.unstubAllGlobals(); native.invoke.mockReset(); native.listen.mockReset(); native.emitTo.mockReset(); native.getCurrentWindow.mockReset(); });

  it("calls every stealth control and all three gate outcomes at the production boundaries", async () => {
    let decoyUnlockCount = 0;
    native.invoke.mockImplementation(async (command: string, args?: { password?: string }) => {
      if (command === "set_hub_stealth_password") return roleStatus;
      if (command === "unlock_hub_password_gate") {
        if (args?.password === "normal-0336") return gate("unlocked");
        if (args?.password === "stealth-0336") {
          decoyUnlockCount += 1;
          return gate("decoy");
        }
        return gate("wrong");
      }
      if (command === "get_core_readiness") return ready;
      if (command === "list_linked_services" || command === "list_hub_identities" || command === "list_hub_people") return [];
      if (command === "get_hub_password_role_status") return roleStatus;
      if (command === "get_friend_profile") return null;
      return null;
    });
    const setup = await load("passwords");
    expect(setup.view.page()).toBe("stealth");
    console.log("TASK0336_START page=stealth decoy_unlock_count=0");

    const fields = setup.view.controls();
    fields.current!.value = "normal-0336"; fields.alternate!.value = "stealth-0336"; fields.confirm!.value = "stealth-0336";
    await fields.current!.dispatch("input"); await fields.alternate!.dispatch("input"); await fields.confirm!.dispatch("input");
    await fields.show!.dispatch("click");
    console.log(`TASK0336_SHOW_PASSWORD first_type=${fields.alternate!.type} value=${fields.alternate!.value}`);
    expect(fields.alternate!.type).toBe("text"); expect(fields.alternate!.value).toBe("stealth-0336");
    await fields.show!.dispatch("click");
    console.log(`TASK0336_HIDE_PASSWORD second_type=${fields.alternate!.type} value=${fields.alternate!.value}`);
    expect(fields.alternate!.type).toBe("password"); expect(fields.alternate!.value).toBe("stealth-0336");
    await fields.unlockForm!.dispatch("submit"); setup.flush();
    expect(native.invoke).toHaveBeenCalledWith("set_hub_stealth_password", { currentMain: "normal-0336", newStealth: "stealth-0336" });
    console.log(`TASK0336_SAVE_STEALTH_PASSWORD saved=stealth-0336 result_page=${setup.view.page()} route=${setup.ui.snapshot().onboardingRoute}`);
    expect(setup.view.page()).toBe("burnpass");

    setup.view.app.innerHTML = setup.ui.renderOnboardingCaptureShell("passwords"); setup.ui.bindOnboarding();
    await setup.view.controls().continue!.dispatch("click"); setup.flush();
    console.log(`TASK0336_CONTINUE result_page=${setup.view.page()}`);
    expect(setup.view.page()).toBe("burnpass");

    setup.view.app.innerHTML = setup.ui.renderOnboardingCaptureShell("passwords"); setup.ui.bindOnboarding();
    await setup.view.controls().back!.dispatch("click"); setup.flush();
    console.log(`TASK0336_BACK result_page=${setup.view.page()} route=${setup.ui.snapshot().onboardingRoute}`);
    expect(setup.view.page()).toBe("visibility");

    const normalUnlock = await load("unlock", true);
    normalUnlock.view.controls().current!.value = "normal-0336";
    await normalUnlock.view.controls().current!.dispatch("input");
    await normalUnlock.view.controls().unlockForm!.dispatch("submit");
    normalUnlock.flush();
    console.log(`TASK0336_NORMAL_UNLOCK entered=normal-0336 result_page=${normalUnlock.view.page()} route=${normalUnlock.ui.snapshot().route} decoy_unlock_count=${decoyUnlockCount}`);
    expect(native.invoke).toHaveBeenCalledWith("unlock_hub_password_gate", { password: "normal-0336" });
    expect(normalUnlock.view.page()).toBe("real-workspace");
    expect(normalUnlock.ui.snapshot().route).toBe("home");
    expect(decoyUnlockCount).toBe(0);

    const stealthUnlock = await load("unlock");
    stealthUnlock.view.controls().current!.value = "stealth-0336";
    await stealthUnlock.view.controls().current!.dispatch("input"); await stealthUnlock.view.controls().unlockForm!.dispatch("submit"); stealthUnlock.flush();
    console.log(`TASK0336_EXACT_STEALTH_UNLOCK result_page=${stealthUnlock.view.page()} decoy_unlock_count=${decoyUnlockCount}`);
    expect(stealthUnlock.view.page()).toBe("decoy"); expect(decoyUnlockCount).toBe(1);

    const near = await load("unlock");
    const pageBefore = near.view.page();
    const markupBefore = near.view.markup();
    near.view.controls().current!.value = "stealth-0336!";
    await near.view.controls().current!.dispatch("input"); await near.view.controls().unlockForm!.dispatch("submit"); near.flush();
    const pageUnchanged = near.view.page() === pageBefore && near.view.markup() === markupBefore;
    console.log(`TASK0336_NEAR_MATCH entered=stealth-0336! result=refused decoy_unlock_count=${decoyUnlockCount} page=${near.view.page()} page_unchanged=${pageUnchanged}`);
    expect(native.invoke).toHaveBeenCalledWith("unlock_hub_password_gate", { password: "stealth-0336!" });
    expect(pageUnchanged).toBe(true);
    expect(decoyUnlockCount).toBe(1);
  }, 30_000);
});
