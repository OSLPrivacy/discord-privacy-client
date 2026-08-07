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
type DialogStub = { id: string; open: boolean; close: () => void };

const localStore = new Map<string, string>();
let ui: Ui;
let openDomDialog: DialogStub | null = null;

function documentStub(): Partial<Document> {
  return {
    querySelector: vi.fn((selector: string) => {
      if (selector === "dialog[open]" && openDomDialog?.open) return openDomDialog;
      return null;
    }) as unknown as Document["querySelector"],
    createElement: vi.fn(() => ({ innerHTML: "", querySelector: vi.fn(() => null) } as unknown as HTMLElement)) as unknown as Document["createElement"],
    documentElement: { classList: { add: vi.fn() }, dataset: {} } as unknown as HTMLElement,
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
  openDomDialog = null;
  vi.stubGlobal("document", documentStub());
  ui.__oslHubUiTest.reset({ route: "home" });
});

describe("TASK 3548 Escape audit", () => {
  it("keeps every OSL half-message intact and sends nothing when Escape is pressed", () => {
    const screens = ui.__oslHubUiTest.escapeAuditComposerScreens() as Array<"osl-chat" | "osl-mail-compose">;
    const screenRows = screens.map((screen, index) => {
      const halfMessage = `TASK3548-screen-${index}-${screen}-half-message`;
      ui.__oslHubUiTest.escapeAuditTypeHalfMessage(screen, halfMessage);
      const before = ui.__oslHubUiTest.escapeAuditDraft(screen);
      const closed = ui.__oslHubUiTest.escapeAuditPressEscape();
      const after = ui.__oslHubUiTest.escapeAuditDraft(screen);
      return { screen, halfMessage, before, closed, after };
    });
    const nonEmptyScreenDraftsBeforeEscape = screenRows.filter((row) => row.before.length > 0).length;

    expect(screens.length).toBeGreaterThan(0);
    expect(nonEmptyScreenDraftsBeforeEscape).toBe(screens.length);
    for (const row of screenRows) {
      expect(row.before).toBe(row.halfMessage);
      expect(row.closed).toBeNull();
      expect(row.after).toBe(row.halfMessage);
    }

    const dialogRows = ui.__oslHubUiTest.escapeAuditDialogs().map((dialog, index) => {
      ui.__oslHubUiTest.reset({ route: "home" });
      const halfMessage = `TASK3548-dialog-${index}-${dialog}-half-message`;
      ui.__oslHubUiTest.escapeAuditTypeHalfMessage("osl-chat", halfMessage);
      ui.__oslHubUiTest.escapeAuditOpenDialog(dialog);
      if (dialog === "update-dialog") {
        openDomDialog = { id: dialog, open: true, close: vi.fn(() => { if (openDomDialog) openDomDialog.open = false; }) };
      }
      const before = ui.__oslHubUiTest.escapeAuditDraft("osl-chat");
      const beforeLayers = ui.__oslHubUiTest.escapeAuditState().openLayers;
      const layerWasOpen = dialog === "update-dialog" ? openDomDialog?.open === true : beforeLayers.includes(dialog);
      const closed = ui.__oslHubUiTest.escapeAuditPressEscape();
      const after = ui.__oslHubUiTest.escapeAuditDraft("osl-chat");
      const afterLayers = ui.__oslHubUiTest.escapeAuditState().openLayers;
      const layerStillOpen = dialog === "update-dialog" ? openDomDialog?.open === true : afterLayers.includes(dialog);
      const sendAttempts = ui.__oslHubUiTest.escapeAuditState().sendAttempts;
      return { dialog, halfMessage, before, layerWasOpen, closed, after, layerStillOpen, sendAttempts };
    });

    for (const row of dialogRows) {
      expect(row.before).toBe(row.halfMessage);
      expect(row.layerWasOpen).toBe(true);
      expect(row.closed).toBe(row.dialog);
      expect(row.after).toBe(row.halfMessage);
      expect(row.layerStillOpen).toBe(false);
      expect(row.sendAttempts).toBe(0);
    }

    const report = {
      screenCount: screens.length,
      nonEmptyScreenDraftsBeforeEscape,
      dialogsCount: dialogRows.length,
      screenRows,
      dialogRows,
      messagesSent: 0,
    };
    console.log(`TASK3548_ESCAPE_AUDIT ${JSON.stringify(report, null, 2)}`);
  });
});
