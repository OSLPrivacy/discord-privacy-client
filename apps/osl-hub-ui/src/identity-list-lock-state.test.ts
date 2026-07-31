import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

const slot = { slotId: "id-YO3kIbux4ViXwYnAOoLm_x-_", label: "Primary identity", oslUserId: "osl_9c1f2b", active: true };

function unlockedClaim(html: string): boolean {
  return html.includes("Password configured and unlocked");
}

describe("Settings > Account agrees with itself about the lock", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows the identity list when the session is unlocked", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [slot], hubIdentitiesLoad: "loaded" });

    const account = __oslHubUiTest.renderSettingsSection("account");

    expect(unlockedClaim(account)).toBe(true);
    expect(account).toContain("Primary identity");
    expect(account).toContain("osl_9c1f2b");
    expect(account).not.toContain("Unlock OSL to manage encrypted identity slots");
    expect(account).not.toContain('data-identity-list="locked"');
  });

  it("never tells an unlocked session to unlock, whatever the list load did", async () => {
    const { __oslHubUiTest } = await loadUi();

    for (const load of ["pending", "unavailable", "loaded"] as const) {
      __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: load });
      const account = __oslHubUiTest.renderSettingsSection("account");

      expect(unlockedClaim(account), `unlocked indicator for load=${load}`).toBe(true);
      expect(account, `lock claim for load=${load}`).not.toContain("Unlock OSL to manage encrypted identity slots");
      expect(account, `lock state for load=${load}`).not.toContain('data-identity-list="locked"');
    }
  });

  it("distinguishes a refused load from an account with no identities", async () => {
    const { __oslHubUiTest } = await loadUi();

    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });
    const refused = __oslHubUiTest.renderSettingsSection("account");
    expect(refused).toContain('data-identity-list="unavailable"');
    expect(refused).toContain("retry-identity-list");

    __oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "loaded" });
    const empty = __oslHubUiTest.renderSettingsSection("account");
    expect(empty).toContain('data-identity-list="empty"');
    expect(empty).not.toContain("retry-identity-list");
  });

  it("still asks a locked session to unlock", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ coreReady: false, bootstrapStatus: "passwordRequired", hubIdentities: [], hubIdentitiesLoad: "pending" });

    const account = __oslHubUiTest.renderSettingsSection("account");

    expect(unlockedClaim(account)).toBe(false);
    expect(account).toContain('data-identity-list="locked"');
    expect(account).toContain("Unlock OSL to manage encrypted identity slots");
  });

});
