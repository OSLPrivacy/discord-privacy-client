import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  assertBadRestoreFixtureResults,
  type RestoreCheck,
} from "./clean-device-restore";

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

const VALID_PHRASE = "abandon ability able about above absent absorb abstract absurd abuse access accident";
const RESTORE_PASSWORD = "correct horse battery staple";

/**
 * TASK 0457 owns this list. Adding a row automatically adds it to the aggregate
 * no-bad-fixture-ready proof; removing a row deliberately narrows that proof.
 */
const BAD_IMPORT_FIXTURES = [
  {
    id: "wrong-word-count",
    phrase: "abandon ability able about above absent absorb abstract absurd abuse access",
    rejectedCheck: "recovery-phrase-format",
  },
  {
    id: "package-for-another-destination",
    phrase: VALID_PHRASE,
    rejectedCheck: "recovery-package-identity",
  },
  {
    id: "changed-package-byte",
    phrase: VALID_PHRASE,
    rejectedCheck: "recovery-package-integrity",
  },
  {
    id: "device-protection-refused",
    phrase: VALID_PHRASE,
    rejectedCheck: "account-protection",
  },
  {
    id: "ready-proof-missing",
    phrase: VALID_PHRASE,
    rejectedCheck: "account-readiness",
  },
] as const satisfies readonly {
  id: string;
  phrase: string;
  rejectedCheck: RestoreCheck;
}[];

class FakeElement {
  value = "";
  type = "";
  disabled = false;
  textContent = "";
  innerHTML = "";
  dataset: Record<string, string> = {};
  attributes = new Map<string, string>();
  handlers = new Map<string, Array<(event: unknown) => unknown>>();
  classList = { add: vi.fn(), remove: vi.fn(), toggle: vi.fn() };
  firstElementChild: FakeElement | null = null;

  constructor(readonly tagName: string, readonly id = "") {}

  addEventListener(type: string, handler: (event: unknown) => unknown): void {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), handler]);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  querySelector(): FakeElement | null {
    return null;
  }

  querySelectorAll(): FakeElement[] {
    return [];
  }

  focus(): void {}

  async dispatch(type: string): Promise<void> {
    for (const handler of this.handlers.get(type) ?? []) {
      await handler({ preventDefault: () => undefined, currentTarget: this });
    }
  }
}

function restoreHarness() {
  const app = new FakeElement("DIV", "app");
  const form = new FakeElement("FORM", "identity-import-form");
  const phrase = new FakeElement("TEXTAREA", "identity-recovery-phrase");
  const password = new FakeElement("INPUT", "import-password");
  password.type = "password";
  const confirm = new FakeElement("INPUT", "import-password-confirm");
  confirm.type = "password";
  const submit = new FakeElement("BUTTON", "identity-import-submit");
  submit.disabled = true;
  submit.firstElementChild = new FakeElement("SPAN");
  submit.firstElementChild.textContent = "Restore";
  const error = new FakeElement("P", "import-error");
  const journey = new FakeElement("DIV", "restore-journey-status");
  const nodes = new Map<string, FakeElement>([
    ["#app", app],
    ["#identity-import-form", form],
    ["#identity-recovery-phrase", phrase],
    ["#import-password", password],
    ["#import-password-confirm", confirm],
    ["#identity-import-submit", submit],
    ["#import-error", error],
    ["#restore-journey-status", journey],
  ]);
  return {
    app,
    form,
    phrase,
    password,
    confirm,
    submit,
    error,
    journey,
    nodes,
    all(selector: string): FakeElement[] {
      void selector;
      return [];
    },
  };
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

function coreReadiness(ready: boolean): Record<string, unknown> {
  return {
    originalCoreLinked: ready,
    identityLoaded: ready,
    keyserverInitialised: ready,
    cloudRegistrationState: ready ? "registered" : "notAttempted",
    groupSenderKeysEnabled: ready,
    groupSenderKeysReachable: ready,
    remoteServiceHasNativeAccess: ready,
    bootstrapAttempted: true,
    passwordGateRequired: !ready,
    unlocked: ready,
    activeOslUserId: ready ? "osl_task0457" : null,
    bootstrapStatus: ready ? "ready" : "setupRequired",
    storageMethod: ready ? "os-keyring" : null,
  };
}

async function loadRestoreUi(harness: ReturnType<typeof restoreHarness>) {
  vi.resetModules();
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => harness.nodes.get(selector) ?? null),
    querySelectorAll: vi.fn((selector: string) => harness.all(selector)),
    getElementById: vi.fn((id: string) => harness.nodes.get(`#${id}`) ?? null),
    createElement: vi.fn((tag: string) => new FakeElement(String(tag).toUpperCase())),
    body: { append: vi.fn() },
    documentElement: new FakeElement("HTML"),
    addEventListener: vi.fn(),
    visibilityState: "visible",
    activeElement: null,
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  mocks.getCurrentWindow.mockReturnValue({
    isFocused: vi.fn(async () => true),
    close: vi.fn(async () => undefined),
  });

  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({
    route: "onboarding",
    onboardingRoute: "import",
    coreReady: false,
    bootstrapStatus: "notAttempted",
  });
  __oslHubUiTest.bindOnboarding();
  return __oslHubUiTest;
}

async function enterRestoreData(
  harness: ReturnType<typeof restoreHarness>,
  phrase: string,
): Promise<void> {
  harness.phrase.value = phrase;
  harness.password.value = RESTORE_PASSWORD;
  harness.confirm.value = RESTORE_PASSWORD;
  await harness.phrase.dispatch("input");
  await harness.password.dispatch("input");
  await harness.confirm.dispatch("input");
}

function resetNativeMocks(): void {
  mocks.invoke.mockReset();
  mocks.listen.mockReset();
  mocks.emitTo.mockReset();
  mocks.getCurrentWindow.mockReset();
}

describe("TASK 0457 clean-device restore journey", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    resetNativeMocks();
  });

  it("takes the valid-import fixture to Account ready with exactly one account", async () => {
    const harness = restoreHarness();
    const accountsCreated: string[] = [];
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "import_hub_osl_identity_phrase") {
        expect(args).toEqual({ recoveryPhrase: VALID_PHRASE });
        accountsCreated.push("osl_task0457");
        return {
          userId: "osl_task0457",
          identityRecoveryPhrase: null,
          storageMethod: "os-keyring",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password") {
        expect(args).toEqual({ password: RESTORE_PASSWORD });
        return {
          passwordRecoveryPhrase: VALID_PHRASE,
          encryptedStateReloadComplete: true,
          encryptedStateReloadIssueCount: 0,
          readiness: {
            accessState: "ready",
            identityLoaded: true,
            mainPasswordSet: true,
            unlocked: true,
            serviceNeutralIdentitySupported: true,
            canCreateIdentity: false,
            canImportIdentityPhrase: false,
            passwordAttemptsUsed: 0,
            passwordLockoutSecondsRemaining: 0,
          },
        };
      }
      if (command === "get_core_readiness") return coreReadiness(true);
      if (command === "list_linked_services") return [];
      if (command === "set_hub_recovery_kit_unsaved" || command === "set_screenshot_protection") return true;
      throw new Error(`unexpected command ${command}`);
    });

    const ui = await loadRestoreUi(harness);
    await enterRestoreData(harness, VALID_PHRASE);
    await harness.form.dispatch("submit");

    const snapshot = ui.cleanDeviceRestoreSnapshot();
    const readyMarkup = ui.renderOnboardingRoute("recovery");
    const importSuccesses = mocks.invoke.mock.calls.filter(([command]) => command === "import_hub_osl_identity_phrase").length;
    console.log(`TASK0457_VALID phase=${snapshot.phase} import_successes=${importSuccesses} accounts_created=${accountsCreated.length} account_ready_count=${(readyMarkup.match(/data-account-ready/gu) ?? []).length} recovery_words_exposed=${readyMarkup.includes(VALID_PHRASE) ? 1 : 0}`);

    expect(snapshot).toEqual({ phase: "account-ready", rejectedCheck: null });
    expect(importSuccesses).toBe(1);
    expect(accountsCreated).toEqual(["osl_task0457"]);
    expect(readyMarkup).toContain("Account ready");
    expect(readyMarkup).not.toContain(VALID_PHRASE);
  }, 30_000);

  it("keeps every task-owned bad-import fixture on Restore with one named refusal", async () => {
    const results: Array<{
      fixtureId: string;
      reachedReady: boolean;
      accountsCreated: number;
      refusalCount: number;
      rejectedCheck: string | null;
    }> = [];

    for (const fixture of BAD_IMPORT_FIXTURES) {
      vi.unstubAllGlobals();
      resetNativeMocks();
      const harness = restoreHarness();
      const accountsCreated: string[] = [];
      mocks.invoke.mockImplementation(async (command: string) => {
        if (command === "import_hub_osl_identity_phrase") {
          throw new Error(`[restore-check:${fixture.rejectedCheck}] fixture detail must stay private: ${fixture.phrase}`);
        }
        if (command === "get_core_readiness") return coreReadiness(false);
        throw new Error(`unexpected command ${command}`);
      });

      const ui = await loadRestoreUi(harness);
      await enterRestoreData(harness, fixture.phrase);
      await harness.form.dispatch("submit");

      const snapshot = ui.cleanDeviceRestoreSnapshot();
      const restoreMarkup = ui.renderOnboardingRoute("import");
      const refusalCount = (restoreMarkup.match(/data-restore-refusal(?:\s|>)/gu) ?? []).length;
      const importSuccesses = accountsCreated.length;
      const reachedReady = snapshot.phase === "account-ready";
      const stillOnRestore = ui.snapshot().onboardingRoute === "import";
      const namedExpectedCheck = restoreMarkup.includes(`data-rejected-check="${fixture.rejectedCheck}"`);
      const phraseExposed = restoreMarkup.includes(fixture.phrase);
      console.log(`TASK0457_BAD fixture=${fixture.id} phase=${snapshot.phase} route=${ui.snapshot().onboardingRoute} import_successes=${importSuccesses} accounts_created=${accountsCreated.length} refusal_count=${refusalCount} rejected_check=${snapshot.rejectedCheck} expected_check=${fixture.rejectedCheck} phrase_exposed=${phraseExposed ? 1 : 0}`);

      expect(stillOnRestore).toBe(true);
      expect(restoreMarkup).toContain("Restore your account");
      expect(snapshot.phase).toBe("refused");
      expect(snapshot.rejectedCheck).toBe(fixture.rejectedCheck);
      expect(namedExpectedCheck).toBe(true);
      expect(refusalCount).toBe(1);
      expect(importSuccesses).toBe(0);
      expect(accountsCreated).toHaveLength(0);
      expect(phraseExposed).toBe(false);
      results.push({
        fixtureId: fixture.id,
        reachedReady,
        accountsCreated: accountsCreated.length,
        refusalCount,
        rejectedCheck: snapshot.rejectedCheck,
      });
    }

    assertBadRestoreFixtureResults(BAD_IMPORT_FIXTURES.map(({ id }) => id), results);
    const badReadyCount = results.filter(({ reachedReady }) => reachedReady).length;
    console.log(`TASK0457_AGGREGATE bad_fixture_count=${BAD_IMPORT_FIXTURES.length} bad_ready_count=${badReadyCount}`);
    expect(badReadyCount).toBe(0);
  }, 30_000);

  it.skipIf(process.env.OSL_TASK0457_NEGATIVE_CONTROL !== "1")(
    "TASK0457_NEGATIVE_CONTROL quietly accepted bad fixture fails",
    () => {
      const fixture = BAD_IMPORT_FIXTURES[0];
      assertBadRestoreFixtureResults(
        BAD_IMPORT_FIXTURES.map(({ id }) => id),
        BAD_IMPORT_FIXTURES.map(({ id, rejectedCheck }) => id === fixture.id
          ? { fixtureId: id, reachedReady: true, accountsCreated: 1, refusalCount: 0, rejectedCheck: null }
          : { fixtureId: id, reachedReady: false, accountsCreated: 0, refusalCount: 1, rejectedCheck }),
      );
    },
  );
});
