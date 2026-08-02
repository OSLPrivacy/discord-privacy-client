/**
 * T14-B2 — OSL Chat delivery must not be gated on the user standing on Home.
 *
 * These are behaviour tests: they drive the real `main.ts` route state, the real
 * delivery runtime and the real unread/timeline bookkeeping, and fake only the
 * IPC boundary. Nothing here reads source text.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ManualPeerContext } from "./adapters";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  setScreenshotProtection: vi.fn(),
  activateOslChatContext: vi.fn(),
  closeOslChatContext: vi.fn(),
  openOslChatText: vi.fn(),
  listOslChatHistory: vi.fn(),
  listOslChatAttachments: vi.fn(),
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
  setScreenshotProtection: mocks.setScreenshotProtection,
  activateOslChatContext: mocks.activateOslChatContext,
  closeOslChatContext: mocks.closeOslChatContext,
  openOslChatText: mocks.openOslChatText,
  listOslChatHistory: mocks.listOslChatHistory,
  listOslChatAttachments: mocks.listOslChatAttachments,
}));

function peerContext(personId: string): ManualPeerContext {
  return {
    contextToken: `ctx-${personId}`,
    serviceId: "osl-chat",
    accountId: "osl-main",
    personId,
    peerOslUserId: `OSLUSER-${personId}`,
    scopeApproved: true,
  } as ManualPeerContext;
}

function batchWith(bodies: string[]): NativeDiscordOverlayOpenedBatch {
  return {
    messages: bodies.map((plaintext, index) => ({
      messageId: `peer-${index.toString(16).padStart(32, "0")}`,
      plaintext,
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      expiresAt: 4_000_000_000,
    })),
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: bodies.length,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 0,
  } as NativeDiscordOverlayOpenedBatch;
}

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
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
  return import("./main");
}

function verifiedFriends(count: number): Array<{ personId: string; oslUserId: string; safetyNumberVerified: true; pendingKeyChange: false }> {
  return Array.from({ length: count }, (_unused, index) => ({
    personId: `p${index + 1}`,
    oslUserId: `OSLUSER-p${index + 1}`,
    safetyNumberVerified: true as const,
    pendingKeyChange: false as const,
  }));
}

describe("OSL Chat delivery is not gated on the Home screen", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.setScreenshotProtection.mockResolvedValue(true);
    mocks.closeOslChatContext.mockResolvedValue(true);
    mocks.listOslChatHistory.mockResolvedValue(null);
    mocks.listOslChatAttachments.mockResolvedValue([]);
    mocks.activateOslChatContext.mockImplementation(async (personId: string) => peerContext(personId));
    mocks.openOslChatText.mockResolvedValue(batchWith([]));
  });

  // The defect this task exists to kill: the drain used to return early unless
  // `route === "home"`, so a reply never appeared unless the recipient happened
  // to be sitting on the Home screen.
  it.each(["settings", "people", "privacy", "connections", "activity"] as const)(
    "delivers a message while the user is on %s, not home",
    async (route) => {
      const { __oslHubUiTest } = await loadUi();
      __oslHubUiTest.reset({ route, coreReady: true, hubPeople: verifiedFriends(1) });
      mocks.openOslChatText.mockResolvedValue(batchWith(["reply from a verified friend"]));

      await __oslHubUiTest.deliverOslChats();

      expect(__oslHubUiTest.snapshot().route).toBe(route);
      expect(__oslHubUiTest.oslChatConversation("p1").map((message) => message.body))
        .toContain("reply from a verified friend");
      expect(__oslHubUiTest.oslChatUnreadCount("p1")).toBe(1);
    },
  );

  it("keeps delivering to an unopened friend while the user reads a different screen", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: true, hubPeople: verifiedFriends(1) });

    mocks.openOslChatText.mockResolvedValue(batchWith(["first"]));
    await __oslHubUiTest.deliverOslChats();
    mocks.openOslChatText.mockResolvedValue(batchWith(["second"]));
    await __oslHubUiTest.deliverOslChats();

    expect(__oslHubUiTest.oslChatConversation("p1").map((message) => message.body)).toEqual(["first", "second"]);
    expect(__oslHubUiTest.oslChatUnreadCount("p1")).toBe(2);
  });

  it("re-drains the conversation the user has open instead of sitting idle", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "home", coreReady: true, hubPeople: verifiedFriends(1) });
    mocks.openOslChatText.mockResolvedValue(batchWith([]));
    await __oslHubUiTest.openOslChatConversation("p1");
    expect(__oslHubUiTest.snapshot().route).toBe("osl-chat");
    expect(__oslHubUiTest.oslChatConversation("p1")).toHaveLength(0);

    // A message arrives with the chat already open and nobody touching Refresh.
    mocks.openOslChatText.mockResolvedValue(batchWith(["arrived while the chat was open"]));
    await __oslHubUiTest.deliverOslChats();

    expect(__oslHubUiTest.oslChatConversation("p1").map((message) => message.body))
      .toEqual(["arrived while the chat was open"]);
    // The user is looking at it, so it is not unread and raises no alert.
    expect(__oslHubUiTest.oslChatUnreadCount("p1")).toBe(0);
    // The open conversation is drained through its own live context; the runtime
    // must not activate anybody else's and tear it down.
    expect(mocks.closeOslChatContext).not.toHaveBeenCalled();
  });

  it("reaches friend 33 — the roster is a rotating batch, not a silent cap", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: true, hubPeople: verifiedFriends(33) });
    mocks.openOslChatText.mockImplementation(async () => {
      const personId = mocks.activateOslChatContext.mock.lastCall?.[0] as string;
      return batchWith([`message for ${personId}`]);
    });

    await __oslHubUiTest.deliverOslChats();
    const firstTick = mocks.activateOslChatContext.mock.calls.map((call) => call[0] as string);
    expect(firstTick).toHaveLength(32);
    expect(firstTick).not.toContain("p33");
    expect(__oslHubUiTest.oslChatConversation("p33")).toHaveLength(0);

    await __oslHubUiTest.deliverOslChats();

    expect(__oslHubUiTest.oslChatConversation("p33").map((message) => message.body))
      .toContain("message for p33");
    for (const person of verifiedFriends(33)) {
      expect(
        __oslHubUiTest.oslChatConversation(person.personId).length,
        `${person.personId} should have received within two ticks`,
      ).toBeGreaterThan(0);
    }
  });

  it("still yields to a foreign protected context rather than clobbering it", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: true, hubPeople: verifiedFriends(1) });
    mocks.openOslChatText.mockResolvedValue(batchWith(["should not arrive"]));

    __oslHubUiTest.setForeignProtectedContextForTest("ctx-discord-overlay");
    await __oslHubUiTest.deliverOslChats();
    expect(mocks.activateOslChatContext).not.toHaveBeenCalled();
    expect(__oslHubUiTest.oslChatConversation("p1")).toHaveLength(0);

    __oslHubUiTest.setForeignProtectedContextForTest(null);
    await __oslHubUiTest.deliverOslChats();
    expect(__oslHubUiTest.oslChatConversation("p1").map((message) => message.body)).toEqual(["should not arrive"]);
  });

  // Retargeted from osl-chats-integration.test.ts, which used to assert the
  // literal `if (!context.scopeApproved) continue` against main.ts source.
  it("drains only friends whose chat scope is approved", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: true, hubPeople: verifiedFriends(2) });
    mocks.activateOslChatContext.mockImplementation(async (personId: string) => ({
      ...peerContext(personId),
      scopeApproved: personId === "p2",
    }));
    mocks.openOslChatText.mockImplementation(async () => {
      const personId = mocks.activateOslChatContext.mock.lastCall?.[0] as string;
      return batchWith([`message for ${personId}`]);
    });

    await __oslHubUiTest.deliverOslChats();

    expect(mocks.openOslChatText).toHaveBeenCalledTimes(1);
    expect(__oslHubUiTest.oslChatConversation("p1")).toHaveLength(0);
    expect(__oslHubUiTest.oslChatConversation("p2").map((message) => message.body)).toEqual(["message for p2"]);
  });

  it("never drains an unverified friend or one with a pending key change", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      route: "settings",
      coreReady: true,
      hubPeople: [
        { personId: "unverified", oslUserId: "OSLUSER-unverified", safetyNumberVerified: false, pendingKeyChange: false },
        { personId: "rekeyed", oslUserId: "OSLUSER-rekeyed", safetyNumberVerified: true, pendingKeyChange: true },
        { personId: "p1", oslUserId: "OSLUSER-p1", safetyNumberVerified: true, pendingKeyChange: false },
      ],
    });
    mocks.openOslChatText.mockResolvedValue(batchWith(["only for the verified friend"]));

    await __oslHubUiTest.deliverOslChats();

    expect(mocks.activateOslChatContext.mock.calls.map((call) => call[0] as string)).toEqual(["p1"]);
    expect(__oslHubUiTest.oslChatConversation("unverified")).toHaveLength(0);
    expect(__oslHubUiTest.oslChatConversation("rekeyed")).toHaveLength(0);
  });

  it("does not deliver before the identity is loaded", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: false, hubPeople: verifiedFriends(1) });
    mocks.openOslChatText.mockResolvedValue(batchWith(["too early"]));

    await __oslHubUiTest.deliverOslChats();

    expect(mocks.activateOslChatContext).not.toHaveBeenCalled();
  });

  it("applies capture resistance before any plaintext is drained", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "settings", coreReady: true, hubPeople: verifiedFriends(1) });
    mocks.setScreenshotProtection.mockResolvedValue(false);
    mocks.openOslChatText.mockResolvedValue(batchWith(["must not be drained unprotected"]));

    await __oslHubUiTest.deliverOslChats();

    expect(mocks.openOslChatText).not.toHaveBeenCalled();
    expect(__oslHubUiTest.oslChatConversation("p1")).toHaveLength(0);
  });
});
