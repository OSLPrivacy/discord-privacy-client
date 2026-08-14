import { OSL_KEY_CHANGES_CHANNEL_ID } from "./osl-enclaves-server-access";

/**
 * Per-device attention state for OSL Enclaves.
 *
 * A Enclave message store supplies records in the order this device accepted
 * them.  This module retains only a local read frontier per channel; it never
 * emits read events or accepts a transport dependency.  Counts are therefore
 * derived from messages already held on this device, not mirrored to a relay
 * or another device.
 */

export interface OslEnclaveStoredMessage {
  readonly messageId: string;
  readonly channelId: string;
  /** Strictly increasing receipt order assigned by this device's message store. */
  readonly localSequence: number;
  readonly incoming: boolean;
  readonly mentionsLocalUser: boolean;
}

export interface OslEnclavesLocalState {
  readonly readThroughByChannel: ReadonlyMap<string, number>;
}

export interface OslEnclaveChannelAttention {
  readonly unreadCount: number;
  readonly mentionCount: number;
}

export function createOslEnclavesLocalState(): OslEnclavesLocalState {
  return { readThroughByChannel: new Map() };
}

/**
 * Marks every message currently held in one channel as read on this device.
 * The returned state is immutable from the caller's perspective, so callers
 * cannot accidentally modify another device profile's read frontier.
 */
export function markChannelRead(
  messages: readonly OslEnclaveStoredMessage[],
  state: OslEnclavesLocalState,
  channelId: string,
): OslEnclavesLocalState {
  const current = state.readThroughByChannel.get(channelId) ?? 0;
  let readThrough = current;
  for (const message of messages) {
    if (message.channelId === channelId && validSequence(message.localSequence)) {
      readThrough = Math.max(readThrough, message.localSequence);
    }
  }
  if (readThrough === current) return state;
  const next = new Map(state.readThroughByChannel);
  next.set(channelId, readThrough);
  return { readThroughByChannel: next };
}

/** Derives a channel's unread and direct-mention counts from the local store. */
export function channelAttention(
  messages: readonly OslEnclaveStoredMessage[],
  state: OslEnclavesLocalState,
  channelId: string,
): OslEnclaveChannelAttention {
  const readThrough = state.readThroughByChannel.get(channelId) ?? 0;
  let unreadCount = 0;
  let mentionCount = 0;
  const seenMessageIds = new Set<string>();

  for (const message of messages) {
    if (
      message.channelId !== channelId
      || !message.incoming
      || !validSequence(message.localSequence)
      || message.localSequence <= readThrough
      || seenMessageIds.has(message.messageId)
    ) continue;

    seenMessageIds.add(message.messageId);
    unreadCount += 1;
    if (message.mentionsLocalUser) mentionCount += 1;
  }
  return { unreadCount, mentionCount };
}

function validSequence(value: number): boolean {
  return Number.isSafeInteger(value) && value > 0;
}

/**
 * Per-device Enclave preferences. The identifiers live in this module's private
 * store rather than on the handle, so serializing application state for a
 * relay or another member cannot accidentally disclose a hide/block/mute list.
 */
class EnclaveLocalFilterHandle {}

export type EnclaveLocalFilters = EnclaveLocalFilterHandle;

export interface SpaceMessage {
  readonly id: string;
  readonly channelId: string;
  readonly senderId: string;
}

export interface OslEnclaveThread {
  readonly threadId: string;
  readonly channelName: string;
}

export interface OslEnclaveThreadMessage {
  readonly messageId: string;
  readonly threadId: string;
  readonly channelName: string;
  readonly body: string;
}

export interface OslEnclaveThreadStore {
  readonly threads: ReadonlyMap<string, OslEnclaveThread>;
  readonly messages: readonly OslEnclaveThreadMessage[];
}

export type OslEnclaveThreadRefusal =
  | { readonly ok: false; readonly reason: "unknownThread" }
  | { readonly ok: false; readonly reason: "threadChannelMismatch"; readonly actualChannelName: string };

export type OslEnclaveThreadReadResult =
  | { readonly ok: true; readonly messages: readonly OslEnclaveThreadMessage[] }
  | OslEnclaveThreadRefusal;

export type OslEnclaveThreadAppendResult =
  | { readonly ok: true; readonly store: OslEnclaveThreadStore }
  | OslEnclaveThreadRefusal;

export function createOslEnclaveThreadStore(
  threads: readonly OslEnclaveThread[],
  messages: readonly OslEnclaveThreadMessage[] = [],
): OslEnclaveThreadStore {
  const byId = new Map<string, OslEnclaveThread>();
  for (const thread of threads) {
    requireLocalId(thread.threadId);
    requireLocalId(thread.channelName);
    if (byId.has(thread.threadId)) throw new Error("Duplicate Enclave thread");
    byId.set(thread.threadId, { threadId: thread.threadId, channelName: thread.channelName });
  }
  for (const message of messages) {
    requireThreadMessage(message);
    const owner = byId.get(message.threadId);
    if (!owner) throw new Error("Unknown Enclave thread message");
    if (owner.channelName !== message.channelName) throw new Error("Enclave thread message channel mismatch");
  }
  return { threads: byId, messages: messages.map((message) => ({ ...message })) };
}

function requireThreadMessage(message: OslEnclaveThreadMessage): void {
  requireLocalId(message.messageId);
  requireLocalId(message.threadId);
  requireLocalId(message.channelName);
}

function requireThreadChannel(
  store: OslEnclaveThreadStore,
  threadId: string,
  channelName: string,
): { readonly ok: true; readonly thread: OslEnclaveThread } | OslEnclaveThreadRefusal {
  requireLocalId(threadId);
  requireLocalId(channelName);
  const thread = store.threads.get(threadId);
  if (!thread) return { ok: false, reason: "unknownThread" };
  if (thread.channelName !== channelName) {
    return { ok: false, reason: "threadChannelMismatch", actualChannelName: thread.channelName };
  }
  return { ok: true, thread };
}

export function readOslEnclaveThreadMessages(
  store: OslEnclaveThreadStore,
  channelName: string,
  threadId: string,
): OslEnclaveThreadReadResult {
  const thread = requireThreadChannel(store, threadId, channelName);
  if (!thread.ok) return thread;
  return {
    ok: true,
    messages: store.messages.filter((message) => (
      message.threadId === threadId && message.channelName === channelName
    )),
  };
}

export function appendOslEnclaveThreadMessage(
  store: OslEnclaveThreadStore,
  message: OslEnclaveThreadMessage,
): OslEnclaveThreadAppendResult {
  requireThreadMessage(message);
  const thread = requireThreadChannel(store, message.threadId, message.channelName);
  if (!thread.ok) return thread;
  return {
    ok: true,
    store: { threads: new Map(store.threads), messages: [...store.messages, { ...message }] },
  };
}

interface FilterState {
  readonly hiddenChannelIds: readonly string[];
  readonly blockedMemberIds: readonly string[];
  readonly mutedChannelIds: readonly string[];
}

const EMPTY_STATE: FilterState = Object.freeze({
  hiddenChannelIds: Object.freeze([]),
  blockedMemberIds: Object.freeze([]),
  mutedChannelIds: Object.freeze([]),
});
const states = new WeakMap<EnclaveLocalFilters, FilterState>();

function filtersWith(state: FilterState): EnclaveLocalFilters {
  const filters = new EnclaveLocalFilterHandle();
  states.set(filters, state);
  return filters;
}

const EMPTY_FILTERS = filtersWith(EMPTY_STATE);

export function emptyEnclaveLocalFilters(): EnclaveLocalFilters {
  return EMPTY_FILTERS;
}

function stateOf(filters: EnclaveLocalFilters): FilterState {
  const state = states.get(filters);
  if (!state) throw new Error("Unknown Enclave local filters handle");
  return state;
}

function requireLocalId(id: string): string {
  if (id.trim().length === 0) throw new Error("Enclave filter identifiers must not be blank");
  return id;
}

function include(ids: readonly string[], id: string): readonly string[] {
  return ids.includes(id) ? ids : [...ids, id];
}

export function hideSpaceChannel(filters: EnclaveLocalFilters, channelId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  // The OSL-authored audit channel must remain discoverable even on a device
  // that hides ordinary channels.
  if (id === OSL_KEY_CHANGES_CHANNEL_ID) return filters;
  return filtersWith({ ...state, hiddenChannelIds: include(state.hiddenChannelIds, id) });
}

export function unhideSpaceChannel(filters: EnclaveLocalFilters, channelId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, hiddenChannelIds: state.hiddenChannelIds.filter((candidate) => candidate !== id) });
}

/** Blocking changes only this client's display and notification decisions. */
export function blockSpaceMember(filters: EnclaveLocalFilters, memberId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(memberId);
  return filtersWith({ ...state, blockedMemberIds: include(state.blockedMemberIds, id) });
}

export function unblockSpaceMember(filters: EnclaveLocalFilters, memberId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(memberId);
  return filtersWith({ ...state, blockedMemberIds: state.blockedMemberIds.filter((candidate) => candidate !== id) });
}

/** Muting leaves content available but stops this device from notifying about it. */
export function muteSpaceChannel(filters: EnclaveLocalFilters, channelId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, mutedChannelIds: include(state.mutedChannelIds, id) });
}

export function unmuteSpaceChannel(filters: EnclaveLocalFilters, channelId: string): EnclaveLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, mutedChannelIds: state.mutedChannelIds.filter((candidate) => candidate !== id) });
}

export function visibleSpaceChannels(filters: EnclaveLocalFilters, channelIds: readonly string[]): string[] {
  const hidden = new Set(stateOf(filters).hiddenChannelIds);
  return channelIds.filter((channelId) => !hidden.has(channelId));
}

export function visibleSpaceMessages<T extends SpaceMessage>(filters: EnclaveLocalFilters, messages: readonly T[]): T[] {
  const blocked = new Set(stateOf(filters).blockedMemberIds);
  return messages.filter((message) => !blocked.has(message.senderId));
}

export function shouldNotifyForSpaceMessage(filters: EnclaveLocalFilters, message: SpaceMessage): boolean {
  const state = stateOf(filters);
  return !state.blockedMemberIds.includes(message.senderId)
    && !state.hiddenChannelIds.includes(message.channelId)
    && !state.mutedChannelIds.includes(message.channelId);
}
