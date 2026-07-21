import type { DeleteFinding, DeleteInspection, DeleteRequestResult, DeleteVerification, ScrubDeleteAdapter } from "./scrub-delete-engine";

export interface ImapMessage { uid: number; messageId: string; mailbox: string; authoredBySelf: boolean; contentFingerprint: string }
export interface NarrowImapClient {
  searchMessageId(mailbox: string, messageId: string): Promise<readonly number[]>;
  fetch(mailbox: string, uid: number): Promise<ImapMessage | null>;
  moveToTrash(mailbox: string, uid: number): Promise<boolean>;
  markDeleted(mailbox: string, uid: number): Promise<boolean>;
  expunge(mailbox: string, uid: number): Promise<void>;
}
export interface ImapAdapterOptions { accountId: string; authEpoch: string; findings: readonly DeleteFinding[]; client: NarrowImapClient; trashMailbox?: string; fixedDelayMs?: number; wait?: (ms: number) => Promise<void> }

/** Gmail, Yahoo, AOL, GMX, Mail.com, and iCloud use the same sanctioned IMAP semantics here. */
export class ImapDeleteAdapter implements ScrubDeleteAdapter {
  readonly #options: ImapAdapterOptions;
  readonly #byId: ReadonlyMap<string, DeleteFinding>;
  constructor(options: ImapAdapterOptions) { this.#options = options; this.#byId = new Map(options.findings.map((f) => [f.itemId, f])); }
  async #pace(): Promise<void> { await (this.#options.wait ?? (async (ms) => { await new Promise((resolve) => setTimeout(resolve, ms)); }))(this.#options.fixedDelayMs ?? 1_000); }
  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#options.accountId) return [];
    const channels = new Set(scope.channelIds);
    return [...this.#byId.values()].filter((f) => channels.has(f.channelId) && f.createdAtUnixMs < scope.beforeUnixMs);
  }
  async #locate(f: DeleteFinding): Promise<ImapMessage | null> {
    await this.#pace();
    const uids = await this.#options.client.searchMessageId(f.channelId, f.itemId);
    if (uids.length === 0) return null;
    if (uids.length !== 1) throw new Error("ambiguous IMAP Message-ID readback");
    return this.#options.client.fetch(f.channelId, uids[0]);
  }
  async inspect(f: DeleteFinding): Promise<DeleteInspection> {
    const message = await this.#locate(f);
    if (message === null) return { state: "absent", authoredBySelf: true, contentFingerprint: null, authEpoch: this.#options.authEpoch, schemaVersion: "imap-v1", retractable: true };
    return { state: "present", authoredBySelf: message.authoredBySelf, contentFingerprint: message.contentFingerprint, authEpoch: this.#options.authEpoch, schemaVersion: "imap-v1", retractable: true };
  }
  async delete(f: DeleteFinding): Promise<DeleteRequestResult> {
    const message = await this.#locate(f);
    if (message === null) return { accepted: false, authEpoch: this.#options.authEpoch, detail: "message was not uniquely located" };
    const moved = await this.#options.client.moveToTrash(f.channelId, message.uid);
    if (!moved) {
      const marked = await this.#options.client.markDeleted(f.channelId, message.uid);
      if (!marked) return { accepted: false, authEpoch: this.#options.authEpoch, detail: "server rejected delete and trash move" };
      await this.#options.client.expunge(f.channelId, message.uid);
    }
    return { accepted: true, authEpoch: this.#options.authEpoch };
  }
  async verify(f: DeleteFinding): Promise<DeleteVerification> {
    try { return { outcome: await this.#locate(f) === null ? "confirmed-deleted" : "confirmed-not-deleted", authEpoch: this.#options.authEpoch, detail: "IMAP Message-ID readback" }; }
    catch (error) { return { outcome: "UNKNOWN", authEpoch: this.#options.authEpoch, detail: classifyImapFailure(error) }; }
  }
}

export function classifyImapFailure(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/rate|limit|too many|try again/i.test(message)) return "provider rate limit; stop and resume only after server recovery";
  if (/auth|login|credential/i.test(message)) return "authentication changed or expired";
  return "ambiguous IMAP failure";
}
