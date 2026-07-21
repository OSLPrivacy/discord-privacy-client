import type {
  BoundedHistoryRequest,
  HostedDeleteResult,
  HostedListResult,
  HostedOwnItem,
  HostedScrollResult,
  HostedSessionDeleteOnlyPort,
  HostedSessionFriction,
  HostedVerifyResult,
  HostedScrubProviderId,
} from "./scrub-hosted-session-port";

type ProviderSchema = {
  readonly providerId: HostedScrubProviderId;
  readonly version: string;
  readonly allowedHosts: readonly string[];
  readonly historyRoot: string;
  readonly item: string;
  readonly itemIdAttributes: readonly string[];
  readonly ownMarker: string;
  readonly timestampAttributes: readonly string[];
  readonly channelAttributes: readonly string[];
  readonly correspondentAttributes: readonly string[];
  readonly retractableMarker: string;
  readonly deleteWithinItem: string;
  readonly deleteToolbar?: string;
  readonly sentLocation?: RegExp;
};

export const GMAIL_WEB_PRELOAD_SCHEMA: ProviderSchema = Object.freeze({
  providerId: "gmail-web",
  version: "gmail-web-ui-v1",
  allowedHosts: ["mail.google.com"],
  historyRoot: "div[role='main']",
  item: "tr[role='main'][data-legacy-thread-id], tr[role='main'][data-thread-id], div[role='main'] div[data-message-id]",
  itemIdAttributes: ["data-message-id", "data-legacy-thread-id", "data-thread-id"],
  ownMarker: "[data-is-sent='true'], [data-authored-by-self='true']",
  timestampAttributes: ["data-timestamp", "data-internal-date"],
  channelAttributes: ["data-thread-perm-id", "data-thread-id"],
  correspondentAttributes: ["data-correspondent-id", "data-hovercard-id"],
  retractableMarker: "[data-non-retractable='true']",
  deleteWithinItem: "[aria-label='Delete'], [data-tooltip='Delete']",
  deleteToolbar: "div[role='main'] [aria-label='Delete'], div[role='main'] [data-tooltip='Delete']",
  sentLocation: /(?:#|\/)(?:sent|search\/label%3Asent)(?:\/|$)/i,
});

export const DISCORD_WEB_PRELOAD_SCHEMA: ProviderSchema = Object.freeze({
  providerId: "discord",
  version: "discord-web-ui-v1",
  allowedHosts: ["discord.com"],
  historyRoot: "main[class*='chatContent']",
  item: "[data-list-item-id^='chat-messages___']",
  itemIdAttributes: ["data-message-id", "id"],
  ownMarker: "[data-authored-by-self='true']",
  timestampAttributes: ["data-timestamp"],
  channelAttributes: ["data-channel-id"],
  correspondentAttributes: ["data-correspondent-id", "data-author-id"],
  retractableMarker: "[data-non-retractable='true']",
  deleteWithinItem: "[aria-label='Delete Message'], [data-action='delete-message']",
});

export const TELEGRAM_WEB_PRELOAD_SCHEMA: ProviderSchema = Object.freeze({
  providerId: "telegram-web",
  version: "telegram-web-ui-v1",
  allowedHosts: ["web.telegram.org"],
  historyRoot: "#MiddleColumn, .middle-column",
  item: "[data-message-id]",
  itemIdAttributes: ["data-message-id"],
  ownMarker: ".own, [data-is-outgoing='true']",
  timestampAttributes: ["data-timestamp"],
  channelAttributes: ["data-peer-id", "data-chat-id"],
  correspondentAttributes: ["data-peer-id", "data-author-id"],
  retractableMarker: "[data-non-retractable='true']",
  deleteWithinItem: "[aria-label='Delete'], [data-action='delete']",
});

const opaqueId = /^[A-Za-z0-9][A-Za-z0-9._:@/<>-]{0,255}$/;
const frictionSelectors: ReadonlyArray<readonly [HostedSessionFriction, string]> = [
  ["captcha", "iframe[src*='captcha'], [data-captcha], [class*='captcha']"],
  ["challenge", "[data-challenge], [class*='challenge'], form[action*='challenge']"],
  ["rate-limit", "[data-rate-limit], [class*='rateLimit'], [aria-label*='rate limit' i]"],
];

function firstAttribute(element: Element, names: readonly string[]): string | null {
  for (const name of names) {
    const value = element.getAttribute(name)?.trim();
    if (value) return value;
  }
  return null;
}

function fingerprint(parts: readonly string[]): string {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(parts.join("\u001f"))) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `ui-${hash.toString(16).padStart(8, "0")}`;
}

/** Provider-specific preload base. It is instantiated inside the service webview, never page script. */
class SemanticProviderPreload implements HostedSessionDeleteOnlyPort {
  readonly #document: Document;
  readonly #location: Location;
  readonly #schema: ProviderSchema;
  readonly #accountId: string;
  readonly #sessionEpoch: string;
  readonly #seen = new Map<string, HostedOwnItem>();
  readonly #deleteAttempts = new Set<string>();
  #stopped: HostedSessionFriction | null = null;

  constructor(document: Document, location: Location, schema: ProviderSchema, accountId: string, sessionEpoch: string) {
    if (!opaqueId.test(accountId) || !opaqueId.test(sessionEpoch)) throw new Error("invalid hosted session identity");
    this.#document = document;
    this.#location = location;
    this.#schema = schema;
    this.#accountId = accountId;
    this.#sessionEpoch = sessionEpoch;
  }

  #base(ok: boolean, friction?: HostedSessionFriction) {
    return { ok, accountId: this.#accountId, sessionEpoch: this.#sessionEpoch, schemaVersion: this.#schema.version, ...(friction ? { friction } : {}) };
  }

  #stop(reason: HostedSessionFriction): HostedSessionFriction {
    this.#stopped ??= reason;
    return this.#stopped;
  }

  #friction(): HostedSessionFriction | null {
    if (this.#stopped) return this.#stopped;
    if (!this.#schema.allowedHosts.includes(this.#location.hostname)) return this.#stop("account-changed");
    for (const [reason, selector] of frictionSelectors) if (this.#document.querySelector(selector)) return this.#stop(reason);
    return null;
  }

  #ownItems(): HostedOwnItem[] | null {
    const root = this.#document.querySelector(this.#schema.historyRoot);
    if (!root) return null;
    const gmailSent = this.#schema.sentLocation?.test(`${this.#location.pathname}${this.#location.hash}`) ?? false;
    const result: HostedOwnItem[] = [];
    for (const element of root.querySelectorAll(this.#schema.item)) {
      const own = gmailSent || element.matches(this.#schema.ownMarker) || element.querySelector(this.#schema.ownMarker) !== null;
      if (!own) continue;
      const id = firstAttribute(element, this.#schema.itemIdAttributes);
      const channelId = firstAttribute(element, this.#schema.channelAttributes) ?? "current-surface";
      const correspondentId = firstAttribute(element, this.#schema.correspondentAttributes) ?? "unknown-correspondent";
      const timestamp = Number(firstAttribute(element, this.#schema.timestampAttributes) ?? "0");
      if (!id || !opaqueId.test(id) || !opaqueId.test(channelId) || !opaqueId.test(correspondentId) || !Number.isSafeInteger(timestamp) || timestamp < 0) {
        this.#stop("schema-drift");
        return null;
      }
      const item: HostedOwnItem = {
        id,
        channelId,
        correspondentId,
        createdAtUnixMs: timestamp,
        contentFingerprint: fingerprint([this.#schema.providerId, id, channelId, correspondentId, String(timestamp)]),
        authoredBySelf: true,
        retractable: element.querySelector(this.#schema.retractableMarker) === null,
      };
      this.#seen.set(id, item);
      result.push(item);
    }
    return result;
  }

  async scrollHistory(bounded: BoundedHistoryRequest): Promise<HostedScrollResult> {
    const friction = this.#friction();
    if (friction) return { ...this.#base(false, friction), complete: false };
    if (!Number.isSafeInteger(bounded.maxScrolls) || bounded.maxScrolls < 1 || bounded.maxScrolls > 10 || !Number.isSafeInteger(bounded.maxItems) || bounded.maxItems < 1 || bounded.maxItems > 500 || !Number.isSafeInteger(bounded.beforeUnixMs) || bounded.beforeUnixMs < 0) {
      return { ...this.#base(false, this.#stop("unknown")), complete: false };
    }
    const root = this.#document.querySelector<HTMLElement>(this.#schema.historyRoot);
    if (!root) return { ...this.#base(false, this.#stop("schema-drift")), complete: false };
    let priorHeight = -1;
    for (let index = 0; index < bounded.maxScrolls; index += 1) {
      const items = this.#ownItems();
      if (items === null) return { ...this.#base(false, this.#stopped ?? "schema-drift"), complete: false };
      if (items.length >= bounded.maxItems || items.some((item) => item.createdAtUnixMs > 0 && item.createdAtUnixMs < bounded.beforeUnixMs)) return { ...this.#base(true), complete: false };
      const nextHeight = root.scrollHeight;
      if (nextHeight === priorHeight) return { ...this.#base(true), complete: true };
      priorHeight = nextHeight;
      root.scrollTo({ top: nextHeight, behavior: "auto" });
    }
    return { ...this.#base(true), complete: false };
  }

  async listOwnItems(): Promise<HostedListResult> {
    const friction = this.#friction();
    if (friction) return { ...this.#base(false, friction), items: [] };
    const items = this.#ownItems();
    if (items === null) return { ...this.#base(false, this.#stopped ?? "schema-drift"), items: [] };
    return { ...this.#base(true), items };
  }

  async deleteOwnItem(id: string): Promise<HostedDeleteResult> {
    const friction = this.#friction();
    if (friction) return { ...this.#base(false, friction), accepted: false };
    if (!opaqueId.test(id)) return { ...this.#base(false, this.#stop("unknown")), accepted: false };
    const items = this.#ownItems();
    if (items === null || !items.some((item) => item.id === id)) return { ...this.#base(false, this.#stop("schema-drift")), accepted: false };
    const element = [...this.#document.querySelectorAll(this.#schema.item)].find((candidate) => firstAttribute(candidate, this.#schema.itemIdAttributes) === id);
    if (!element) return { ...this.#base(false, this.#stop("schema-drift")), accepted: false };
    let action = element.querySelector<HTMLElement>(this.#schema.deleteWithinItem);
    if (!action && this.#schema.deleteToolbar) {
      const checkbox = element.querySelector<HTMLElement>("[role='checkbox'], input[type='checkbox']");
      if (!checkbox) return { ...this.#base(false, this.#stop("schema-drift")), accepted: false };
      checkbox.click();
      action = this.#document.querySelector<HTMLElement>(this.#schema.deleteToolbar);
    }
    if (!action) return { ...this.#base(false, this.#stop("schema-drift")), accepted: false };
    action.click();
    this.#deleteAttempts.add(id);
    return { ...this.#base(true), accepted: true };
  }

  async verifyGone(id: string): Promise<HostedVerifyResult> {
    const friction = this.#friction();
    if (friction) return { ...this.#base(false, friction), gone: false, covered: false };
    if (!opaqueId.test(id) || !this.#deleteAttempts.has(id) || !this.#seen.has(id)) return { ...this.#base(true), gone: false, covered: false };
    const items = this.#ownItems();
    if (items === null) return { ...this.#base(false, this.#stopped ?? "schema-drift"), gone: false, covered: false };
    return { ...this.#base(true), gone: !items.some((item) => item.id === id), covered: true };
  }
}

export class GmailWebScrubPreload extends SemanticProviderPreload {
  constructor(document: Document, location: Location, accountId: string, sessionEpoch: string) { super(document, location, GMAIL_WEB_PRELOAD_SCHEMA, accountId, sessionEpoch); }
}
export class DiscordWebScrubPreload extends SemanticProviderPreload {
  constructor(document: Document, location: Location, accountId: string, sessionEpoch: string) { super(document, location, DISCORD_WEB_PRELOAD_SCHEMA, accountId, sessionEpoch); }
}
export class TelegramWebScrubPreload extends SemanticProviderPreload {
  constructor(document: Document, location: Location, accountId: string, sessionEpoch: string) { super(document, location, TELEGRAM_WEB_PRELOAD_SCHEMA, accountId, sessionEpoch); }
}

