import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  const makeToast = () => ({
    className: "",
    role: "",
    textContent: "",
    classList: { add: vi.fn() },
    addEventListener: vi.fn(),
    remove: vi.fn(),
  });
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => makeToast()), documentElement: { classList: { add: vi.fn() }, dataset: {} }, body: { append: vi.fn() }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
  mocks.invoke.mockReset();
});

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Mullvad integration", () => {
  it("renders Mullvad card without VPN content privacy claims", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "connections", mullvadAvailability: "installed" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-connection-card="mullvad"');
    expect(html).toContain('data-privacy-scope="networkOnly"');
    expect(html).toContain('data-connection-state="installed"');
    expect(html).toContain("Network privacy signal only.");
    expect(html).toContain("Platforms and recipients can still see ordinary content you send there.");
    expect(html).not.toMatch(/OSL-built VPN|message content private from|recipient cannot see|platform cannot see/i);
  });

  it("exposes Mullvad card markup as a network-only helper", () => {
    const { mullvadConnectionCardMarkup } = ui;
    const markup = mullvadConnectionCardMarkup({
      availability: "installed",
      integrationState: "availableToOpen",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    const copy = visibleText(markup);

    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(markup).toContain('data-privacy-scope="networkOnly"');
    expect(copy).toMatch(/Network privacy only/iu);
    expect(copy).toMatch(/Network privacy signal only/iu);
    expect(copy).toMatch(/separate network tool/iu);
    expect(copy).toMatch(/does not read its account state, connection state, or app content/iu);
    expect(copy).not.toMatch(/message content private|private messages|end-to-end encrypted|anonymous browsing|hides your content|OSL protects Mullvad content/iu);
  });

  it("records every Mullvad setup control against found and absent sessions", async () => {
    const { __oslHubUiTest } = ui;
    const routeKey = "osl-mullvad-setup-route-v1";
    const records: Record<string, unknown> = {};

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "installed" });
    records.initialSavedRoute = localStore.get(routeKey) ?? null;

    records.foundSessionAccepted = __oslHubUiTest.confirmMullvadFoundSession();
    records.foundSessionSavedRoute = localStore.get(routeKey) ?? null;
    records.foundSessionPage = __oslHubUiTest.snapshot().onboardingRoute;

    localStore.delete(routeKey);
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "installable" });
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    mocks.invoke.mockResolvedValueOnce({ started: true });
    await __oslHubUiTest.openMullvadInstallPage();
    records.installCommand = mocks.invoke.mock.calls.at(-1)?.[0] ?? null;
    records.installNotice = __oslHubUiTest.snapshot().mullvadSetupNotice;
    records.installPage = __oslHubUiTest.snapshot().onboardingRoute;

    localStore.set(routeKey, "found-session");
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "installed" });
    records.continueAccepted = __oslHubUiTest.continueMullvadSetup();
    records.continueSavedRoute = localStore.get(routeKey) ?? null;
    records.continuePage = __oslHubUiTest.snapshot().onboardingRoute;

    localStore.delete(routeKey);
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "unavailable" });
    __oslHubUiTest.skipMullvadSetup();
    records.notNowSavedRoute = localStore.get(routeKey) ?? null;
    records.notNowPage = __oslHubUiTest.snapshot().onboardingRoute;

    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "installed" });
    __oslHubUiTest.backFromMullvadSetup();
    records.backPage = __oslHubUiTest.snapshot().onboardingRoute;

    localStore.set(routeKey, "no-mullvad");
    __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "mullvad", mullvadAvailability: "unavailable" });
    const absentBefore = { savedRoute: localStore.get(routeKey) ?? null, page: __oslHubUiTest.snapshot().onboardingRoute };
    records.absentFoundSessionAccepted = __oslHubUiTest.confirmMullvadFoundSession();
    records.absentSavedRouteBefore = absentBefore.savedRoute;
    records.absentSavedRouteAfter = localStore.get(routeKey) ?? null;
    records.absentPageBefore = absentBefore.page;
    records.absentPageAfter = __oslHubUiTest.snapshot().onboardingRoute;

    console.log("TASK0346_MULLVAD_CONTROLS", JSON.stringify(records));

    expect(records).toMatchObject({
      initialSavedRoute: null,
      foundSessionAccepted: true,
      foundSessionSavedRoute: "found-session",
      foundSessionPage: "mullvad",
      installCommand: "install_mullvad",
      installNotice: "Mullvad install page opened",
      installPage: "mullvad",
      continueAccepted: true,
      continueSavedRoute: "found-session",
      continuePage: "apps",
      notNowSavedRoute: "no-mullvad",
      notNowPage: "apps",
      backPage: "cover",
      absentFoundSessionAccepted: false,
      absentSavedRouteBefore: "no-mullvad",
      absentSavedRouteAfter: "no-mullvad",
      absentPageBefore: "mullvad",
      absentPageAfter: "mullvad",
    });
  });
});
