import { readFileSync } from "node:fs";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { advisoryForSenderDraft, senderDraftAdvisoryMarkup } from "./sender-draft-advisory";
import { englishCatalogue } from "./catalogue/en";

const SHIPPING_ADVISORY_SENTENCE = "Advisory: this is your device checking your own draft. A modified client would not run this check, and nothing prevents the message from arriving.";
const FORBIDDEN_PROMISE_WORDS = ["block", "remove", "owner enforcement", "enforcement"] as const;

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  window: {
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
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("document", {
    querySelector: () => null,
    querySelectorAll: () => [],
    createElement: () => ({ innerHTML: "", appendChild: () => undefined, classList: { add: () => undefined }, remove: () => undefined }),
    body: { append: () => undefined },
  });
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, setTimeout: () => 0, clearTimeout: () => undefined });
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => vi.unstubAllGlobals());

beforeEach(() => {
  localStore.clear();
  mocks.invoke.mockReset();
  mocks.emitTo.mockReset();
  ui.__oslHubUiTest.reset({
    route: "osl-chat",
    coreReady: true,
    hubPeople: [{ personId: "sender-6964", alias: "Sender", safetyNumberVerified: true }, { personId: "receiver-6964", alias: "Receiver", safetyNumberVerified: true }],
  });
  ui.__oslHubUiTest.setOslChatForTest("receiver-6964", "exact original draft: porn stays verbatim");
  ui.__oslHubUiTest.setSenderFilterSetForTest("basic");
});

describe("TASK 6964 sender draft advisory", () => {
  it("names the signed rule and carries the shipping advisory without promise words", () => {
    const advisory = advisoryForSenderDraft("exact original draft: porn stays verbatim", "basic");
    expect(advisory).toEqual({ draft: "exact original draft: porn stays verbatim", matchedRule: "sexual-porn" });
    const markup = senderDraftAdvisoryMarkup(advisory);
    expect(markup).toContain("sexual-porn");
    expect(englishCatalogue.senderDraftFilterAdvisory, "TASK6964_ADVISORY sentence").toBe(SHIPPING_ADVISORY_SENTENCE);
    expect(markup).toContain(SHIPPING_ADVISORY_SENTENCE);
    expect(markup).toContain("Send anyway");
    expect(markup).toContain("Edit draft");
    for (const word of FORBIDDEN_PROMISE_WORDS) {
      expect(markup, `TASK6964_PROMISE_WORD word=${word}`).not.toMatch(new RegExp(`\\b${word.replace(" ", "[ -]?")}\\b`, "iu"));
    }
  });

  it("keeps a matching draft local until Send anyway, then sends the exact original to the second identity", async () => {
    await ui.__oslHubUiTest.sendSenderDraftAdvisoryForTest();
    const warning = ui.__oslHubUiTest.renderOslChatForTest();
    expect(warning).toContain('id="sender-draft-advisory"');
    expect(warning).toContain("sexual-porn");
    expect(warning).toContain("exact original draft: porn stays verbatim");
    expect(mocks.invoke, "TASK6964_RELAY_REPORT surface=relay").not.toHaveBeenCalled();
    expect(mocks.emitTo, "TASK6964_OWNER_NOTIFICATION surface=enclave-owner").not.toHaveBeenCalled();

    mocks.invoke.mockResolvedValueOnce({
      messageId: "peer-6964-0123456789abcdef0123456789abcdef",
      expiresAt: 2_000_000_000,
      personToPersonE2ee: true,
      viewOnce: false,
      deliveredToOslInbox: true,
    });
    await ui.__oslHubUiTest.sendSenderDraftAnywayForTest();
    expect(mocks.invoke).toHaveBeenCalledWith("prepare_osl_chat_text", {
      plaintext: "exact original draft: porn stays verbatim",
      viewOnce: false,
    });
    expect(ui.__oslHubUiTest.oslChatConversation("receiver-6964").at(-1)).toMatchObject({
      direction: "outgoing",
      body: "exact original draft: porn stays verbatim",
      state: "sent",
    });
  });

  it("returns Edit draft intact, and off leaves delivery unchanged", async () => {
    await ui.__oslHubUiTest.sendSenderDraftAdvisoryForTest();
    ui.__oslHubUiTest.editSenderDraftForTest();
    expect(ui.__oslHubUiTest.renderOslChatForTest()).not.toContain('id="sender-draft-advisory"');
    expect(ui.__oslHubUiTest.renderOslChatForTest()).toContain("exact original draft: porn stays verbatim");
    ui.__oslHubUiTest.setSenderFilterSetForTest("off");
    expect(ui.__oslHubUiTest.renderOslChatForTest()).toContain("exact original draft: porn stays verbatim");
    expect(advisoryForSenderDraft("exact original draft: porn stays verbatim", "off")).toBeNull();

    mocks.invoke.mockResolvedValueOnce({
      messageId: "peer-6964-off-0123456789abcdef0123456789",
      expiresAt: 2_000_000_000,
      personToPersonE2ee: true,
      viewOnce: false,
      deliveredToOslInbox: true,
    });
    await ui.__oslHubUiTest.sendSenderDraftAdvisoryForTest();
    expect(mocks.invoke).toHaveBeenCalledWith("prepare_osl_chat_text", {
      plaintext: "exact original draft: porn stays verbatim",
      viewOnce: false,
    });
  });

  it("keeps the warning outside every relay and other-member surface", () => {
    const source = readFileSync(new URL("./sender-draft-advisory.ts", import.meta.url), "utf8");
    const executable = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/gu, "");
    expect(executable).not.toMatch(/\binvoke\b|fetch\(|WebSocket|emitTo/iu);
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(mocks.emitTo).not.toHaveBeenCalled();
    console.log("TASK6964_GREEN matched_rule=sexual-porn advisory=1 send_anyway=1 exact_original_delivery=1 second_identity=receiver-6964 edit_draft=1 filter_off_warning=0 observer_invoke=0 observer_emit=0 promise_words=0");
  });
});
