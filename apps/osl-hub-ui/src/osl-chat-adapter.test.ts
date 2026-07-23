import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { activateOslChatContext, closeOslChatContext, listOslChatHistory, openOslChatText, prepareOslChatText } from "./adapters";

describe("first-party OSL Chat IPC", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("accepts only the fixed Rust-owned OSL chat scope", async () => {
    mocks.invoke.mockResolvedValueOnce({
      contextToken: "context-token-1234567890",
      serviceId: "osl-chat",
      accountId: "osl-main",
      personId: "person-a",
      peerOslUserId: "peer-a",
      scopeApproved: false,
    });
    await expect(activateOslChatContext("person-a")).resolves.toMatchObject({ serviceId: "osl-chat", accountId: "osl-main" });
    expect(mocks.invoke).toHaveBeenCalledWith("activate_osl_chat_context", { personId: "person-a" });

    mocks.invoke.mockResolvedValueOnce({
      contextToken: "context-token-1234567890",
      serviceId: "discord",
      accountId: "osl-main",
      personId: "person-a",
      peerOslUserId: "peer-a",
      scopeApproved: true,
    });
    await expect(activateOslChatContext("person-a")).resolves.toBeNull();
  });

  it("revokes the fixed chat lease without accepting renderer authority", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(closeOslChatContext()).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenCalledWith("close_osl_chat_context");
  });

  it("delivers bounded text with explicit view-once flags and strictly parses the authenticated inbox batch", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ messageId: "peer-0123456789abcdef0123456789abcdef", expiresAt: 2_000_000_000, personToPersonE2ee: true, viewOnce: false, deliveredToOslInbox: true })
      .mockResolvedValueOnce({ messages: [{ plaintext: "hello\nworld", contextVerified: true, personToPersonE2ee: true, viewOnceConsumed: false, expiresAt: 2_000_000_000 }], pendingViewOnce: [], acknowledgments: [], fetched: 1 });
    await expect(prepareOslChatText("hello\nworld")).resolves.toMatchObject({ deliveredToOslInbox: true });
    await expect(openOslChatText(false)).resolves.toMatchObject({ fetched: 1 });
    expect(mocks.invoke.mock.calls).toEqual([
      ["prepare_osl_chat_text", { plaintext: "hello\nworld", viewOnce: false }],
      ["open_osl_chat_text", { revealViewOnce: false }],
    ]);
  });

  it("rejects empty, oversized, or malformed data", async () => {
    await expect(prepareOslChatText("")).resolves.toBeNull();
    await expect(prepareOslChatText("x".repeat(1_001))).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
    mocks.invoke.mockResolvedValueOnce({ messages: [{ plaintext: "unsafe", contextVerified: false }], acknowledgments: [], fetched: 1 });
    await expect(openOslChatText()).resolves.toBeNull();
  });

  it("defaults to revealing view-once messages when no flag is supplied", async () => {
    mocks.invoke.mockResolvedValueOnce({ messages: [], pendingViewOnce: [], acknowledgments: [], fetched: 0 });
    await expect(openOslChatText()).resolves.toMatchObject({ fetched: 0 });
    expect(mocks.invoke).toHaveBeenCalledWith("open_osl_chat_text", { revealViewOnce: true });
  });

  it("strictly parses bounded encrypted-at-rest history rows", async () => {
    mocks.invoke.mockResolvedValueOnce([{
      discord_message_id: "peer-0123456789abcdef0123456789abcdef",
      channel_id: "manual-dm-0123456789abcdef",
      sender_discord_id: "osl-peer",
      sender_osl_user_id: "osl-peer",
      plaintext: "line one\nline two",
      decrypted_at: 1_900_000_000,
      burned: false,
    }]);
    await expect(listOslChatHistory()).resolves.toEqual([{
      messageId: "peer-0123456789abcdef0123456789abcdef",
      senderOslUserId: "osl-peer",
      plaintext: "line one\nline two",
      decryptedAt: 1_900_000_000,
    }]);
    expect(mocks.invoke).toHaveBeenCalledWith("list_osl_chat_history");
  });
});
