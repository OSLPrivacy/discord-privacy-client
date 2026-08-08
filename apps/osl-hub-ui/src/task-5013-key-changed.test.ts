import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
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
    whitelistCount: 0,
    whitelistedScopes: [],
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

describe("TASK 5013 key-changed OSL chat", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mocks.invoke.mockReset();
  });

  it("blocks every changed-key send until hub verification refreshes the active friend", async () => {
    let authoritativePerson = person(true);
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "list_hub_people") return [authoritativePerson];
      if (command === "verify_hub_friend_safety_number") {
        authoritativePerson = person(false);
        return undefined;
      }
      if (command === "prepare_osl_chat_text") {
        return { messageId: "prepared-5013", expiresAt: 4_000_000_000, personToPersonE2ee: true, viewOnce: false, deliveredToOslInbox: true };
      }
      return undefined;
    });
    const { ui, toasts } = await loadUi();
    ui.__oslHubUiTest.reset({ route: "osl-chat", coreReady: true, licenseAccess: "pro", hubPeople: [authoritativePerson] });
    ui.__oslHubUiTest.setOslChatForTest(authoritativePerson.personId, "blocked message");

    const changedMarkup = ui.__oslHubUiTest.renderOslChatForTest();
    expect((changedMarkup.match(/data-osl-key-changed-banner=/gu) ?? [])).toHaveLength(1);
    expect(changedMarkup).toContain('class="osl-chat-friend is-active is-key-changed"');
    expect(changedMarkup).toContain('class="osl-chat-key-changed-triangle" role="img" aria-label="Safety number changed">⚠</span>');
    expect(changedMarkup).toContain('<strong>Key changed</strong>');
    expect(changedMarkup).toContain(OSL_CHAT_KEY_CHANGED_REFUSAL_REASON);
    expect(changedMarkup).toContain(`data-verify-person="${authoritativePerson.personId}"`);
    expect(changedMarkup).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(changedMarkup).toMatch(/id="osl-chat-attach"[^>]* disabled/u);
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(styles).toMatch(/\.osl-chat-friend\.is-key-changed[^{]*\{[^}]*var\(--warn/isu);
    expect(styles).toMatch(/\.osl-chat-friend\.is-key-changed \.osl-chat-friend-copy strong,[\s\S]*?\{ color: var\(--warn\); \}/u);
    expect(styles).toMatch(/\.osl-chat-key-changed-banner[^{]*\{[^}]*var\(--warn/isu);

    await ui.__oslHubUiTest.sendOslChatForTest();
    await ui.__oslHubUiTest.sendOslChatForTest();
    await ui.__oslHubUiTest.sendOslChatAttachmentForTest();
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "prepare_osl_chat_text")).toHaveLength(0);
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "select_osl_chat_attachment")).toHaveLength(0);
    expect(toasts.map((toast) => toast.textContent)).toEqual([
      OSL_CHAT_KEY_CHANGED_REFUSAL_REASON,
      OSL_CHAT_KEY_CHANGED_REFUSAL_REASON,
      OSL_CHAT_KEY_CHANGED_REFUSAL_REASON,
    ]);
    expect(ui.__oslHubUiTest.oslChatConversation(authoritativePerson.personId)).toHaveLength(0);

    await expect(ui.__oslHubUiTest.verifyHubPersonAndRefreshForTest(authoritativePerson.personId, "1234 5678")).resolves.toBe(true);
    const verifiedMarkup = ui.__oslHubUiTest.renderOslChatForTest();
    expect(verifiedMarkup).not.toContain("data-osl-key-changed-banner");

    await ui.__oslHubUiTest.sendOslChatForTest();
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "prepare_osl_chat_text")).toHaveLength(1);
    expect(ui.__oslHubUiTest.oslChatConversation(authoritativePerson.personId)).toHaveLength(1);
    console.log(`TASK5013 bannerBeforeVerify=1 amberNames=1 warningTriangles=1 refusedAttempts=3 textPreparesBeforeVerify=0 attachmentSelectsBeforeVerify=0 bannerAfterVerify=0 firstSendAfterVerify=1 sendsBetweenKeyChangeAndVerify=0 reason="${OSL_CHAT_KEY_CHANGED_REFUSAL_REASON}"`);
  }, 30_000);
});
