import type { DeleteFinding, DeleteInspection, DeleteRequestResult, DeleteVerification, ScrubDeleteAdapter } from "./scrub-delete-engine";

export const OSL_TELEGRAM_API_ID_PLACEHOLDER = "OSL_OWNED_TELEGRAM_API_ID_REQUIRED";
export interface TelegramMessage { id: string; chatId: string; authoredBySelf: boolean; contentFingerprint: string; canBeDeletedForAllUsers: boolean }
export interface NarrowTdlibClient { getChatHistory(chatId: string, beforeUnixMs: number): Promise<readonly TelegramMessage[]>; getMessage(chatId: string, messageId: string): Promise<TelegramMessage | null>; deleteMessages(chatId: string, messageIds: readonly string[], revoke: true): Promise<void> }

export class TelegramDeleteAdapter implements ScrubDeleteAdapter {
  readonly #client: NarrowTdlibClient; readonly #accountId: string; readonly #authEpoch: string;
  constructor(client: NarrowTdlibClient, accountId: string, authEpoch: string) { this.#client = client; this.#accountId = accountId; this.#authEpoch = authEpoch; }
  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#accountId) return [];
    const batches = await Promise.all(scope.channelIds.map((id) => this.#client.getChatHistory(id, scope.beforeUnixMs)));
    return batches.flat().map((m) => ({ providerId: "telegram", accountId: this.#accountId, channelId: m.chatId, correspondentId: m.chatId, itemId: m.id, authoredBySelf: m.authoredBySelf, createdAtUnixMs: 0, contentFingerprint: m.contentFingerprint }));
  }
  async inspect(f: DeleteFinding): Promise<DeleteInspection> {
    const m = await this.#client.getMessage(f.channelId, f.itemId);
    return m === null ? { state: "absent", authoredBySelf: true, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: true }
      : { state: "present", authoredBySelf: m.authoredBySelf, contentFingerprint: m.contentFingerprint, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: m.canBeDeletedForAllUsers, detail: m.canBeDeletedForAllUsers ? undefined : "cannot be recalled; item remains located in this chat" };
  }
  async delete(f: DeleteFinding): Promise<DeleteRequestResult> {
    try { await this.#client.deleteMessages(f.channelId, [f.itemId], true); return { accepted: true, authEpoch: this.#authEpoch }; }
    catch (error) { const wait = telegramFloodWaitSeconds(error); if (wait === null) throw error; return { accepted: false, authEpoch: this.#authEpoch, detail: `FLOOD_WAIT_${wait}: stop for exactly the provider-required interval` }; }
  }
  async verify(f: DeleteFinding): Promise<DeleteVerification> {
    try { return { outcome: await this.#client.getMessage(f.channelId, f.itemId) === null ? "confirmed-deleted" : "confirmed-not-deleted", authEpoch: this.#authEpoch, detail: "TDLib getMessage readback" }; }
    catch { return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: "TDLib readback failed ambiguously" }; }
  }
}
export function telegramFloodWaitSeconds(error: unknown): number | null {
  const match = (error instanceof Error ? error.message : String(error)).match(/(?:FLOOD_WAIT_|retry after\s+)(\d+)/i);
  if (match === null) return null;
  const seconds = Number(match[1]);
  return Number.isSafeInteger(seconds) && seconds > 0 && seconds <= 86_400 ? seconds : null;
}
