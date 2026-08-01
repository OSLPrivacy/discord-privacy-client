/**
 * Per-device attention state for OSL Spaces.
 *
 * A Space message store supplies records in the order this device accepted
 * them.  This module retains only a local read frontier per channel; it never
 * emits read events or accepts a transport dependency.  Counts are therefore
 * derived from messages already held on this device, not mirrored to a relay
 * or another device.
 */

export interface OslSpaceStoredMessage {
  readonly messageId: string;
  readonly channelId: string;
  /** Strictly increasing receipt order assigned by this device's message store. */
  readonly localSequence: number;
  readonly incoming: boolean;
  readonly mentionsLocalUser: boolean;
}

export interface OslSpacesLocalState {
  readonly readThroughByChannel: ReadonlyMap<string, number>;
}

export interface OslSpaceChannelAttention {
  readonly unreadCount: number;
  readonly mentionCount: number;
}

export function createOslSpacesLocalState(): OslSpacesLocalState {
  return { readThroughByChannel: new Map() };
}

/**
 * Marks every message currently held in one channel as read on this device.
 * The returned state is immutable from the caller's perspective, so callers
 * cannot accidentally modify another device profile's read frontier.
 */
export function markChannelRead(
  messages: readonly OslSpaceStoredMessage[],
  state: OslSpacesLocalState,
  channelId: string,
): OslSpacesLocalState {
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
  messages: readonly OslSpaceStoredMessage[],
  state: OslSpacesLocalState,
  channelId: string,
): OslSpaceChannelAttention {
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
