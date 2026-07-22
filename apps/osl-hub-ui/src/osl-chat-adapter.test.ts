import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauriRuntime: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { activateOslChatContext, closeOslChatContext, listOslChatHistory, openOslChatText, prepareOslChatText } from "./adapters";

describe("first-party OSL Chat IPC", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("accepts only the fixed first-party context", async () => {
    mocks.invoke.mockResolvedValueOnce({ contextToken: "context-token-1234567890", serviceId: "osl-chat", accountId: "osl-main", personId: "person-a", peerOslUserId: "peer-a", scopeApproved: true });
    await expect(activateOslChatContext("person-a")).resolves.toMatchObject({ serviceId: "osl-chat", accountId: "osl-main" });
    expect(mocks.invoke).toHaveBeenCalledWith("activate_osl_chat_context", { personId: "person-a" });
  });

  it("prepares view-once text, parses authenticated inbox, and closes authority", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ messageId: "peer-0123456789abcdef0123456789abcdef", expiresAt: 2_000_000_000, personToPersonE2ee: true, viewOnce: true, deliveredToOslInbox: true })
      .mockResolvedValueOnce({ messages: [{ plaintext: "hello", contextVerified: true, personToPersonE2ee: true, viewOnceConsumed: true, expiresAt: 2_000_000_000 }], pendingViewOnce: [], acknowledgments: [], fetched: 1 })
      .mockResolvedValueOnce(undefined);
    await expect(prepareOslChatText("hello", true)).resolves.toMatchObject({ viewOnce: true });
    await expect(openOslChatText(false)).resolves.toMatchObject({ fetched: 1 });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "open_osl_chat_text", { revealViewOnce: false });
    await expect(closeOslChatContext()).resolves.toBe(true);
  });

  it("rejects malformed plaintext and strictly parses encrypted-at-rest history", async () => {
    await expect(prepareOslChatText("x".repeat(1_001))).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
    mocks.invoke.mockResolvedValueOnce([{ discord_message_id: "peer-0123456789abcdef0123456789abcdef", channel_id: "manual-dm-0123456789abcdef", sender_discord_id: "osl-peer", sender_osl_user_id: "osl-peer", plaintext: "line one\nline two", decrypted_at: 1_900_000_000, burned: false }]);
    await expect(listOslChatHistory()).resolves.toEqual([{ messageId: "peer-0123456789abcdef0123456789abcdef", senderOslUserId: "osl-peer", plaintext: "line one\nline two", decryptedAt: 1_900_000_000 }]);
  });
});
