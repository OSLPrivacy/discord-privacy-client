import type { DeleteFinding, DeleteInspection, DeleteRequestResult, DeleteVerification, ScrubDeleteAdapter } from "./scrub-delete-engine";

export const DISCORD_ASSISTED_DELETE_BAN_WARNING = "Discord may restrict or ban an account for automated deletion. OSL stops on every challenge, rate signal, unknown error, or changed interface. Use only while present, in small human-speed batches.";
export const DISCORD_ACCOUNT_DELETION_URL = "https://support.discord.com/hc/articles/212500837-How-do-I-permanently-delete-my-account";
export const DISCORD_DATA_REQUEST_URL = "https://support.discord.com/hc/articles/360004027692-Requesting-a-Copy-of-your-Data";

export type DiscordFriction = "captcha" | "challenge" | "rate-signal" | "dom-schema-drift" | "unknown";
export interface HostedDiscordOwnMessage { id: string; channelId: string; correspondentId: string; createdAtUnixMs: number; contentFingerprint: string; authoredBySelf: boolean; retractable: boolean }
export interface HostedDiscordDeleteOnlySession {
  loadNextOwnMessageHistory(channelId: string): Promise<{ messages: readonly HostedDiscordOwnMessage[]; complete: boolean; friction?: DiscordFriction }>;
  inspectOwnMessage(channelId: string, messageId: string): Promise<{ message: HostedDiscordOwnMessage | null; schemaVersion: string; friction?: DiscordFriction }>;
  deleteOwnMessage(channelId: string, messageId: string): Promise<{ accepted: boolean; friction?: DiscordFriction }>;
  verifyOwnMessageAbsent(channelId: string, messageId: string): Promise<{ absent: boolean; friction?: DiscordFriction }>;
}
export interface DiscordPacingOptions { fixedRestMs: number; maxBatch: number; presenceTtlMs: number; boundedAwayMs: number; wait: (ms: number) => Promise<void>; clock: () => number }

export class PresenceGatedPacer {
  readonly #options: DiscordPacingOptions; #lastPresence = -1; #batchStart = -1; #count = 0;
  constructor(options: DiscordPacingOptions) { this.#options = options; }
  signalHumanPresence(): void { this.#lastPresence = this.#options.clock(); this.#batchStart = this.#lastPresence; this.#count = 0; }
  async beforeAction(): Promise<"ready" | "parked"> {
    const now = this.#options.clock();
    if (this.#lastPresence < 0 || now - this.#lastPresence > this.#options.presenceTtlMs || this.#batchStart < 0 || now - this.#batchStart > this.#options.boundedAwayMs || this.#count >= this.#options.maxBatch) return "parked";
    await this.#options.wait(this.#options.fixedRestMs); this.#count += 1; return "ready";
  }
}

export class DiscordAssistedDeleteAdapter implements ScrubDeleteAdapter {
  readonly #session: HostedDiscordDeleteOnlySession; readonly #accountId: string; readonly #authEpoch: string; readonly #pacer: PresenceGatedPacer; #friction: DiscordFriction | null = null;
  constructor(session: HostedDiscordDeleteOnlySession, accountId: string, authEpoch: string, pacer: PresenceGatedPacer) { this.#session = session; this.#accountId = accountId; this.#authEpoch = authEpoch; this.#pacer = pacer; }
  #stop(friction?: DiscordFriction): boolean { if (friction !== undefined) this.#friction = friction; return this.#friction !== null; }
  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#accountId || this.#friction !== null) return [];
    const findings: DeleteFinding[] = [];
    for (const channelId of scope.channelIds) {
      let complete = false;
      while (!complete && findings.length < 500) {
        if (await this.#pacer.beforeAction() === "parked") return findings;
        let page;
        try { page = await this.#session.loadNextOwnMessageHistory(channelId); } catch { this.#stop("unknown"); return findings; }
        if (this.#stop(page.friction)) return findings;
        for (const m of page.messages) if (m.authoredBySelf && m.createdAtUnixMs < scope.beforeUnixMs) findings.push({ providerId: "discord", accountId: this.#accountId, channelId: m.channelId, correspondentId: m.correspondentId, itemId: m.id, authoredBySelf: true, createdAtUnixMs: m.createdAtUnixMs, contentFingerprint: m.contentFingerprint });
        complete = page.complete;
      }
    }
    return findings;
  }
  async inspect(f: DeleteFinding): Promise<DeleteInspection> {
    if (this.#friction !== null || await this.#pacer.beforeAction() === "parked") return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "discord-hosted-v1", retractable: false, detail: "parked pending genuine human presence or friction review" };
    try {
      const result = await this.#session.inspectOwnMessage(f.channelId, f.itemId);
      if (this.#stop(result.friction)) return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "discord-hosted-v1", retractable: false, detail: `stopped on ${this.#friction}` };
      return result.message === null ? { state: "absent", authoredBySelf: true, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: result.schemaVersion, retractable: true }
        : { state: "present", authoredBySelf: result.message.authoredBySelf, contentFingerprint: result.message.contentFingerprint, authEpoch: this.#authEpoch, schemaVersion: result.schemaVersion, retractable: result.message.retractable, detail: result.message.retractable ? undefined : "cannot be recalled; item remains located in this channel" };
    } catch { this.#stop("unknown"); return { state: "unknown", authoredBySelf: false, contentFingerprint: null, authEpoch: this.#authEpoch, schemaVersion: "discord-hosted-v1", retractable: false, detail: "stopped on unknown hosted-session error" }; }
  }
  async delete(f: DeleteFinding): Promise<DeleteRequestResult> {
    if (!f.authoredBySelf || this.#friction !== null || await this.#pacer.beforeAction() === "parked") return { accepted: false, authEpoch: this.#authEpoch, detail: "parked or item is not owned by this account" };
    try { const result = await this.#session.deleteOwnMessage(f.channelId, f.itemId); if (this.#stop(result.friction)) return { accepted: false, authEpoch: this.#authEpoch, detail: `stopped on ${this.#friction}` }; return { accepted: result.accepted, authEpoch: this.#authEpoch }; }
    catch { this.#stop("unknown"); return { accepted: false, authEpoch: this.#authEpoch, detail: "stopped on unknown hosted-session error" }; }
  }
  async verify(f: DeleteFinding): Promise<DeleteVerification> {
    if (this.#friction !== null || await this.#pacer.beforeAction() === "parked") return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: "parked pending genuine human presence or friction review" };
    try { const result = await this.#session.verifyOwnMessageAbsent(f.channelId, f.itemId); if (this.#stop(result.friction)) return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: `stopped on ${this.#friction}` }; return { outcome: result.absent ? "confirmed-deleted" : "confirmed-not-deleted", authEpoch: this.#authEpoch, detail: "hosted Discord readback" }; }
    catch { this.#stop("unknown"); return { outcome: "UNKNOWN", authEpoch: this.#authEpoch, detail: "stopped on unknown hosted-session error" }; }
  }
}
