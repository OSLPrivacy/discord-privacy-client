import type {
  DiscordFriction,
  HostedDiscordDeleteOnlySession,
  HostedDiscordOwnMessage,
} from "./scrub-discord-assisted";

/**
 * This capability is intentionally not active until the retained Discord
 * service webview provides the narrow command port below and a live account
 * confirms its current page schema. It is not valid for the native-window
 * host, an ordinary Discord window, or an arbitrary webview.
 */
export const DISCORD_HOSTED_SCROLL_CAPABILITY = Object.freeze({
  enabledByDefault: false,
  verification: "live-account-required" as const,
  boundaryImplemented: true,
  injectedCommandPortImplemented: false,
  hostIntegration: "delete-only-injected-command-port-required" as const,
  integrationBlocked: Object.freeze({
    code: "hosted-delete-command-port-unavailable" as const,
    detail: "The current host has no Scrub preload/isolated-world port. DOM click automation is prohibited synthetic input, and undocumented Discord internals are not an acceptable deletion transport.",
  }),
  supportsGenericAutomation: false,
  supportsSyntheticInput: false,
  supportsSendReactOrJoin: false,
  supportsProxying: false,
});

export type DiscordHostedScrubCommand =
  | { operation: "scrollOwnHistory"; accountId: string; channelId: string }
  | { operation: "inspectOwnMessage"; accountId: string; channelId: string; messageId: string }
  | { operation: "deleteOwnMessage"; accountId: string; channelId: string; messageId: string; requireAuthoredBySelf: true }
  | { operation: "verifyOwnMessageAbsent"; accountId: string; channelId: string; messageId: string };

/** Implemented by the hosted webview preload/injected world, never by page JS. */
export interface DiscordHostedDeleteOnlyCommandPort {
  invoke(command: DiscordHostedScrubCommand): Promise<unknown>;
}

type BridgeEnvelope = {
  ok: boolean;
  friction?: DiscordFriction;
  schemaVersion?: string;
  complete?: boolean;
  accepted?: boolean;
  absent?: boolean;
  message?: HostedDiscordOwnMessage | null;
  messages?: readonly HostedDiscordOwnMessage[];
};

const frictionValues = new Set<DiscordFriction>([
  "captcha",
  "challenge",
  "rate-signal",
  "dom-schema-drift",
  "unknown",
]);
const opaqueId = /^[A-Za-z0-9][A-Za-z0-9._:@/-]{0,255}$/;

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function friction(value: unknown): DiscordFriction | undefined {
  return typeof value === "string" && frictionValues.has(value as DiscordFriction)
    ? value as DiscordFriction
    : undefined;
}

function message(value: unknown, expectedChannelId: string): HostedDiscordOwnMessage | null {
  const item = record(value);
  if (item === null
    || typeof item.id !== "string" || !opaqueId.test(item.id)
    || item.channelId !== expectedChannelId
    || typeof item.correspondentId !== "string" || !opaqueId.test(item.correspondentId)
    || !Number.isSafeInteger(item.createdAtUnixMs) || (item.createdAtUnixMs as number) < 0
    || typeof item.contentFingerprint !== "string" || item.contentFingerprint.length === 0
    || item.contentFingerprint.length > 512
    || typeof item.authoredBySelf !== "boolean"
    || typeof item.retractable !== "boolean") return null;
  return item as unknown as HostedDiscordOwnMessage;
}

function envelope(value: unknown): BridgeEnvelope | null {
  const item = record(value);
  if (item === null || typeof item.ok !== "boolean") return null;
  const reportedFriction = friction(item.friction);
  if (item.friction !== undefined && reportedFriction === undefined) return null;
  return { ...item, ok: item.ok, friction: reportedFriction } as BridgeEnvelope;
}

/**
 * The main-world adapter has no eval/script/fetch/input primitive. Its only
 * authority is four fixed delete-workflow commands, all bound to one account
 * and an explicit set of channels.
 */
export class DiscordHostedScrollBridgeSession implements HostedDiscordDeleteOnlySession {
  readonly #port: DiscordHostedDeleteOnlyCommandPort;
  readonly #accountId: string;
  readonly #channels: ReadonlySet<string>;
  #stopped: DiscordFriction | null = null;

  constructor(port: DiscordHostedDeleteOnlyCommandPort, accountId: string, channelIds: readonly string[]) {
    if (!opaqueId.test(accountId) || channelIds.length === 0 || channelIds.some((id) => !opaqueId.test(id))) {
      throw new Error("invalid fixed Discord scrub scope");
    }
    this.#port = port;
    this.#accountId = accountId;
    this.#channels = new Set(channelIds);
  }

  #guardChannel(channelId: string): DiscordFriction | null {
    return this.#channels.has(channelId) ? this.#stopped : this.#stop("unknown");
  }

  #stop(reason: DiscordFriction): DiscordFriction {
    this.#stopped ??= reason;
    return this.#stopped;
  }

  async #invoke(command: DiscordHostedScrubCommand): Promise<BridgeEnvelope> {
    if (this.#stopped !== null) return { ok: false, friction: this.#stopped };
    try {
      const result = envelope(await this.#port.invoke(command));
      if (result === null) return { ok: false, friction: this.#stop("dom-schema-drift") };
      if (result.friction !== undefined) this.#stop(result.friction);
      else if (!result.ok) return { ok: false, friction: this.#stop("unknown") };
      return result;
    } catch {
      return { ok: false, friction: this.#stop("unknown") };
    }
  }

  async loadNextOwnMessageHistory(channelId: string): Promise<{ messages: readonly HostedDiscordOwnMessage[]; complete: boolean; friction?: DiscordFriction }> {
    const blocked = this.#guardChannel(channelId);
    if (blocked !== null) return { messages: [], complete: false, friction: blocked };
    const result = await this.#invoke({ operation: "scrollOwnHistory", accountId: this.#accountId, channelId });
    if (result.friction !== undefined) return { messages: [], complete: false, friction: result.friction };
    if (!Array.isArray(result.messages) || typeof result.complete !== "boolean") return { messages: [], complete: false, friction: this.#stop("dom-schema-drift") };
    const messages = result.messages.map((item) => message(item, channelId));
    if (messages.some((item) => item === null)) return { messages: [], complete: false, friction: this.#stop("dom-schema-drift") };
    return { messages: messages as HostedDiscordOwnMessage[], complete: result.complete };
  }

  async inspectOwnMessage(channelId: string, messageId: string): Promise<{ message: HostedDiscordOwnMessage | null; schemaVersion: string; friction?: DiscordFriction }> {
    const blocked = this.#guardChannel(channelId);
    if (blocked !== null || !opaqueId.test(messageId)) return { message: null, schemaVersion: "", friction: blocked ?? this.#stop("unknown") };
    const result = await this.#invoke({ operation: "inspectOwnMessage", accountId: this.#accountId, channelId, messageId });
    if (result.friction !== undefined) return { message: null, schemaVersion: "", friction: result.friction };
    if (typeof result.schemaVersion !== "string" || result.schemaVersion.length === 0 || result.message === undefined) return { message: null, schemaVersion: "", friction: this.#stop("dom-schema-drift") };
    if (result.message === null) return { message: null, schemaVersion: result.schemaVersion };
    const parsed = message(result.message, channelId);
    if (parsed === null || parsed.id !== messageId) return { message: null, schemaVersion: "", friction: this.#stop("dom-schema-drift") };
    return { message: parsed, schemaVersion: result.schemaVersion };
  }

  async deleteOwnMessage(channelId: string, messageId: string): Promise<{ accepted: boolean; friction?: DiscordFriction }> {
    const blocked = this.#guardChannel(channelId);
    if (blocked !== null || !opaqueId.test(messageId)) return { accepted: false, friction: blocked ?? this.#stop("unknown") };
    const result = await this.#invoke({ operation: "deleteOwnMessage", accountId: this.#accountId, channelId, messageId, requireAuthoredBySelf: true });
    if (result.friction !== undefined) return { accepted: false, friction: result.friction };
    if (typeof result.accepted !== "boolean") return { accepted: false, friction: this.#stop("dom-schema-drift") };
    return { accepted: result.accepted };
  }

  async verifyOwnMessageAbsent(channelId: string, messageId: string): Promise<{ absent: boolean; friction?: DiscordFriction }> {
    const blocked = this.#guardChannel(channelId);
    if (blocked !== null || !opaqueId.test(messageId)) return { absent: false, friction: blocked ?? this.#stop("unknown") };
    const result = await this.#invoke({ operation: "verifyOwnMessageAbsent", accountId: this.#accountId, channelId, messageId });
    if (result.friction !== undefined) return { absent: false, friction: result.friction };
    if (typeof result.absent !== "boolean") return { absent: false, friction: this.#stop("dom-schema-drift") };
    return { absent: result.absent };
  }
}
