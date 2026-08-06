import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  prepareOslChatText: vi.fn(),
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
vi.mock("./adapters", async (importOriginal) => ({
  ...await importOriginal<typeof import("./adapters")>(),
  prepareOslChatText: mocks.prepareOslChatText,
}));

function testDocument() {
  const root = {
    innerHTML: "",
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
  };
  return {
    root,
    document: {
      querySelector: vi.fn((selector: string) => selector === "#app" ? root : null),
      querySelectorAll: vi.fn(() => []),
      createElement: vi.fn(() => ({
        innerHTML: "",
        querySelector: vi.fn(() => null),
        querySelectorAll: vi.fn(() => []),
      })),
      getElementById: vi.fn(() => null),
      documentElement: { classList: { add: vi.fn() }, dataset: {} },
      addEventListener: vi.fn(),
      activeElement: null,
      visibilityState: "visible",
    },
  };
}

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  const { document } = testDocument();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", document);
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("navigator", { onLine: true });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn(() => undefined));
  vi.stubGlobal("HTMLElement", class {});
  vi.stubGlobal("HTMLInputElement", class {});
  vi.stubGlobal("HTMLTextAreaElement", class {});
  vi.stubGlobal("HTMLSelectElement", class {});
  return import("./main");
}

describe("TASK 3550 Enter never accidentally sends on OSL Chat", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.prepareOslChatText.mockImplementation(async (plaintext: string) => ({
      messageId: `task3550-${mocks.prepareOslChatText.mock.calls.length.toString().padStart(4, "0")}`,
      expiresAt: 4_000_000_000,
      personToPersonE2ee: true,
      viewOnce: false,
      deliveredToOslInbox: true,
      plaintext,
    }));
  });

  it("reports every Enter state and only the deliberate enabled Send delivers", async () => {
    const { __oslHubUiTest } = await loadUi();

    const report = await __oslHubUiTest.auditOslChatEnterNeverAccidentalSendForTest();
    console.log(`TASK3550_ENTER_AUDIT ${JSON.stringify(report)}`);

    expect(report.states.map((state) => state.name)).toEqual([
      "screen-empty-thread",
      "screen-unverified-friend",
      "screen-not-ready",
      "dialog-chat-settings",
      "textbox-composer",
      "disabled-send-empty-draft",
      "disabled-send-oversize-draft",
      "disabled-send-busy",
      "deliberate-enabled-send",
    ]);
    expect(new Set(report.states.map((state) => state.mark)).size).toBe(report.states.length);
    expect(report.statesFound).toBe(report.statesTried);
    expect(report.deliberateDeliveries).toBe(1);
    expect(report.deliberateDeliveries).toBeGreaterThan(0);
    expect(report.accidentalDeliveries).toBe(0);
    for (const state of report.states.filter((entry) => entry.name !== "deliberate-enabled-send")) {
      expect(state.deliveries, state.name).toBe(0);
      expect(["retained", "refused"]).toContain(state.markDisposition);
    }
    expect(report.states.find((state) => state.name === "deliberate-enabled-send")?.markDisposition)
      .toBe("cleared-by-send");
    expect(mocks.prepareOslChatText).toHaveBeenCalledTimes(1);
    expect(mocks.prepareOslChatText.mock.calls[0]?.[0]).toBe("OSL3550-ENTER-MARK-deliberate-enabled-send");
  }, 30_000);
});
