import { beforeEach, describe, expect, it, vi } from "vitest";
import { OSL_CHAT_KEY_CHANGED_REFUSAL_REASON } from "./osl-chats-view";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  emitTo: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  appWindow: {
    isFullscreen: vi.fn(() => Promise.resolve(false)),
    setFullscreen: vi.fn(() => Promise.resolve()),
    onResized: vi.fn(() => Promise.resolve(() => undefined)),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.appWindow }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => id,
  providerLogo: (id: string) => id,
  serviceLogo: (id: string) => id,
}));

function person(pendingKeyChange: boolean) {
  return {
    personId: "changed-key-friend",
    oslUserId: "OSLUSER-changed-key-friend",
    alias: "Rose",
    safetyNumber: "1234 5678",
    safetyNumberVerified: true,
    whitelistCount: 1,
    whitelistedScopes: [{ kind: "dm" as const, contextId: null, storageKey: "dm:task-5034", userSpecific: true }],
    whitelistedScopesTruncated: false,
    pendingKeyChange,
    reachBroadened: false,
    reachBroadenedAt: null,
    reachNarrowedScopes: [],
  };
}

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  const toasts: Array<{ className: string; role: string; textContent: string; classList: { add: () => void }; addEventListener: () => void }> = [];
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", {
    activeElement: null,
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({ className: "", role: "", textContent: "", classList: { add() {} }, addEventListener() {} })),
    body: { append: (toast: typeof toasts[number]) => { toasts.push(toast); } },
    documentElement: { classList: { add() {}, remove() {} }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.stubGlobal("HTMLInputElement", class {});
  vi.stubGlobal("HTMLTextAreaElement", class {});
  vi.stubGlobal("HTMLSelectElement", class {});
  return { ui: await import("./main"), toasts };
}

describe("TASK 5034 key-changed send starvation", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
  });

  it("refuses all four routes and whitelist writes until verification", async () => {
    let authoritativePerson = person(true);
    let prepared = 0;
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "list_hub_people") return [authoritativePerson];
      if (command === "verify_hub_friend_safety_number") {
        authoritativePerson = person(false);
        return undefined;
      }
      if (command === "prepare_osl_chat_text") {
        prepared += 1;
        return { messageId: `prepared-5034-${prepared}`, expiresAt: 4_000_000_000, personToPersonE2ee: true, viewOnce: false, deliveredToOslInbox: true };
      }
      return undefined;
    });

    const { ui, toasts } = await loadUi();
    ui.__oslHubUiTest.reset({ route: "osl-chat", coreReady: true, licenseAccess: "pro", hubPeople: [authoritativePerson] });
    ui.__oslHubUiTest.setOslChatForTest(authoritativePerson.personId, "blocked enter");

    const changedMarkup = ui.__oslHubUiTest.renderOslChatForTest();
    expect((changedMarkup.match(/data-osl-key-changed-banner=/gu) ?? [])).toHaveLength(1);
    const lockedRow = ui.__oslHubUiTest.renderOslChatSettingsForTest(authoritativePerson.personId);
    expect((lockedRow.match(/data-osl-chat-whitelist-state="not-verified"/gu) ?? [])).toHaveLength(1);
    expect(lockedRow).toContain("Not verified. Verify the new safety number before changing this whitelist.");
    expect(lockedRow).toMatch(/id="osl-chat-permission-toggle"[^>]*disabled aria-disabled="true"/u);
    await ui.__oslHubUiTest.toggleOslChatPermissionForTest();
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "set_active_hub_friend_permission")).toHaveLength(0);

    for (const route of ui.OSL_CHAT_SEND_ROUTES) {
      ui.__oslHubUiTest.setOslChatForTest(authoritativePerson.personId, `blocked ${route}`);
      await ui.__oslHubUiTest.sendOslChatForTest(route);
    }
    expect(ui.__oslHubUiTest.oslChatSendRouteAttemptsForTest()).toEqual({
      enter: 1,
      "send-button": 1,
      "send-later": 1,
      "queued-draft": 1,
    });
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "prepare_osl_chat_text")).toHaveLength(0);
    expect(ui.__oslHubUiTest.oslChatConversation(authoritativePerson.personId)).toHaveLength(0);
    expect(toasts.map((toast) => toast.textContent).filter((text) => text === OSL_CHAT_KEY_CHANGED_REFUSAL_REASON)).toHaveLength(5);

    await expect(ui.__oslHubUiTest.verifyHubPersonAndRefreshForTest(authoritativePerson.personId, "1234 5678")).resolves.toBe(true);
    expect(ui.__oslHubUiTest.renderOslChatForTest()).not.toContain("data-osl-key-changed-banner");
    const releasedRow = ui.__oslHubUiTest.renderOslChatSettingsForTest(authoritativePerson.personId);
    expect((releasedRow.match(/data-osl-chat-whitelist-state="available"/gu) ?? [])).toHaveLength(1);
    expect(releasedRow).toMatch(/id="osl-chat-permission-toggle"[^>]*>Revoke<\/button>/u);
    expect(releasedRow).not.toMatch(/id="osl-chat-permission-toggle"[^>]*disabled/u);

    for (const route of ui.OSL_CHAT_SEND_ROUTES) {
      ui.__oslHubUiTest.setOslChatForTest(authoritativePerson.personId, `released ${route}`);
      await ui.__oslHubUiTest.sendOslChatForTest(route);
    }
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "prepare_osl_chat_text")).toHaveLength(4);
    expect(ui.__oslHubUiTest.oslChatConversation(authoritativePerson.personId)).toHaveLength(4);

    await ui.__oslHubUiTest.toggleOslChatPermissionForTest();
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "set_active_hub_friend_permission")).toHaveLength(1);

    console.log(
      `TASK5034_UI banner=1 routesRefused=4 enter=0 sendButton=0 sendLater=0 queuedDraft=0 messagesBeforeVerify=0 whitelistRowsLocked=1 uiWhitelistWritesBeforeVerify=0 routesReleased=4 messagesAfterVerify=4 whitelistRowsReleased=1 uiWhitelistWritesAfterVerify=1 reason="${OSL_CHAT_KEY_CHANGED_REFUSAL_REASON}"`,
    );
  }, 30_000);
});
