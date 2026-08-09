export interface EnclaveServerMessage {
  readonly messageId: string;
  readonly channelId: string;
  readonly senderId: string;
  readonly plaintext: string;
  readonly marked?: boolean;
}

/** The reserved channel and author used for the enclave-wide key-change log. */
export const OSL_SYSTEM_AUTHOR_ID = "OSL";
export const OSL_KEY_CHANGES_CHANNEL_ID = "osl-key-changes";
export const OSL_KEY_CHANGES_CHANNEL_NAME = "key-changes";
export const OSL_KEY_CHANGES_CHANNEL_NOTE = "This channel is written by OSL. You cannot post here, and neither can the owner.";

export interface EnclaveServerMember {
  readonly memberId: string;
  readonly displayName: string;
}

export interface EnclaveServerChannel {
  readonly channelId: string;
  readonly name: string;
  /** System channels are authored and retained by OSL, never by an enclave role. */
  readonly kind?: "text" | "osl-key-changes";
}

export interface EnclaveServerSnapshot {
  readonly serverId: string;
  readonly messages: readonly EnclaveServerMessage[];
  readonly members: readonly EnclaveServerMember[];
  readonly channels: readonly EnclaveServerChannel[];
}

export interface EnclaveServerAccessResult<T> {
  readonly requesterName: string;
  readonly refused: boolean;
  readonly refusal: string | null;
  readonly rows: readonly T[];
}

export interface EnclaveServerScreen {
  readonly requesterName: string;
  readonly messageRows: readonly EnclaveServerMessage[];
  readonly memberRows: readonly EnclaveServerMember[];
  readonly channelRows: readonly EnclaveServerChannel[];
  readonly refusals: readonly string[];
}

export type EnclaveServerMutationRefusal =
  | "notMember"
  | "keyChangesOslOnly"
  | "keyChangesUndeletable";

export type EnclaveServerMutationResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: EnclaveServerMutationRefusal };

const OSL_KEY_CHANGES_CHANNEL: EnclaveServerChannel = Object.freeze({
  channelId: OSL_KEY_CHANGES_CHANNEL_ID,
  name: OSL_KEY_CHANGES_CHANNEL_NAME,
  kind: "osl-key-changes",
});

/**
 * Every enclave receives exactly one system-owned key-change channel. A user
 * channel cannot claim this reserved name or id; it is replaced by OSL's
 * immutable record before any consumer sees the channel list.
 */
export function enclaveChannelsWithKeyChanges(
  channels: readonly EnclaveServerChannel[],
): readonly EnclaveServerChannel[] {
  return [
    ...channels.filter((channel) => !isOslKeyChangesChannel(channel)),
    OSL_KEY_CHANGES_CHANNEL,
  ];
}

export function isOslKeyChangesChannel(channel: Pick<EnclaveServerChannel, "channelId" | "name">): boolean {
  return channel.channelId === OSL_KEY_CHANGES_CHANNEL_ID
    || channel.name.trim().toLowerCase() === OSL_KEY_CHANGES_CHANNEL_NAME;
}

export function isOslKeyChangesChannelId(channelId: string): boolean {
  return channelId === OSL_KEY_CHANGES_CHANNEL_ID;
}

function requesterIsMember(snapshot: EnclaveServerSnapshot, requesterName: string): boolean {
  return snapshot.members.some((member) => member.displayName === requesterName);
}

/**
 * The app-state gate for all human sends. Roles are intentionally absent:
 * neither an owner nor any other human identity can write this OSL record.
 */
export function requestEnclaveMessagePost(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
  channelId: string,
): EnclaveServerMutationResult {
  if (!requesterIsMember(snapshot, requesterName)) return { ok: false, reason: "notMember" };
  if (isOslKeyChangesChannelId(channelId)) return { ok: false, reason: "keyChangesOslOnly" };
  return { ok: true };
}

/** The OSL key-change channel has no deletion path, regardless of role. */
export function requestEnclaveChannelDeletion(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
  channelId: string,
): EnclaveServerMutationResult {
  if (!requesterIsMember(snapshot, requesterName)) return { ok: false, reason: "notMember" };
  if (isOslKeyChangesChannelId(channelId)) return { ok: false, reason: "keyChangesUndeletable" };
  return { ok: true };
}

function refused<T>(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
  listName: "message list" | "member list" | "channel list",
): EnclaveServerAccessResult<T> {
  return {
    requesterName,
    refused: true,
    refusal: `${requesterName} was never a member of ${snapshot.serverId}; ${listName} refused`,
    rows: [],
  };
}

function allowed<T>(requesterName: string, rows: readonly T[]): EnclaveServerAccessResult<T> {
  return { requesterName, refused: false, refusal: null, rows };
}

export function requestEnclaveServerMessages(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerMessage> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "message list");
  }
  return allowed(requesterName, snapshot.messages.filter((message) => (
    !isOslKeyChangesChannelId(message.channelId) || message.senderId === OSL_SYSTEM_AUTHOR_ID
  )));
}

export function requestEnclaveServerMembers(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerMember> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "member list");
  }
  return allowed(requesterName, snapshot.members);
}

export function requestEnclaveServerChannels(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerChannel> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "channel list");
  }
  return allowed(requesterName, enclaveChannelsWithKeyChanges(snapshot.channels));
}

export function enclaveServerScreen(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerScreen {
  const messages = requestEnclaveServerMessages(snapshot, requesterName);
  const members = requestEnclaveServerMembers(snapshot, requesterName);
  const channels = requestEnclaveServerChannels(snapshot, requesterName);
  return {
    requesterName,
    messageRows: messages.rows,
    memberRows: members.rows,
    channelRows: channels.rows,
    refusals: [messages.refusal, members.refusal, channels.refusal].filter((row): row is string => row !== null),
  };
}
