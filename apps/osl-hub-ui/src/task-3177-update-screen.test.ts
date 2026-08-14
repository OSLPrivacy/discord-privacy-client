import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./styles.css", () => ({}));
vi.mock("./local-protected-sheet.css", () => ({}));
vi.mock("./friend-invite.css", () => ({}));
vi.mock("./recovery-screen.css", () => ({}));
vi.mock("./onboarding-mullvad.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Ui = typeof import("./main");

const localStore = new Map<string, string>();
let ui: Ui;

function documentStub(): Partial<Document> {
  return {
    querySelector: vi.fn(() => null) as unknown as Document["querySelector"],
    createElement: vi.fn(() => ({ innerHTML: "", querySelector: vi.fn(() => null) } as unknown as HTMLElement)) as unknown as Document["createElement"],
    documentElement: { classList: { add: vi.fn() }, dataset: {} } as unknown as HTMLElement,
    body: { append: vi.fn() } as unknown as HTMLElement,
    addEventListener: vi.fn(),
    visibilityState: "visible",
  };
}

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", documentStub());
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false), __TAURI_INTERNALS__: {} });
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
  vi.stubGlobal("document", documentStub());
  ui.__oslHubUiTest.reset({ route: "home" });
});

describe("TASK 3177 update screen", () => {
  it("shows the current version, the new version, and what changed", () => {
    const markup = ui.__oslHubUiTest.renderUpdateScreenForTest("0.4.2", "0.5.0", "Fixed the friend list scroll jump.");
    expect(markup).toContain("Update OSL");
    expect(markup).toContain("0.4.2");
    expect(markup).toContain("0.5.0");
    expect(markup).toContain("Fixed the friend list scroll jump.");
    expect(markup).toContain(">Update now<");
    expect(markup).toContain(">Not now<");
  });

  it("does not start the download until Update now is pressed", async () => {
    ui.__oslHubUiTest.renderUpdateScreenForTest("0.4.2", "0.5.0", "Notes");
    expect(mocks.invoke).not.toHaveBeenCalledWith("install_hub_update", expect.anything());

    mocks.invoke.mockResolvedValueOnce({ status: "no_update" });
    await ui.__oslHubUiTest.pressUpdateNowForTest();

    expect(mocks.invoke).toHaveBeenCalledWith("install_hub_update", { expectedVersion: "0.5.0" });
  });
});
