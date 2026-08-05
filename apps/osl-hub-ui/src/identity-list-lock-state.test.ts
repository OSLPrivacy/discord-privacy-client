import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
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
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
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
});

const slot = { slotId: "id-YO3kIbux4ViXwYnAOoLm_x-_", label: "Primary identity", oslUserId: "osl_9c1f2b", active: true };

function unlockedClaim(html: string): boolean {
  return html.includes("Password configured and unlocked");
}

describe("Settings > Account agrees with itself about the lock", () => {
  it("shows the identity list when the session is unlocked", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [slot], hubIdentitiesLoad: "loaded" });

    const account = __oslHubUiTest.renderSettingsSection("account");

    expect(unlockedClaim(account)).toBe(true);
    expect(account).toContain("Primary identity");
    expect(account).toContain("osl_9c1f2b");
    expect(account).not.toContain("Unlock OSL to manage encrypted identity slots");
    expect(account).not.toContain('data-identity-list="locked"');
  });

  it("never tells an unlocked session to unlock, whatever the list load did", () => {
    const { __oslHubUiTest } = ui;

    for (const load of ["pending", "unavailable", "loaded"] as const) {
      __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: load });
      const account = __oslHubUiTest.renderSettingsSection("account");

      expect(unlockedClaim(account), `unlocked indicator for load=${load}`).toBe(true);
      expect(account, `lock claim for load=${load}`).not.toContain("Unlock OSL to manage encrypted identity slots");
      expect(account, `lock state for load=${load}`).not.toContain('data-identity-list="locked"');
    }
  });

  it("distinguishes a refused load from an account with no identities", () => {
    const { __oslHubUiTest } = ui;

    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });
    const refused = __oslHubUiTest.renderSettingsSection("account");
    expect(refused).toContain('data-identity-list="unavailable"');
    expect(refused).toContain("retry-identity-list");

    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "loaded" });
    const empty = __oslHubUiTest.renderSettingsSection("account");
    expect(empty).toContain('data-identity-list="empty"');
    expect(empty).not.toContain("retry-identity-list");
  });

  it("still asks a locked session to unlock", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ coreReady: false, bootstrapStatus: "passwordRequired", hubIdentities: [], hubIdentitiesLoad: "pending" });

    const account = __oslHubUiTest.renderSettingsSection("account");

    expect(unlockedClaim(account)).toBe(false);
    expect(account).toContain('data-identity-list="locked"');
    expect(account).toContain("Unlock OSL to manage encrypted identity slots");
  });

});
