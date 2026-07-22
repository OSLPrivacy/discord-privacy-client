import type {
  BoundedHistoryRequest,
  HostedDeleteResult,
  HostedListResult,
  HostedOwnItem,
  HostedScrollResult,
  HostedSessionDeleteOnlyPort,
  HostedSessionFriction,
  HostedSessionResult,
  HostedVerifyResult,
} from "./scrub-hosted-session-port";

export type HostedSessionCommand =
  | { readonly operation: "scrollHistory"; readonly bounded: BoundedHistoryRequest }
  | { readonly operation: "listOwnItems" }
  | { readonly operation: "deleteOwnItem"; readonly id: string }
  | { readonly operation: "verifyGone"; readonly id: string };

/** Thin native/preload integration seam. Its request type cannot express arbitrary automation. */
export interface HostedSessionCommandChannel {
  request(command: HostedSessionCommand): Promise<unknown>;
}

const friction = new Set<HostedSessionFriction>(["captcha", "challenge", "rate-limit", "schema-drift", "signed-out", "account-changed", "unknown"]);
const idPattern = /^[A-Za-z0-9][A-Za-z0-9._:@/<>-]{0,255}$/;

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("hosted port returned a malformed envelope");
  return value as Record<string, unknown>;
}

function base(value: unknown): HostedSessionResult {
  const item = record(value);
  if (typeof item.ok !== "boolean" || typeof item.accountId !== "string" || !idPattern.test(item.accountId) || typeof item.sessionEpoch !== "string" || !idPattern.test(item.sessionEpoch) || typeof item.schemaVersion !== "string" || !idPattern.test(item.schemaVersion)) throw new Error("hosted port identity envelope is malformed");
  if (item.friction !== undefined && (typeof item.friction !== "string" || !friction.has(item.friction as HostedSessionFriction))) throw new Error("hosted port friction is malformed");
  if (!item.ok && item.friction === undefined) throw new Error("hosted port failure omitted permanent friction");
  return { ok: item.ok, accountId: item.accountId, sessionEpoch: item.sessionEpoch, schemaVersion: item.schemaVersion, ...(item.friction ? { friction: item.friction as HostedSessionFriction } : {}) };
}

function ownItem(value: unknown): HostedOwnItem {
  const item = record(value);
  if (typeof item.id !== "string" || !idPattern.test(item.id) || typeof item.channelId !== "string" || !idPattern.test(item.channelId) || typeof item.correspondentId !== "string" || !idPattern.test(item.correspondentId) || !Number.isSafeInteger(item.createdAtUnixMs) || (item.createdAtUnixMs as number) < 0 || typeof item.contentFingerprint !== "string" || item.contentFingerprint.length < 1 || item.contentFingerprint.length > 512 || item.authoredBySelf !== true || typeof item.retractable !== "boolean") throw new Error("hosted port exposed a malformed or non-owned item");
  return item as unknown as HostedOwnItem;
}

export class CheckedHostedSessionPort implements HostedSessionDeleteOnlyPort {
  readonly #channel: HostedSessionCommandChannel;
  constructor(channel: HostedSessionCommandChannel) { this.#channel = channel; }

  async scrollHistory(bounded: BoundedHistoryRequest): Promise<HostedScrollResult> {
    const value = await this.#channel.request({ operation: "scrollHistory", bounded });
    const checked = base(value), item = record(value);
    if (typeof item.complete !== "boolean") throw new Error("hosted scroll result is malformed");
    return { ...checked, complete: item.complete };
  }

  async listOwnItems(): Promise<HostedListResult> {
    const value = await this.#channel.request({ operation: "listOwnItems" });
    const checked = base(value), item = record(value);
    if (!Array.isArray(item.items) || item.items.length > 500) throw new Error("hosted own-item list is malformed");
    return { ...checked, items: item.items.map(ownItem) };
  }

  async deleteOwnItem(id: string): Promise<HostedDeleteResult> {
    if (!idPattern.test(id)) throw new Error("invalid hosted item id");
    const value = await this.#channel.request({ operation: "deleteOwnItem", id });
    const checked = base(value), item = record(value);
    if (typeof item.accepted !== "boolean") throw new Error("hosted delete result is malformed");
    return { ...checked, accepted: item.accepted };
  }

  async verifyGone(id: string): Promise<HostedVerifyResult> {
    if (!idPattern.test(id)) throw new Error("invalid hosted item id");
    const value = await this.#channel.request({ operation: "verifyGone", id });
    const checked = base(value), item = record(value);
    if (typeof item.gone !== "boolean" || typeof item.covered !== "boolean" || (item.gone && !item.covered)) throw new Error("hosted readback result is malformed");
    return { ...checked, gone: item.gone, covered: item.covered };
  }
}
