import type {
  DeleteFinding,
  DeleteInspection,
  DeleteRequestResult,
  DeleteVerification,
  ScrubDeleteAdapter,
  StepUpProof,
} from "./scrub-delete-engine";
import type {
  BoundedHistoryRequest,
  HostedOwnItem,
  HostedPortBinding,
  HostedSessionDeleteOnlyPort,
  HostedSessionFriction,
  HostedSessionResult,
  HostedScrubProviderId,
} from "./scrub-hosted-session-port";
import type { AutoScrubCapability, AutoScrubProviderBridge, AutoScrubProviderId } from "./autoscrub-flow";

export const HOSTED_SESSION_FIXED_REST_MS = 1_500;
export const HOSTED_SESSION_PRESENCE_TTL_MS = 30_000;
export const HOSTED_SESSION_MAX_BATCH = 25;

export interface HostedSessionPacing {
  readonly fixedRestMs: number;
  readonly presenceTtlMs: number;
  readonly maxBatch: number;
  readonly wait: (milliseconds: number) => Promise<void>;
  readonly clock: () => number;
}

export class HostedSessionPresenceGate {
  readonly #pacing: HostedSessionPacing;
  #lastPresence = -1;
  #actions = 0;
  #parked = true;
  #tail: Promise<void> = Promise.resolve();

  constructor(pacing: HostedSessionPacing) { this.#pacing = pacing; }

  signalHumanPresence(): void {
    this.#lastPresence = this.#pacing.clock();
    this.#actions = 0;
    this.#parked = false;
  }

  park(): void { this.#parked = true; }

  async beforeAction(): Promise<"ready" | "parked"> {
    let release!: () => void;
    const prior = this.#tail;
    this.#tail = new Promise<void>((resolve) => { release = resolve; });
    await prior;
    try {
      const now = this.#pacing.clock();
      if (this.#parked || this.#lastPresence < 0 || now - this.#lastPresence > this.#pacing.presenceTtlMs || this.#actions >= this.#pacing.maxBatch) {
        this.#parked = true;
        return "parked";
      }
      await this.#pacing.wait(this.#pacing.fixedRestMs);
      this.#actions += 1;
      return "ready";
    } finally { release(); }
  }
}

const providerSchema: Record<HostedScrubProviderId, string> = {
  "gmail-web": "gmail-web-ui-v1",
  discord: "discord-web-ui-v1",
  "telegram-web": "telegram-web-ui-v1",
};

function hostedProvider(providerId: AutoScrubProviderId): providerId is HostedScrubProviderId {
  return providerId === "gmail-web" || providerId === "discord" || providerId === "telegram-web";
}

function sameSession(result: HostedSessionResult, providerId: HostedScrubProviderId, accountId: string, sessionEpoch: string): boolean {
  return result.ok && result.accountId === accountId && result.sessionEpoch === sessionEpoch && result.schemaVersion === providerSchema[providerId] && !result.friction;
}

export class HostedSessionAssistedDeleteAdapter implements ScrubDeleteAdapter {
  readonly #providerId: HostedScrubProviderId;
  readonly #accountId: string;
  readonly #sessionEpoch: string;
  readonly #port: HostedSessionDeleteOnlyPort;
  readonly #gate: HostedSessionPresenceGate;
  #friction: HostedSessionFriction | null = null;
  #listed = new Map<string, HostedOwnItem>();

  constructor(binding: HostedPortBinding, sessionEpoch: string, gate: HostedSessionPresenceGate) {
    this.#providerId = binding.providerId;
    this.#accountId = binding.accountId;
    this.#sessionEpoch = sessionEpoch;
    this.#port = binding.port;
    this.#gate = gate;
  }

  signalHumanPresence(): void { this.#gate.signalHumanPresence(); }
  park(): void { this.#gate.park(); }

  #stop(reason: HostedSessionFriction): void {
    this.#friction ??= reason;
    this.#gate.park();
  }

  #accept(result: HostedSessionResult): boolean {
    if (result.friction) this.#stop(result.friction);
    else if (!sameSession(result, this.#providerId, this.#accountId, this.#sessionEpoch)) this.#stop(result.accountId !== this.#accountId || result.sessionEpoch !== this.#sessionEpoch ? "account-changed" : "schema-drift");
    return this.#friction === null;
  }

  async #ready(): Promise<boolean> {
    return this.#friction === null && await this.#gate.beforeAction() === "ready";
  }

  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#accountId || this.#friction || !await this.#ready()) return [];
    const bounded: BoundedHistoryRequest = { maxScrolls: 4, maxItems: 500, beforeUnixMs: scope.beforeUnixMs };
    const scroll = await this.#port.scrollHistory(bounded).catch(() => null);
    if (!scroll) { this.#stop("unknown"); return []; }
    if (!this.#accept(scroll) || !await this.#ready()) return [];
    const listed = await this.#port.listOwnItems().catch(() => null);
    if (!listed) { this.#stop("unknown"); return []; }
    if (!this.#accept(listed)) return [];
    const channels = new Set(scope.channelIds);
    const findings: DeleteFinding[] = [];
    for (const item of listed.items) {
      if (!item.authoredBySelf || !channels.has(item.channelId) || item.createdAtUnixMs >= scope.beforeUnixMs) continue;
      this.#listed.set(item.id, item);
      findings.push({ providerId: this.#providerId, accountId: this.#accountId, channelId: item.channelId, correspondentId: item.correspondentId, itemId: item.id, authoredBySelf: true, createdAtUnixMs: item.createdAtUnixMs, contentFingerprint: item.contentFingerprint });
    }
    return findings;
  }

  async inspect(finding: DeleteFinding): Promise<DeleteInspection> {
    const base = { authEpoch: this.#sessionEpoch, schemaVersion: providerSchema[this.#providerId] };
    if (finding.providerId !== this.#providerId || finding.accountId !== this.#accountId || !finding.authoredBySelf || this.#friction || !await this.#ready()) return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false, detail: "hosted session is parked, stopped, or outside its fixed account" };
    const listed = await this.#port.listOwnItems().catch(() => null);
    if (!listed) { this.#stop("unknown"); return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false }; }
    if (!this.#accept(listed)) return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false, detail: `permanently stopped on ${this.#friction}` };
    const item = listed.items.find((candidate) => candidate.id === finding.itemId);
    if (item) {
      this.#listed.set(item.id, item);
      return { ...base, schemaVersion: listed.schemaVersion, state: "present", authoredBySelf: item.authoredBySelf, contentFingerprint: item.contentFingerprint, retractable: item.retractable, detail: item.retractable ? undefined : "surface-only: this UI cannot retract the remote copy" };
    }
    if (!await this.#ready()) return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false };
    const verified = await this.#port.verifyGone(finding.itemId).catch(() => null);
    if (!verified) { this.#stop("unknown"); return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false }; }
    if (!this.#accept(verified) || !verified.covered) return { ...base, state: "unknown", authoredBySelf: false, contentFingerprint: null, retractable: false, detail: verified.covered ? `permanently stopped on ${this.#friction}` : "item is outside current readback coverage" };
    return { ...base, schemaVersion: verified.schemaVersion, state: verified.gone ? "absent" : "unknown", authoredBySelf: true, contentFingerprint: null, retractable: true };
  }

  async delete(finding: DeleteFinding): Promise<DeleteRequestResult> {
    if (finding.providerId !== this.#providerId || finding.accountId !== this.#accountId || !finding.authoredBySelf || !this.#listed.has(finding.itemId) || this.#friction || !await this.#ready()) return { accepted: false, authEpoch: this.#sessionEpoch, detail: "delete refused: item is not a listed own item or the session is parked" };
    const result = await this.#port.deleteOwnItem(finding.itemId).catch(() => null);
    if (!result) { this.#stop("unknown"); return { accepted: false, authEpoch: this.#sessionEpoch, detail: "permanently stopped on unknown delete result" }; }
    if (!this.#accept(result)) return { accepted: false, authEpoch: this.#sessionEpoch, detail: `permanently stopped on ${this.#friction}` };
    return { accepted: result.accepted, authEpoch: this.#sessionEpoch, detail: "provider UI delete action returned; readback still required" };
  }

  async verify(finding: DeleteFinding): Promise<DeleteVerification> {
    if (finding.providerId !== this.#providerId || finding.accountId !== this.#accountId || this.#friction || !await this.#ready()) return { outcome: "UNKNOWN", authEpoch: this.#sessionEpoch, detail: "hosted session is parked or permanently stopped" };
    const result = await this.#port.verifyGone(finding.itemId).catch(() => null);
    if (!result) { this.#stop("unknown"); return { outcome: "UNKNOWN", authEpoch: this.#sessionEpoch, detail: "permanently stopped on unknown readback" }; }
    if (!this.#accept(result) || !result.covered) return { outcome: "UNKNOWN", authEpoch: this.#sessionEpoch, detail: result.covered ? `permanently stopped on ${this.#friction}` : "provider UI could not cover the requested readback" };
    return { outcome: result.gone ? "confirmed-deleted" : "confirmed-not-deleted", authEpoch: this.#sessionEpoch, detail: "verified by hosted provider UI readback; other copies may remain" };
  }
}

export interface HostedSessionBridgeOptions {
  readonly binding: HostedPortBinding;
  readonly gate: HostedSessionPresenceGate;
  readonly now?: () => number;
}

/** Creates a bridge only after a live, account-bound provider preload answers listOwnItems. */
export async function createLiveHostedSessionBridge(options: HostedSessionBridgeOptions): Promise<AutoScrubProviderBridge> {
  const probe = await options.binding.port.listOwnItems();
  if (!probe.ok || probe.friction || probe.accountId !== options.binding.accountId || probe.schemaVersion !== providerSchema[options.binding.providerId] || !probe.sessionEpoch) throw new Error("hosted session path did not live-confirm the fixed account and schema");
  const capability: AutoScrubCapability = { providerId: options.binding.providerId, label: options.binding.providerId === "gmail-web" ? "Gmail (signed-in session)" : options.binding.providerId === "discord" ? "Discord (signed-in session)" : "Telegram Web (signed-in session)", liveConfirmed: true, coverage: "Currently loaded own items with provider UI readback", pathKind: "hosted-session", primary: true };
  return {
    async capabilities() { return [capability]; },
    async adapter(providerId, accountId, _findings, proof) {
      if (!hostedProvider(providerId) || providerId !== options.binding.providerId || accountId !== options.binding.accountId || proof.authEpoch !== probe.sessionEpoch) throw new Error("hosted adapter identity changed");
      return new HostedSessionAssistedDeleteAdapter(options.binding, probe.sessionEpoch, options.gate);
    },
    async stepUp(providerId, accountId): Promise<StepUpProof> {
      if (!hostedProvider(providerId) || providerId !== options.binding.providerId || accountId !== options.binding.accountId) throw new Error("hosted session account changed");
      const fresh = await options.binding.port.listOwnItems();
      if (!sameSession(fresh, options.binding.providerId, options.binding.accountId, probe.sessionEpoch)) throw new Error("hosted session is no longer live-confirmed");
      options.gate.signalHumanPresence();
      const now = options.now?.() ?? Date.now();
      return { providerId, accountId, authEpoch: probe.sessionEpoch, authenticatedAt: now, expiresAt: now + HOSTED_SESSION_PRESENCE_TTL_MS };
    },
  };
}

