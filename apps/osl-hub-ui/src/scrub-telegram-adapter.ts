import type { DeleteFinding, DeleteInspection, DeleteRequestResult, DeleteVerification, ScrubDeleteAdapter } from "./scrub-delete-engine";

export const OSL_TELEGRAM_API_ID_PLACEHOLDER = "OSL_OWNED_TELEGRAM_API_ID_REQUIRED";
export interface TelegramMessage { id: string; chatId: string; authoredBySelf: boolean; createdAtUnixMs: number; contentFingerprint: string; canBeDeletedForAllUsers: boolean }
export interface NarrowTdlibClient { getChatHistory(chatId: string, beforeUnixMs: number): Promise<readonly TelegramMessage[]>; getMessage(chatId: string, messageId: string): Promise<TelegramMessage | null>; deleteMessages(chatId: string, messageIds: readonly string[], revoke: true): Promise<void> }

export class TelegramDeleteAdapter implements ScrubDeleteAdapter {
  readonly #client: NarrowTdlibClient; readonly #accountId: string; readonly #authEpoch: string; readonly #clock: () => number; #retryNotBeforeUnixMs = 0;
  constructor(client: NarrowTdlibClient, accountId: string, authEpoch: string, clock: () => number = Date.now) { this.#client = client; this.#accountId = accountId; this.#authEpoch = authEpoch; this.#clock = clock; }
  #remainingWaitSeconds(): number { return Math.max(0, Math.ceil((this.#retryNotBeforeUnixMs - this.#clock()) / 1_000)); }
  #parkFrom(error: unknown): number | null {
    const seconds = telegramFloodWaitSeconds(error);
    if (seconds !== null) this.#retryNotBeforeUnixMs = Math.max(this.#retryNotBeforeUnixMs, this.#clock() + seconds * 1_000);
    return seconds;
  }
  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#accountId || this.#remainingWaitSeconds() > 0) return [];
    const batches = [];
    for (const id of scope.channelIds) {
      try { batches.push(await this.#client.getChatHistory(id, scope.beforeUnixMs)); }
      catch (error) { if (this.#parkFrom(error) !== null) return []; throw error; }
    }
    return batches.flat().filter((m) => m.authoredBySelf && m.createdAtUnixMs < scope.beforeUnixMs).map((m) => ({ providerId: "telegram", accountId: this.#accountId, channelId: m.chatId, correspondentId: m.chatId, itemId: m.id, authoredBySelf: true, createdAtUnixMs: m.createdAtUnixMs, contentFingerprint: m.contentFingerprint }));
  }
  async inspect(f: DeleteFinding): Promise<DeleteInspection> {
    if (f.providerId !== "telegram" || f.accountId !== this.#accountId) return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: false, detail: "finding is outside the fixed TDLib account" };
    const remaining = this.#remainingWaitSeconds();
    if (remaining > 0) return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: false, detail: `FLOOD_WAIT active for ${remaining} more seconds` };
    let m: TelegramMessage | null;
    try { m = await this.#client.getMessage(f.channelId, f.itemId); }
    catch (error) {
      const wait = this.#parkFrom(error);
      if (wait !== null) return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: false, detail: `FLOOD_WAIT_${wait}: parked` };
      throw error;
    }
    return m === null ? { state: "absent", authoredBySelf: true, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: true }
      : { state: "present", authoredBySelf: m.authoredBySelf, contentFingerprint: m.contentFingerprint, authEpoch: this.#authEpoch, schemaVersion: "tdlib-v1", retractable: m.canBeDeletedForAllUsers, detail: m.canBeDeletedForAllUsers ? undefined : "cannot be recalled; item remains located in this chat" };
  }
  async delete(f: DeleteFinding): Promise<DeleteRequestResult> {
    if (f.providerId !== "telegram" || f.accountId !== this.#accountId || !f.authoredBySelf) return { accepted: false, authEpoch: this.#authEpoch, detail: "item is outside the fixed account or is not owned" };
    const remaining = this.#remainingWaitSeconds();
    if (remaining > 0) return { accepted: false, authEpoch: this.#authEpoch, detail: `FLOOD_WAIT active for ${remaining} more seconds` };
    try {
      const current = await this.#client.getMessage(f.channelId, f.itemId);
      if (current === null) return { accepted: false, authEpoch: this.#authEpoch, detail: "message is already absent" };
      if (current.id !== f.itemId || current.chatId !== f.channelId || !current.authoredBySelf || current.contentFingerprint !== f.contentFingerprint || !current.canBeDeletedForAllUsers) return { accepted: false, authEpoch: this.#authEpoch, detail: "ownership, content, identity, or retractability guard rejected deletion" };
      await this.#client.deleteMessages(f.channelId, [f.itemId], true);
      return { accepted: true, authEpoch: this.#authEpoch };
    } catch (error) {
      const wait = this.#parkFrom(error);
      if (wait === null) throw error;
      return { accepted: false, authEpoch: this.#authEpoch, detail: `FLOOD_WAIT_${wait}: stop for exactly the provider-required interval` };
    }
  }
  async verify(f: DeleteFinding): Promise<DeleteVerification> {
    if (f.providerId !== "telegram" || f.accountId !== this.#accountId) return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: "finding is outside the fixed TDLib account" };
    const remaining = this.#remainingWaitSeconds();
    if (remaining > 0) return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: `FLOOD_WAIT active for ${remaining} more seconds` };
    try { return { outcome: await this.#client.getMessage(f.channelId, f.itemId) === null ? "confirmed-deleted" : "confirmed-not-deleted", authEpoch: this.#authEpoch, detail: "TDLib getMessage readback" }; }
    catch (error) { const wait = this.#parkFrom(error); return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: wait === null ? "TDLib readback failed ambiguously" : `FLOOD_WAIT_${wait}: parked` }; }
  }
}
export function telegramFloodWaitSeconds(error: unknown): number | null {
  const match = (error instanceof Error ? error.message : String(error)).match(/(?:FLOOD_WAIT_|retry after\s+)(\d+)/i);
  if (match === null) return null;
  const seconds = Number(match[1]);
  return Number.isSafeInteger(seconds) && seconds > 0 && seconds <= 86_400 ? seconds : null;
}
