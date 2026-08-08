import { beforeEach, describe, expect, it, vi } from "vitest";

const storage = new Map<string, string>();
const MODULE_RELOAD_BUDGET_MS = 30_000;

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));

const account = {
  serviceId: "email" as const,
  accountId: "personal-mail",
  accountLabel: "Personal mail",
  openChoices: [
    { kind: "windowsApp" as const, label: "Mail" },
    { kind: "browser" as const, label: "Browser" },
  ],
};

async function loadUi() {
  vi.resetModules();
  return import("./main");
}

describe("TASK 0316 save setup app and account choices", () => {
  beforeEach(() => {
    storage.clear();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear(),
    });
  });

  it("reopens Browser and Ask after setup finishes, while a fresh profile uses defaults", async () => {
    const fresh = await loadUi();
    await fresh.loadUiPreferences();
    fresh.__oslHubUiTest.setDetectedAccountsForTest([account]);
    const freshMarkup = fresh.__oslHubUiTest.renderDetectedAppsForTest();
    expect(freshMarkup).toContain('data-detected-account-opening-choice="windowsApp"');
    expect(freshMarkup).toContain('aria-checked="false"');
    expect(fresh.__oslHubUiTest.savedAccountModeForTest()).toBe("ask");

    fresh.__oslHubUiTest.chooseDetectedAccountOpeningForTest(account, "browser");
    fresh.__oslHubUiTest.finishSetupChoicesForTest();

    const reopened = await loadUi();
    await reopened.loadUiPreferences();
    reopened.__oslHubUiTest.setDetectedAccountsForTest([account]);
    const reopenedMarkup = reopened.__oslHubUiTest.renderDetectedAppsForTest();
    expect(reopenedMarkup).toContain('data-detected-account-opening-choice="browser"');
    expect(reopenedMarkup).toContain('data-detected-account-opening-choice="browser">Browser · Browser</button>');
    expect(reopenedMarkup).toContain('data-detected-account-opening-choice="windowsApp">Windows app · Mail</button>');
    expect(reopenedMarkup).toMatch(/aria-checked="true"[^>]*data-detected-account-opening-choice="browser"/);
    expect(reopenedMarkup).toMatch(/aria-checked="false"[^>]*data-detected-account-opening-choice="windowsApp"/);
    expect(reopened.__oslHubUiTest.savedAccountModeForTest()).toBe("ask");

    console.log("TASK0316 fresh_route=Windows_app fresh_opening=Ask reopened_route=Browser reopened_opening=Ask");
  }, MODULE_RELOAD_BUDGET_MS);
});
