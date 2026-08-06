import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

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

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({ classList: { add: vi.fn() }, addEventListener: vi.fn(), remove: vi.fn() })),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
    body: { append: vi.fn() },
  });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, clearTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("navigator", { onLine: true });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("busy buttons", () => {
  it("disables every long-running action button while running and re-enables after success and failure", () => {
    const rows = ui.__oslHubUiTest.longRunningButtonAudit();
    console.log(`BUSY_BUTTON_AUDIT ${JSON.stringify(rows)}`);

    expect(rows).toHaveLength(21);
    for (const row of rows) {
      expect(row.runningDisabled, `${row.action} (${row.button}) disabled while running`).toBe(true);
      expect(row.secondPressCount, `${row.action} (${row.button}) second-press count`).toBe(0);
      expect(row.afterSuccessDisabled, `${row.action} (${row.button}) disabled after success`).toBe(false);
      expect(row.afterFailureDisabled, `${row.action} (${row.button}) disabled after failure`).toBe(false);
    }
  });
});
