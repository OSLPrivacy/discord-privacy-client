/** Delete-only commands exposed by a provider preload in OSL's retained service webview. */
export type HostedScrubProviderId = "gmail-web" | "discord" | "telegram-web";
export type HostedSessionFriction = "captcha" | "challenge" | "rate-limit" | "schema-drift" | "signed-out" | "account-changed" | "unknown";

export interface BoundedHistoryRequest {
  readonly maxScrolls: number;
  readonly maxItems: number;
  readonly beforeUnixMs: number;
}

export interface HostedOwnItem {
  readonly id: string;
  readonly channelId: string;
  readonly correspondentId: string;
  readonly createdAtUnixMs: number;
  readonly contentFingerprint: string;
  readonly authoredBySelf: true;
  readonly retractable: boolean;
}

export interface HostedSessionResult {
  readonly ok: boolean;
  readonly accountId: string;
  readonly sessionEpoch: string;
  readonly schemaVersion: string;
  readonly friction?: HostedSessionFriction;
}

export interface HostedScrollResult extends HostedSessionResult {
  readonly complete: boolean;
}

export interface HostedListResult extends HostedSessionResult {
  readonly items: readonly HostedOwnItem[];
}

export interface HostedDeleteResult extends HostedSessionResult {
  /** The provider UI accepted the semantic delete action; this is not proof of deletion. */
  readonly accepted: boolean;
}

export interface HostedVerifyResult extends HostedSessionResult {
  /** True only when the provider UI readback covers this item and no longer finds it. */
  readonly gone: boolean;
  readonly covered: boolean;
}

/**
 * The complete inbound authority of Scrub inside a hosted service page.
 *
 * There is deliberately no invoke/eval/click/input/fetch/send/post/react/join
 * primitive. Provider preload code may implement these four semantic methods
 * against its own current UI schema, but callers cannot select DOM operations.
 */
export interface HostedSessionDeleteOnlyPort {
  scrollHistory(bounded: BoundedHistoryRequest): Promise<HostedScrollResult>;
  listOwnItems(): Promise<HostedListResult>;
  deleteOwnItem(id: string): Promise<HostedDeleteResult>;
  verifyGone(id: string): Promise<HostedVerifyResult>;
}

export interface HostedPortBinding {
  readonly providerId: HostedScrubProviderId;
  readonly accountId: string;
  readonly port: HostedSessionDeleteOnlyPort;
}

export const HOSTED_SESSION_PORT_METHODS = Object.freeze([
  "deleteOwnItem",
  "listOwnItems",
  "scrollHistory",
  "verifyGone",
] as const);

