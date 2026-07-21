import { describe, expect, it, vi } from "vitest";
import type { DeleteFinding } from "./scrub-delete-engine";
import { classifyImapFailure, ImapDeleteAdapter, type ImapMessage, type NarrowImapClient } from "./scrub-imap-adapter";
import { TelegramDeleteAdapter, telegramFloodWaitSeconds, type NarrowTdlibClient, type TelegramMessage } from "./scrub-telegram-adapter";

const finding: DeleteFinding = { providerId: "imap", accountId: "acct", channelId: "Sent", correspondentId: "p", itemId: "message@example", authoredBySelf: true, createdAtUnixMs: 1, contentFingerprint: "hash" };
function imapClient(): NarrowImapClient {
  let current: ImapMessage | null = { uid: 7, messageId: finding.itemId, mailbox: "Sent", authoredBySelf: true, contentFingerprint: "hash" };
  return { searchMessageId: vi.fn(async () => current === null ? [] : [7]), fetch: vi.fn(async () => current), moveToTrash: vi.fn(async () => { current = null; return true; }), markDeleted: vi.fn(async () => true), expunge: vi.fn(async () => undefined) };
}
describe("sanctioned delete adapters", () => {
  it("IMAP paces at a fixed honest interval, deletes, and verifies by Message-ID readback", async () => {
    const client = imapClient(), wait = vi.fn(async (_ms: number) => undefined);
    const adapter = new ImapDeleteAdapter({ accountId: "acct", authEpoch: "a1", findings: [finding], client, fixedDelayMs: 1_250, wait });
    expect(Object.getOwnPropertyNames(ImapDeleteAdapter.prototype).sort()).toEqual(["constructor", "delete", "enumerate", "inspect", "verify"]);
    expect(await adapter.inspect(finding)).toMatchObject({ state: "present", authoredBySelf: true });
    expect(await adapter.delete(finding)).toMatchObject({ accepted: true });
    expect(await adapter.verify(finding)).toMatchObject({ outcome: "confirmed-deleted" });
    expect(wait.mock.calls.every(([ms]) => ms === 1_250)).toBe(true);
  });
  it("IMAP uses standards-based deleted+expunge fallback and classifies rate limits honestly", async () => {
    const client = imapClient(); vi.mocked(client.moveToTrash).mockResolvedValue(false);
    const adapter = new ImapDeleteAdapter({ accountId: "acct", authEpoch: "a1", findings: [finding], client, wait: async () => undefined });
    expect(await adapter.delete(finding)).toMatchObject({ accepted: true });
    expect(client.markDeleted).toHaveBeenCalledWith("Sent", 7);
    expect(client.expunge).toHaveBeenCalledWith("Sent", 7);
    expect(classifyImapFailure(new Error("RATE LIMIT try again"))).toContain("rate limit");
  });
  it("treats duplicate IMAP Message-IDs as ambiguous, never absent", async () => {
    const client = imapClient(); vi.mocked(client.searchMessageId).mockResolvedValue([7, 8]);
    const adapter = new ImapDeleteAdapter({ accountId: "acct", authEpoch: "a1", findings: [finding], client, wait: async () => undefined });
    await expect(adapter.inspect(finding)).rejects.toThrow("ambiguous");
    expect(await adapter.verify(finding)).toMatchObject({ outcome: "UNKNOWN" });
  });
  it("Telegram uses documented deleteMessages(revoke=true), readback, and exact FLOOD_WAIT", async () => {
    let message: TelegramMessage | null = { id: "42", chatId: "chat", authoredBySelf: true, contentFingerprint: "h", canBeDeletedForAllUsers: true };
    const client: NarrowTdlibClient = { getChatHistory: vi.fn(async () => message === null ? [] : [message]), getMessage: vi.fn(async () => message), deleteMessages: vi.fn(async (_chat, _ids, revoke) => { expect(revoke).toBe(true); message = null; }) };
    const adapter = new TelegramDeleteAdapter(client, "acct", "a1");
    const item: DeleteFinding = { ...finding, providerId: "telegram", channelId: "chat", itemId: "42", contentFingerprint: "h" };
    expect(Object.getOwnPropertyNames(TelegramDeleteAdapter.prototype).sort()).toEqual(["constructor", "delete", "enumerate", "inspect", "verify"]);
    expect(await adapter.delete(item)).toMatchObject({ accepted: true });
    expect(await adapter.verify(item)).toMatchObject({ outcome: "confirmed-deleted" });
    expect(telegramFloodWaitSeconds(new Error("FLOOD_WAIT_37"))).toBe(37);
    expect(telegramFloodWaitSeconds(new Error("unknown"))).toBeNull();
  });
  it("surfaces Telegram's non-retractable item location without forcing past the window", async () => {
    const message: TelegramMessage = { id: "42", chatId: "chat", authoredBySelf: true, contentFingerprint: "h", canBeDeletedForAllUsers: false };
    const client: NarrowTdlibClient = { getChatHistory: vi.fn(async () => [message]), getMessage: vi.fn(async () => message), deleteMessages: vi.fn(async () => undefined) };
    const inspected = await new TelegramDeleteAdapter(client, "acct", "a1").inspect({ ...finding, providerId: "telegram", channelId: "chat", itemId: "42", contentFingerprint: "h" });
    expect(inspected).toMatchObject({ retractable: false, detail: expect.stringContaining("remains located") });
    expect(client.deleteMessages).not.toHaveBeenCalled();
  });
});
