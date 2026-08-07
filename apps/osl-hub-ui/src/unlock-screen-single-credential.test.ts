// D80 -- the unlock screen has ONE credential input.
//
// The screen used to render a second, visibly labelled `#identity-duress-pin`
// with the placeholder "Burn code". That advertised the existence of a duress
// mechanism to anyone who seized the device or glanced at the screen, which is
// the one thing a duress mechanism cannot survive: it converts "he might have a
// burn code" from a guess into a certainty, and it tells an adversary that a
// password they extracted may be the wrong one.
//
// There are three alternate credentials behind the single box -- stealth, burn
// and duress -- and none of them may be visible here. These tests assert over
// the RENDERED MARKUP and the ACCESSIBILITY TREE, not the source text, because
// a removed visual label that survives as an `aria-label`, an `sr-only` label,
// a `title`, an `autocomplete` token or a `data-` attribute re-advertises the
// mechanism to anything reading the AT tree -- and this project already drives
// the app through exactly that surface.

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
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// ---------------------------------------------------------------------------
// A deliberately small DOM. There is no jsdom in this package, so the unlock
// form is driven against hand-built nodes that implement exactly the surface
// `bindPasswordForm` touches. Anything it uses that is missing here throws,
// which is the behaviour we want -- a silent no-op would let these tests pass
// against a form that was never bound.
// ---------------------------------------------------------------------------

interface FakeElement {
  tagName: string;
  id: string;
  value: string;
  type: string;
  disabled: boolean;
  textContent: string;
  innerHTML: string;
  dataset: Record<string, string>;
  attributes: Map<string, string>;
  handlers: Map<string, ((event: unknown) => unknown)[]>;
  focusCount: number;
  classList: { add(name: string): void; toggle(name: string, force?: boolean): boolean };
  addEventListener(type: string, handler: (event: unknown) => unknown): void;
  setAttribute(name: string, value: string): void;
  removeAttribute(name: string): void;
  getAttribute(name: string): string | null;
  focus(): void;
  click(): Promise<void>;
  dispatch(type: string): Promise<void>;
  querySelector(selector: string): FakeElement | null;
  querySelectorAll(selector: string): FakeElement[];
}

function fakeElement(tagName: string, id = ""): FakeElement {
  return {
    tagName,
    id,
    value: "",
    type: tagName === "INPUT" ? "text" : "",
    disabled: false,
    textContent: "",
    innerHTML: "",
    dataset: {},
    attributes: new Map(),
    handlers: new Map(),
    focusCount: 0,
    classList: { add: () => undefined, toggle: () => false },
    addEventListener(type, handler) {
      const list = this.handlers.get(type) ?? [];
      list.push(handler);
      this.handlers.set(type, list);
    },
    setAttribute(name, value) { this.attributes.set(name, value); },
    removeAttribute(name) { this.attributes.delete(name); },
    getAttribute(name) { return this.attributes.get(name) ?? null; },
    focus() { this.focusCount += 1; },
    async click(): Promise<void> { await this.dispatch("click"); },
    async dispatch(type: string): Promise<void> {
      for (const handler of this.handlers.get(type) ?? []) await handler({ preventDefault: () => undefined, currentTarget: this });
    },
    querySelector: () => null,
    querySelectorAll: () => [],
  };
}

interface UnlockHarness {
  form: FakeElement;
  password: FakeElement;
  eyeButton: FakeElement;
  submitButton: FakeElement;
  forgotButton: FakeElement;
  backButton: FakeElement;
  error: FakeElement;
  nodes: Map<string, FakeElement>;
  allNodes: FakeElement[];
  /** Everything `showToast` (or anything else) appended to the document. */
  appended: FakeElement[];
  storage: Map<string, string>;
  submit(): Promise<void>;
}

function buildUnlockHarness(): UnlockHarness {
  const form = fakeElement("FORM", "identity-password-form");
  form.dataset.passwordMode = "unlock";
  const password = fakeElement("INPUT", "identity-password");
  password.type = "password";
  const eyeButton = fakeElement("BUTTON");
  eyeButton.dataset.passwordToggle = "identity-password";
  const submitButton = fakeElement("BUTTON", "identity-password-submit");
  submitButton.textContent = "Unlock";
  const forgotButton = fakeElement("BUTTON");
  forgotButton.dataset.onboarding = "account-recovery";
  forgotButton.textContent = "Forgot password?";
  const backButton = fakeElement("BUTTON");
  backButton.dataset.onboarding = "welcome";
  backButton.textContent = "Back";
  const error = fakeElement("P", "password-error");

  const nodes = new Map<string, FakeElement>([
    ["#identity-password-form", form],
    ["#identity-password", password],
    ["#identity-password-submit", submitButton],
    ["#password-error", error],
  ]);

  const allNodes = [form, password, eyeButton, submitButton, forgotButton, backButton, error];
  return {
    form,
    password,
    eyeButton,
    submitButton,
    forgotButton,
    backButton,
    error,
    nodes,
    allNodes,
    appended: [],
    storage: new Map(),
    async submit(): Promise<void> {
      const handlers = form.handlers.get("submit") ?? [];
      expect(handlers.length, "the unlock form was never bound").toBe(1);
      await handlers[0]({ preventDefault: () => undefined });
    },
  };
}

async function loadUi(harness: UnlockHarness) {
  vi.resetModules();
  const store = harness.storage;
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  const querySelectorAll = vi.fn((selector: string) => {
    if (selector === "[data-password-toggle]") return [harness.eyeButton];
    if (selector === "[data-onboarding]") return [harness.forgotButton, harness.backButton];
    return [];
  });
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => harness.nodes.get(selector) ?? null),
    querySelectorAll,
    getElementById: vi.fn((id: string) => harness.allNodes.find((node) => node.id === id) ?? null),
    createElement: vi.fn((tag: string) => fakeElement(String(tag).toUpperCase())),
    // `showToast` builds a div and appends it here. Recording the appends is
    // how the "announces nothing" test below can tell a silent branch from a
    // branch that talks.
    body: { append: (node: FakeElement) => { harness.appended.push(node); } },
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  vi.stubGlobal("HTMLElement", Object);
  vi.stubGlobal("HTMLInputElement", Object);
  vi.stubGlobal("HTMLTextAreaElement", Object);
  vi.stubGlobal("HTMLSelectElement", Object);
  // `isTauriRuntime()` is a `"__TAURI_INTERNALS__" in window` probe. Without it
  // `unlockHubPasswordGate` throws before it ever reaches the gate.
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { callback(0); return 1; });
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function gateResult(outcome: string, extra: Record<string, unknown> = {}): unknown {
  return {
    outcome,
    lockoutSecondsRemaining: 0,
    attemptsUsed: 0,
    readiness: null,
    burn: null,
    ...extra,
  };
}

const verifiedBurnReport = {
  localCleanupComplete: true,
  removedTargets: ["hub_core"],
  failedTargets: [],
  remoteUnregister: { identitiesFound: 1, succeeded: 1, failed: 0, unavailable: 0 },
  restartRequired: true,
  originalDiscordDataUntouched: true,
};

// ---------------------------------------------------------------------------
// Markup + accessibility-tree analysis of the rendered unlock screen.
// ---------------------------------------------------------------------------

interface RenderedNode {
  tag: string;
  attributes: Record<string, string>;
  text: string;
}

/** Flatten the rendered markup into tags, their attributes, and their text. */
function parseRendered(markup: string): RenderedNode[] {
  const nodes: RenderedNode[] = [];
  const tagPattern = /<([a-zA-Z][\w-]*)((?:\s+[^\s=/>]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*\/?>([^<]*)/g;
  let match = tagPattern.exec(markup);
  while (match !== null) {
    const attributes: Record<string, string> = {};
    const attributePattern = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
    let attribute = attributePattern.exec(match[2] ?? "");
    while (attribute !== null) {
      attributes[attribute[1].toLowerCase()] = attribute[2] ?? attribute[3] ?? attribute[4] ?? "";
      attribute = attributePattern.exec(match[2] ?? "");
    }
    nodes.push({ tag: match[1].toLowerCase(), attributes, text: (match[3] ?? "").trim() });
    match = tagPattern.exec(markup);
  }
  return nodes;
}

/**
 * Every string the accessibility tree can expose for this screen: accessible
 * names (`aria-label`, `aria-labelledby` targets, `<label for>`, `title`,
 * `placeholder`, and a button's own text), accessible descriptions
 * (`aria-describedby` targets, `title`), and -- because the AT tree is not the
 * only readable surface -- every attribute value and every text node.
 */
function accessibilitySurface(markup: string): string[] {
  const nodes = parseRendered(markup);
  const textById = new Map<string, string>();
  for (const node of nodes) {
    if (node.attributes.id) textById.set(node.attributes.id, node.text);
  }
  const labelForTarget = new Map<string, string>();
  for (const node of nodes) {
    if (node.tag === "label" && node.attributes.for) labelForTarget.set(node.attributes.for, node.text);
  }

  const surface: string[] = [];
  for (const node of nodes) {
    // Accessible name and description.
    if (node.attributes["aria-label"]) surface.push(node.attributes["aria-label"]);
    if (node.attributes.title) surface.push(node.attributes.title);
    if (node.attributes.placeholder) surface.push(node.attributes.placeholder);
    if (node.attributes.alt) surface.push(node.attributes.alt);
    if (node.attributes.value) surface.push(node.attributes.value);
    for (const key of ["aria-labelledby", "aria-describedby"] as const) {
      for (const id of (node.attributes[key] ?? "").split(/\s+/u).filter(Boolean)) {
        surface.push(textById.get(id) ?? "");
      }
    }
    if (node.attributes.id && labelForTarget.has(node.attributes.id)) {
      surface.push(labelForTarget.get(node.attributes.id) ?? "");
    }
    // Everything else that is readable: all attribute names and values (ids,
    // classes, `data-*` keys, `autocomplete` tokens) and all text nodes.
    for (const [name, value] of Object.entries(node.attributes)) {
      surface.push(name);
      surface.push(value);
    }
    surface.push(node.text);
  }
  return surface.filter((entry) => entry.length > 0);
}

/**
 * Vocabulary that would name or hint at an alternate credential. Deliberately
 * broader than the exact strings that used to be here, so that a rename to
 * "panic code" or "emergency password" is caught too.
 */
const ALTERNATE_CREDENTIAL_VOCABULARY = [
  /duress/iu,
  /\bburn\b/iu,
  /\bwipe\b/iu,
  /\bstealth\b/iu,
  /\bdecoy\b/iu,
  /\bpanic\b/iu,
  /\bself[- ]?destruct/iu,
  /\bemergency\b/iu,
  /\bdestroy\b/iu,
  /\berase\b/iu,
  /\bdistress\b/iu,
  /\bsecond password\b/iu,
  /\balternate password\b/iu,
  /\bcode\b/iu,
] as const;

function advertisements(markup: string): string[] {
  const found: string[] = [];
  for (const entry of accessibilitySurface(markup)) {
    for (const pattern of ALTERNATE_CREDENTIAL_VOCABULARY) {
      if (pattern.test(entry)) found.push(`${pattern.source} matched ${JSON.stringify(entry)}`);
    }
  }
  return found;
}

describe("D80 unlock screen renders one credential input", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
  });

  it("renders exactly one password input and one show/hide control", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);
    const markup = __oslHubUiTest.renderOnboardingRoute("unlock");
    const nodes = parseRendered(markup);

    expect(nodes.find((node) => node.tag === "h1")?.text).toBe("Unlock");

    const inputs = nodes.filter((node) => node.tag === "input");
    expect(inputs).toHaveLength(1);
    expect(inputs[0].attributes.id).toBe("identity-password");
    expect(inputs[0].attributes.type).toBe("password");

    const eyes = nodes.filter((node) => (node.attributes.class ?? "").includes("password-eye"));
    expect(eyes).toHaveLength(1);
    expect(eyes[0].attributes["data-password-toggle"]).toBe("identity-password");

    // The deleted field and everything that used to point at it.
    expect(markup).not.toContain("identity-duress-pin");
    expect(markup).not.toContain("data-duress-pin");
    // And no empty row left behind reserving space for it.
    expect(nodes.filter((node) => (node.attributes.class ?? "").includes("password-input-row"))).toHaveLength(1);
  }, MODULE_RELOAD_BUDGET_MS);

  it("routes the unlock screen's forgot-password control to password recovery", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);

    const unlock = __oslHubUiTest.renderOnboardingRoute("unlock");
    const recovery = __oslHubUiTest.renderOnboardingRoute("account-recovery");

    expect(unlock).toContain('data-onboarding="account-recovery"');
    expect(recovery).toContain('data-account-recovery-phrase');
    expect(recovery).toContain("Password reset");
  }, MODULE_RELOAD_BUDGET_MS);

  it("records show, unlock, forgot, back, and wrong-password results with one saved account", async () => {
    const savedAccountKey = "osl-saved-native-apps-v1";
    const harness = buildUnlockHarness();
    harness.storage.set("osl-saved-account-mode-v1", "use");
    harness.storage.set(savedAccountKey, JSON.stringify(["discord"]));
    const { __oslHubUiTest } = await loadUi(harness);
    await __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "unlock", onboardingComplete: true, coreReady: false, bootstrapStatus: "passwordRequired" });
    await __oslHubUiTest.bindOnboarding();

    harness.password.value = "saved-password";
    await harness.eyeButton.click();
    const firstShowResult = { type: harness.password.type, value: harness.password.value, label: harness.eyeButton.getAttribute("aria-label") };
    await harness.eyeButton.click();
    const secondShowResult = { type: harness.password.type, value: harness.password.value, label: harness.eyeButton.getAttribute("aria-label") };

    await harness.forgotButton.click();
    const forgotSnapshot = __oslHubUiTest.snapshot();
    const forgotMarkup = __oslHubUiTest.renderOnboardingRoute("account-recovery");

    __oslHubUiTest.renderOnboardingRoute("unlock");
    await __oslHubUiTest.bindOnboarding();
    await harness.backButton.click();
    const backSnapshot = __oslHubUiTest.snapshot();
    const backMarkup = __oslHubUiTest.renderOnboardingRoute("welcome");

    __oslHubUiTest.renderOnboardingRoute("unlock");
    harness.form.handlers.clear();
    __oslHubUiTest.bindUnlockForm();
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "unlock_hub_password_gate") {
        expect(args).toEqual({ password: "wrong-password" });
        return gateResult("wrong");
      }
      throw new Error(`unexpected command in wrong-password check: ${command}`);
    });
    harness.password.value = "wrong-password";
    harness.submitButton.disabled = false;
    await harness.submit();
    const wrongSnapshot = __oslHubUiTest.snapshot();
    const wrongResult = {
      error: harness.error.textContent,
      route: wrongSnapshot.route,
      onboardingRoute: wrongSnapshot.onboardingRoute,
      savedAccounts: JSON.parse(harness.storage.get(savedAccountKey) ?? "[]").length,
      submitText: harness.submitButton.textContent,
      inputDisabled: harness.password.disabled,
    };

    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "unlock_hub_password_gate") {
        expect(args).toEqual({ password: "saved-password" });
        return gateResult("unlocked", {
          readiness: {
            originalCoreLinked: true,
            identityLoaded: true,
            keyserverInitialised: true,
            groupSenderKeysEnabled: true,
            remoteServiceHasNativeAccess: true,
            bootstrapAttempted: true,
            passwordGateRequired: false,
            unlocked: true,
            activeOslUserId: "test-osl-user",
            bootstrapStatus: "ready",
            cloudRegistrationState: "registered",
            storageMethod: "os-keyring",
          },
        });
      }
      if (command === "get_core_readiness") {
        return {
          originalCoreLinked: true,
          identityLoaded: true,
          keyserverInitialised: true,
          groupSenderKeysEnabled: true,
          remoteServiceHasNativeAccess: true,
          bootstrapAttempted: true,
          passwordGateRequired: false,
          unlocked: true,
          activeOslUserId: "test-osl-user",
          bootstrapStatus: "ready",
          cloudRegistrationState: "registered",
          storageMethod: "os-keyring",
        };
      }
      if (command === "list_linked_services") return [];
      if (command === "get_hub_password_role_status") {
        return {
          mainPasswordSet: true,
          stealthPasswordSet: false,
          burnPasswordSet: false,
          unlocked: true,
          stealthActionWired: true,
          burnActionWired: true,
        };
      }
      throw new Error(`unexpected command in saved-password check: ${command}`);
    });
    harness.password.value = "saved-password";
    harness.submitButton.disabled = false;
    await harness.submit();
    const unlockSnapshot = __oslHubUiTest.snapshot();
    const recorded = {
      savedAccountCount: JSON.parse(harness.storage.get(savedAccountKey) ?? "[]").length,
      showFirst: firstShowResult,
      showSecond: secondShowResult,
      forgot: { route: forgotSnapshot.route, onboardingRoute: forgotSnapshot.onboardingRoute, heading: "Password reset" },
      back: { route: backSnapshot.route, onboardingRoute: backSnapshot.onboardingRoute, control: "Sign in" },
      wrong: wrongResult,
      unlock: { route: unlockSnapshot.route },
    };
    console.info(`OSL-0327 ${JSON.stringify(recorded)}`);

    expect(JSON.parse(harness.storage.get(savedAccountKey) ?? "[]")).toEqual(["discord"]);
    expect(firstShowResult).toEqual({ type: "text", value: "saved-password", label: "Hide password" });
    expect(secondShowResult).toEqual({ type: "password", value: "saved-password", label: "Show password" });
    expect(forgotSnapshot).toMatchObject({ route: "onboarding", onboardingRoute: "account-recovery" });
    expect(forgotMarkup).toContain("Password reset");
    expect(backSnapshot).toMatchObject({ route: "onboarding", onboardingRoute: "welcome" });
    expect(backMarkup).toContain('data-onboarding="unlock"');
    expect(backMarkup).toContain("Sign in");
    expect(wrongResult).toEqual({
      error: "Password not recognized.",
      route: "onboarding",
      onboardingRoute: "unlock",
      savedAccounts: 1,
      submitText: "Unlock",
      inputDisabled: false,
    });
    expect(unlockSnapshot.route).toBe("home");
  }, MODULE_RELOAD_BUDGET_MS);

  it("exposes nothing in the markup or the accessibility tree that names an alternate credential", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);
    const markup = __oslHubUiTest.renderOnboardingRoute("unlock");

    // The audit only means anything if it can see a real accessible name, so
    // prove the surface extractor is actually reading one before trusting it.
    const surface = accessibilitySurface(markup);
    expect(surface).toContain("Show password");
    expect(surface).toContain("Password");

    expect(advertisements(markup)).toEqual([]);
  }, MODULE_RELOAD_BUDGET_MS);

  it("says nothing about an alternate credential when the entry is rejected", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);
    __oslHubUiTest.renderOnboardingRoute("unlock");
    __oslHubUiTest.bindUnlockForm();
    mocks.invoke.mockResolvedValue(gateResult("wrong"));

    harness.password.value = "not-the-password";
    harness.submitButton.disabled = false;
    await harness.submit();

    expect(harness.error.textContent).toBe("Password not recognized.");
    expect(advertisements(harness.error.textContent)).toEqual([]);
    // The form comes back usable and focus returns to the one input there is.
    expect(harness.password.disabled).toBe(false);
    expect(harness.submitButton.disabled).toBe(false);
    expect(harness.password.focusCount).toBe(1);
    expect(__oslHubUiTest.snapshot().route).toBe("onboarding");
    expect(__oslHubUiTest.snapshot().onboardingRoute).toBe("unlock");
  }, 15_000);

  it("sends the typed value to the single gate command with no second argument", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);
    __oslHubUiTest.bindUnlockForm();
    mocks.invoke.mockResolvedValue(gateResult("wrong"));

    harness.password.value = "burn-code-9381";
    harness.submitButton.disabled = false;
    await harness.submit();

    const gateCalls = mocks.invoke.mock.calls.filter((call) => call[0] === "unlock_hub_password_gate");
    expect(gateCalls).toHaveLength(1);
    expect(gateCalls[0][1]).toEqual({ password: "burn-code-9381" });
    expect(Object.keys(gateCalls[0][1] as object)).toEqual(["password"]);
  }, 15_000);

  it("still fires the burn path when the burn code is typed into the one input", async () => {
    const harness = buildUnlockHarness();
    const { __oslHubUiTest } = await loadUi(harness);
    __oslHubUiTest.bindUnlockForm();
    mocks.invoke.mockResolvedValue(gateResult("burned", { burn: verifiedBurnReport }));

    harness.password.value = "burn-code-9381";
    harness.submitButton.disabled = false;
    await harness.submit();

    // Same command, same single argument as the main password above.
    const gateCalls = mocks.invoke.mock.calls.filter((call) => call[0] === "unlock_hub_password_gate");
    expect(gateCalls[0][1]).toEqual({ password: "burn-code-9381" });
    // And the burn outcome still lands the app back on a fresh welcome screen.
    const snapshot = __oslHubUiTest.snapshot();
    expect(snapshot.route).toBe("onboarding");
    expect(snapshot.onboardingRoute).toBe("welcome");
  }, 15_000);

  it("still fires the duress path, and the stealth path, from the same one input", async () => {
    for (const [outcome, expectedRoute] of [["duress", "welcome"], ["decoy", "decoy"]] as const) {
      const harness = buildUnlockHarness();
      const { __oslHubUiTest } = await loadUi(harness);
      __oslHubUiTest.bindUnlockForm();
      mocks.invoke.mockResolvedValue(gateResult(outcome));

      harness.password.value = "alternate-credential-1";
      harness.submitButton.disabled = false;
      await harness.submit();

      expect(__oslHubUiTest.snapshot().onboardingRoute).toBe(expectedRoute);
    }
  }, 30_000);

  it("announces nothing after a burn, a duress or a stealth unlock", async () => {
    for (const [outcome, extra] of [
      ["burned", { burn: verifiedBurnReport }],
      ["duress", {}],
      ["decoy", {}],
    ] as const) {
      const harness = buildUnlockHarness();
      const { __oslHubUiTest } = await loadUi(harness);
      __oslHubUiTest.bindUnlockForm();
      mocks.invoke.mockResolvedValue(gateResult(outcome, extra));

      harness.password.value = "alternate-credential-1";
      harness.submitButton.disabled = false;
      await harness.submit();

      // A toast reading "Verified local OSL cleanup completed" used to print a
      // confession onto the screen the adversary is holding, and "OSL signed
      // out on this device" told them the entry did something. Both are gone:
      // these branches land on the welcome screen in silence, which is what a
      // device that was never set up looks like.
      expect(
        harness.appended.map((node) => node.textContent),
        `the ${outcome} outcome announced itself`,
      ).toEqual([]);
    }
  }, 30_000);

  it("holds every outcome to the same transition deadline", async () => {
    const elapsed: Record<string, number[]> = {
      wrong: [],
      decoy: [],
      duress: [],
      burned: [],
    };
    for (const [outcome, extra] of [
      ["wrong", {}],
      ["decoy", {}],
      ["duress", {}],
      ["burned", { burn: verifiedBurnReport }],
    ] as const) {
      // A median discards an occasional event-loop stall from a loaded full
      // suite, while preserving a branch that consistently returns early.
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const harness = buildUnlockHarness();
        const { __oslHubUiTest } = await loadUi(harness);
        __oslHubUiTest.bindUnlockForm();
        mocks.invoke.mockResolvedValue(gateResult(outcome, extra));

        harness.password.value = "some-credential-1";
        harness.submitButton.disabled = false;
        const startedAt = Date.now();
        await harness.submit();
        elapsed[outcome].push(Date.now() - startedAt);
      }
    }

    const median = (samples: number[]): number => [...samples].sort((a, b) => a - b)[1];
    const durations = Object.fromEntries(
      Object.entries(elapsed).map(([outcome, samples]) => [outcome, median(samples)]),
    );

    // This is deliberately relative rather than a wall-clock floor: suite
    // contention may delay every outcome, but may not make one credential
    // class observably faster than another. A missing wait on one branch is
    // about 1.2 seconds faster and therefore remains well outside this window.
    const values = Object.values(durations);
    expect(Math.max(...values) - Math.min(...values), JSON.stringify(durations)).toBeLessThan(500);
  }, 90_000);
});
